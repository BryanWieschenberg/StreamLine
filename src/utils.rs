use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, MutexGuard};
use std::io;
use std::io::{Write};
use std::time::{SystemTime, UNIX_EPOCH};

use colored::Colorize;

use crate::types::{Clients, Client, ClientState, Rooms, Room, USERS_LOCK, ROOMS_LOCK};
use crate::commands::command_utils::{save_rooms_to_disk, ColorizeExt};

pub fn broadcast_message(clients: &Clients, room_name: &str, sender: &str, msg: &str, include_sender: bool, bypass_ignores: bool) -> io::Result<()> {
    let client_arcs: Vec<Arc<Mutex<Client>>> =
        match lock_clients(clients) {
            Ok(map) => map.values().cloned().collect(),
            Err(_)  => return Ok(()),
        };

    for arc in client_arcs {
        let mut c = lock_client(&arc)?;
        
        if let ClientState::InRoom { username, room, .. } = &c.state {
            if room != room_name { continue; }

            if !include_sender && username == sender {
                continue;
            }

            if !bypass_ignores && c.ignore_list.contains(&sender.to_string()) {
                continue;
            }

            writeln!(c.stream, "{msg}")?;
        }
    }
    Ok(())
}

pub fn check_mute(rooms: &Rooms, room: &str, username: &str) -> io::Result<Option<String>> {
    let room_arc = {
        let rooms_map = lock_rooms(rooms)?;
        match rooms_map.get(room) {
            Some(r) => Arc::clone(r),
            None => return Ok(Some("Room not found".to_string())),
        }
    };

    let mut need_save = false;
    let mut still_muted_msg = None;

    {
        let mut rg = lock_room(&room_arc)?;

        if let Some(rec) = rg.users.get_mut(username) {
            if rec.muted {
                let now = match SystemTime::now().duration_since(UNIX_EPOCH) {
                    Ok(dur) => dur.as_secs(),
                    Err(_) => 0,
                };

                let still_muted = if rec.mute_length == 0 {
                    true
                } else {
                    now < rec.mute_stamp.saturating_add(rec.mute_length)
                };

                if still_muted {
                    let remaining = if rec.mute_length == 0 {
                        "Permanent".to_string()
                    } else {
                        let mut rem = rec.mute_stamp + rec.mute_length - now;
                        let d = rem / 86_400;
                        rem %= 86_400;
                        let h = rem / 3_600;
                        rem %= 3_600;
                        let m = rem / 60;
                        let s = rem % 60;
                        format!("{d}d {h}h {m}m {s}s left")
                    };
                    still_muted_msg = Some(if rec.mute_reason.is_empty() {
                        format!("You are muted ({remaining})")
                    } else {
                        format!("You are muted: {}\n> {remaining}", rec.mute_reason)
                    });
                } else {
                    // Mute expired, lift it
                    rec.muted = false;
                    rec.mute_stamp = 0;
                    rec.mute_length = 0;
                    rec.mute_reason.clear();
                    need_save = true;
                }
            }
        }
    }

    if need_save {
        let rooms_map = lock_rooms(rooms)?;
        if let Err(e) = save_rooms_to_disk(&rooms_map) {
            eprintln!("Failed to save rooms: {e}");
        }

    }

    Ok(still_muted_msg)
}

pub fn format_broadcast(rooms: &Rooms, room_name: &str, username: &str) -> io::Result<(String, String)> {
    let rooms_map = lock_rooms(rooms)?;
    let room_arc = match rooms_map.get(room_name) {
        Some(r) => Arc::clone(r),
        None => return Ok(("".to_string(), username.green().to_string())),
    };

    let rg = lock_room(&room_arc)?;
    let user_info = rg.users.get(username);

    let mut prefix_colored = "".to_string();
    let mut display_name = username.green().to_string();

    if let Some(info) = user_info {
        // Role prefix and color
        let role_key = info.role.to_lowercase();
        if let Some(hex) = rg.roles.colors.get(&role_key) {
            let prefix = match role_key.as_str() {
                "owner" => "[Owner]",
                "admin" => "[Admin]",
                "mod" | "moderator" => "[Mod]",
                _ => "[User]",
            };
            prefix_colored = prefix.truecolor_from_hex(hex).to_string();
        }

        // Nick italicized or fallback to username
        if !info.nick.is_empty() {
            if !info.color.is_empty() {
                display_name = info.nick.as_str().truecolor_from_hex(&info.color).italic().to_string();
            } else {
                display_name = info.nick.italic().to_string();
            }
        } else if !info.color.is_empty() {
            display_name = username.truecolor_from_hex(&info.color).to_string();
        }
    }

    Ok((prefix_colored, display_name))
}

pub fn lock_clients(clients: &Clients) -> std::io::Result<std::sync::MutexGuard<'_, HashMap<SocketAddr, Arc<Mutex<Client>>>>> {
    clients.lock().map_err(|e| {
        eprintln!("Failed to lock clients: {e}");
        std::io::Error::new(std::io::ErrorKind::Other, "Lock poisoned")
    })
}

pub fn lock_client(client_arc: &Arc<Mutex<Client>>) -> std::io::Result<std::sync::MutexGuard<'_, Client>> {
    client_arc.lock().map_err(|e| {
        eprintln!("Failed to lock client: {e}");
        std::io::Error::new(std::io::ErrorKind::Other, "Lock poisoned")
    })
}

pub fn lock_rooms(rooms: &Rooms) -> std::io::Result<std::sync::MutexGuard<'_, HashMap<String, Arc<Mutex<Room>>>>> {
    rooms.lock().map_err(|e| {
        eprintln!("Failed to lock rooms: {e}");
        std::io::Error::new(std::io::ErrorKind::Other, "Lock poisoned")
    })
}

pub fn lock_room(room_arc: &Arc<Mutex<Room>>) -> std::io::Result<std::sync::MutexGuard<'_, Room>> {
    room_arc.lock().map_err(|e| {
        eprintln!("Failed to lock room: {e}");
        std::io::Error::new(std::io::ErrorKind::Other, "Lock poisoned")
    })
}

pub fn lock_users_storage<'a>() -> io::Result<MutexGuard<'a, ()>> {
    USERS_LOCK.lock().map_err(|e| {
        eprintln!("Failed to lock users: {e}");
        io::Error::new(io::ErrorKind::Other, "Error: Could not acquire user lock")
    })
}

pub fn lock_rooms_storage<'a>() -> io::Result<MutexGuard<'a, ()>> {
    ROOMS_LOCK.lock().map_err(|e| {
        eprintln!("Failed to lock rooms: {e}");
        io::Error::new(io::ErrorKind::Other, "Error: Could not acquire room lock")
    })
}
