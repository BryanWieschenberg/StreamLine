#![allow(dead_code)]
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, MutexGuard};
use std::io;
use std::io::{Write};
use std::time::{SystemTime, UNIX_EPOCH};
use colored::Colorize;
use crate::shared::types::{Client, ClientState, Clients, Room, Rooms, ROOMS_LOCK, USERS_LOCK};

pub trait ColorizeExt {
    fn truecolor_from_hex(self, hex: &str) -> colored::ColoredString;
}

impl ColorizeExt for &str {
    fn truecolor_from_hex(self, hex: &str) -> colored::ColoredString {
        self.to_string().truecolor_from_hex(hex)
    }
}

impl ColorizeExt for String {
    fn truecolor_from_hex(self, hex: &str) -> colored::ColoredString {
        let hex = hex.trim_start_matches('#');
        if hex.len() != 6 {
            return self.normal();
        }
        let r = u8::from_str_radix(&hex[0..2], 16).map_or(255, |v| v);
        let g = u8::from_str_radix(&hex[2..4], 16).map_or(255, |v| v);
        let b = u8::from_str_radix(&hex[4..6], 16).map_or(255, |v| v);
        self.truecolor(r, g, b)
    }
}

pub fn save_rooms_to_disk(map: &HashMap<String, Arc<Mutex<Room>>>) -> std::io::Result<()> {
    let _lock = lock_rooms_storage()?;

    let mut snapshot = HashMap::new();
    for (name, arc) in map.iter() {
        if let Ok(room) = arc.lock() {
            snapshot.insert(name.clone(), room.clone());
        } else {
            eprintln!("Failed to lock room '{name}'");
        }
    }
    let file = std::fs::OpenOptions::new().create(true).write(true).truncate(true).open("data/rooms.json")?;
    let mut writer = std::io::BufWriter::new(file);
    let formatter = serde_json::ser::PrettyFormatter::with_indent(b"    ");
    let mut ser = serde_json::Serializer::with_formatter(&mut writer, formatter);
    serde::Serialize::serialize(&snapshot, &mut ser).map_err(io::Error::other)?;

    Ok(())
}

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
        None => return Ok(("".to_string(), username.to_string())),
    };

    let rg = lock_room(&room_arc)?;
    let user_info = rg.users.get(username);

    let mut prefix_colored = "".to_string();
    let mut display_name = username.to_string();

    if let Some(info) = user_info {
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
        std::io::Error::other("Lock poisoned")
    })
}

pub fn lock_client(client_arc: &Arc<Mutex<Client>>) -> std::io::Result<std::sync::MutexGuard<'_, Client>> {
    client_arc.lock().map_err(|e| {
        eprintln!("Failed to lock client: {e}");
        std::io::Error::other("Lock poisoned")
    })
}

pub fn lock_rooms(rooms: &Rooms) -> std::io::Result<std::sync::MutexGuard<'_, HashMap<String, Arc<Mutex<Room>>>>> {
    rooms.lock().map_err(|e| {
        eprintln!("Failed to lock rooms: {e}");
        std::io::Error::other("Lock poisoned")
    })
}

pub fn lock_room(room_arc: &Arc<Mutex<Room>>) -> std::io::Result<std::sync::MutexGuard<'_, Room>> {
    room_arc.lock().map_err(|e| {
        eprintln!("Failed to lock room: {e}");
        std::io::Error::other("Lock poisoned")
    })
}

pub fn lock_users_storage<'a>() -> io::Result<MutexGuard<'a, ()>> {
    USERS_LOCK.lock().map_err(|e| {
        eprintln!("Failed to lock users: {e}");
        io::Error::other("Error: Could not acquire user lock")
    })
}

pub fn lock_rooms_storage<'a>() -> io::Result<MutexGuard<'a, ()>> {
    ROOMS_LOCK.lock().map_err(|e| {
        eprintln!("Failed to lock rooms: {e}");
        io::Error::other("Error: Could not acquire room lock")
    })
}

pub fn load_json(path: &str) -> io::Result<serde_json::Value> {
    let file = std::fs::File::open(path)?;
    let reader = std::io::BufReader::new(file);
    let json: serde_json::Value = serde_json::from_reader(reader)?;
    Ok(json)
}

pub fn save_json(path: &str, data: &serde_json::Value) -> io::Result<()> {
    let file = std::fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(path)?;
    let mut writer = std::io::BufWriter::new(file);
    let formatter = serde_json::ser::PrettyFormatter::with_indent(b"    ");
    let mut ser = serde_json::Serializer::with_formatter(&mut writer, formatter);
    serde::Serialize::serialize(data, &mut ser)?;
    Ok(())
}

pub fn send_message(client_arc: &Arc<Mutex<Client>>, msg: &str) -> io::Result<()> {
    let mut c = lock_client(client_arc)?;
    writeln!(c.stream, "{msg}")?;
    c.stream.flush()?;
    Ok(())
}

pub fn send_message_locked(client: &mut Client, msg: &str) -> io::Result<()> {
    writeln!(client.stream, "{msg}")?;
    client.stream.flush()?;
    Ok(())
}

pub fn send_error(client_arc: &Arc<Mutex<Client>>, msg: &str) -> io::Result<()> {
    let mut c = lock_client(client_arc)?;
    writeln!(c.stream, "{}", msg.red())?;
    c.stream.flush()?;
    Ok(())
}

pub fn send_error_locked(client: &mut Client, msg: &str) -> io::Result<()> {
    writeln!(client.stream, "{}", msg.red())?;
    client.stream.flush()?;
    Ok(())
}

pub fn send_success(client_arc: &Arc<Mutex<Client>>, msg: &str) -> io::Result<()> {
    let mut c = lock_client(client_arc)?;
    writeln!(c.stream, "{}", msg.green())?;
    c.stream.flush()?;
    Ok(())
}

pub fn send_success_locked(client: &mut Client, msg: &str) -> io::Result<()> {
    writeln!(client.stream, "{}", msg.green())?;
    client.stream.flush()?;
    Ok(())
}
