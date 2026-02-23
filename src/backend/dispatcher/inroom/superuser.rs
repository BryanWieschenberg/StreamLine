use std::io::{self, BufReader, BufWriter, Write};
use std::sync::{Arc, Mutex};
use serde_json::{json, Serializer};
use serde::Serialize;
use serde_json::ser::PrettyFormatter;
use std::fs::OpenOptions;
use colored::*;

use crate::shared::types::{Client, ClientState, Clients, Rooms};
use crate::shared::utils::{lock_client, lock_clients, lock_rooms, lock_room, send_success, send_error, send_message, save_rooms_to_disk, ColorizeExt, send_message_locked, send_error_locked, send_success_locked, broadcast_room_list_to_all};
use crate::backend::dispatcher::CommandResult;

pub fn handle_super_users(client: Arc<Mutex<Client>>, clients: &Clients, rooms: &Rooms, room: &String) -> io::Result<CommandResult> {
    let mut status_map = std::collections::HashMap::new();
    {
        let clients_map = lock_clients(clients)?;
        for c_arc in clients_map.values() {
            if let Ok(target_c) = c_arc.try_lock() {
                if let ClientState::InRoom { username, room: rnm, is_afk, room_time, .. } = &target_c.state {
                    if rnm == room {
                        let secs = room_time.and_then(|t| t.elapsed().ok()).map(|d| d.as_secs()).unwrap_or(0);
                        status_map.insert(username.clone(), (*is_afk, secs));
                    }
                }
            }
        }
    }

    let rooms_map = lock_rooms(rooms)?;
    let room_arc = match rooms_map.get(room) {
        Some(r) => Arc::clone(r),
        None => {
            let mut c = lock_client(&client)?;
            send_message_locked(&mut c, &format!("Room {room} not found").yellow().to_string())?;
            return Ok(CommandResult::Handled);
        }
    };
    let room_guard = lock_room(&room_arc)?;
    let mut c = lock_client(&client)?;

    writeln!(c.stream, "{}", format!("User data for {room}:").green())?;
    c.stream.flush()?;

    for (uname, udata) in &room_guard.users {
        if !room_guard.online_users.contains(uname) {
            continue;
        }

        let role = {
            let mut ch = udata.role.chars();
            match ch.next() {
                Some(f) => f.to_uppercase().collect::<String>() + ch.as_str(),
                None => String::new(),
            }
        };

        let color_display = if udata.color.is_empty() {
            "Default".to_string()
        } else {
            udata.color.to_string().truecolor_from_hex(&udata.color).to_string()
        };

        let nickname = if udata.nick.is_empty() {
            "None".to_string()
        } else {
            udata.nick.italic().to_string()
        };

        let hidden_status = if udata.hidden {
            "True".yellow().to_string()
        } else {
            "False".green().to_string()
        };

        let (afk, secs) = status_map.get(uname).cloned().unwrap_or((false, 0));

        let afk_status = if afk {
            "True".yellow().to_string()
        } else {
            "False".green().to_string()
        };

        let session_time = {
            let h = secs / 3600;
            let m = (secs % 3600) / 60;
            let s = secs % 60;
            format!("{h:0>2}:{m:0>2}:{s:0>2}")
        };

        writeln!(c.stream, "> {} - Role: {}, Nickname: {}, Color: {}, Hidden: {}, AFK: {}, Session: {}",
            uname.green(),
            role,
            nickname,
            color_display,
            hidden_status,
            afk_status,
            session_time
        )?;
        c.stream.flush()?;
    }

    Ok(CommandResult::Handled)
}

pub fn handle_super_rename(client: Arc<Mutex<Client>>, clients: &Clients, rooms: &Rooms, room: &String, new_name: &String) -> io::Result<CommandResult> {
    let old_name = room.clone();

    {
        let clients_map = lock_clients(clients)?;
        let mut rooms_map = lock_rooms(rooms)?;

        if rooms_map.contains_key(new_name) {
            let mut c = lock_client(&client)?;
            send_message_locked(&mut c, &format!("Room name '{new_name}' is already taken").yellow().to_string())?;
            return Ok(CommandResult::Handled);
        }

        let room_arc = match rooms_map.remove(&old_name) {
            Some(r) => r,
            None => {
                let mut c = lock_client(&client)?;
                send_message_locked(&mut c, &format!("Room '{old_name}' not found").yellow().to_string())?;
                return Ok(CommandResult::Handled);
            }
        };

        rooms_map.insert(new_name.clone(), Arc::clone(&room_arc));

        for c_arc in clients_map.values() {
            if let Ok(mut target_c) = c_arc.try_lock() {
                if let ClientState::InRoom { room: r, .. } = &mut target_c.state {
                    if r == &old_name {
                        *r = new_name.clone();
                        let _ = writeln!(target_c.stream, "/ROOM_NAME {new_name}");
                        let _ = target_c.stream.flush();
                    }
                }
            }
        }

        if let Err(e) = save_rooms_to_disk(&rooms_map) {
            let mut c = lock_client(&client)?;
            send_error_locked(&mut c, &format!("Failed to save rooms: {e}"))?;
            return Ok(CommandResult::Handled);
        }
    }

    let mut c = lock_client(&client)?;
    send_success_locked(&mut c, &format!("Room renamed from '{old_name}' to '{new_name}'"))?;
    Ok(CommandResult::Handled)
}

pub fn handle_super_export(client: Arc<Mutex<Client>>, _rooms: &Rooms, room: &String, filename: &String) -> io::Result<CommandResult> {
    let file = match std::fs::File::open("data/rooms.json") {
        Ok(f)  => f,
        Err(e) => {
            send_error(&client, &format!("Error opening rooms.json: {e}"))?;
            return Ok(CommandResult::Handled);
        }
    };
    
    let reader = BufReader::new(file);
    let rooms_json: serde_json::Value = match serde_json::from_reader(reader) {
        Ok(v)  => v,
        Err(e) => {
            send_error(&client, &format!("Malformed rooms.json: {e}"))?;
            return Ok(CommandResult::Handled);
        }
    };

    let room_data = match rooms_json.get(room) {
        Some(v) => v.clone(),
        None => {
            send_message(&client, &"Error: room data not found".yellow().to_string())?;
            return Ok(CommandResult::Handled);
        }
    };

    let final_filename = if filename.is_empty() {
        let stamp = chrono::Local::now().format("%y%m%d%H%M%S").to_string();
        format!("{room}_{stamp}.json")
    } else if filename.ends_with(".json") {
        filename.clone()
    } else {
        format!("{filename}.json")
    };

    let export_path = format!("data/vault/rooms/{final_filename}");
    let export_file = match OpenOptions::new().create(true).write(true).truncate(true).open(&export_path) {
        Ok(f)  => f,
        Err(e) => {
            send_error(&client, &format!("Error creating {export_path}: {e}"))?;
            return Ok(CommandResult::Handled);
        }
    };

    let mut writer = BufWriter::new(export_file);
    let formatter = PrettyFormatter::with_indent(b"    ");
    let mut ser = Serializer::with_formatter(&mut writer, formatter);
    json!({ room: room_data }).serialize(&mut ser)?;

    send_success(&client, &format!("Exported room data to: {final_filename}"))?;
    Ok(CommandResult::Handled)
}

pub fn handle_super_whitelist(client: Arc<Mutex<Client>>, rooms: &Rooms, room: &String) -> io::Result<CommandResult> {
    let rooms_map = lock_rooms(rooms)?;
    let room_arc = match rooms_map.get(room) {
        Some(r) => Arc::clone(r),
        None => {
            let mut c = lock_client(&client)?;
            send_message_locked(&mut c, &format!("Room {room} not found").yellow().to_string())?;
            return Ok(CommandResult::Handled);
        }
    };
    let room_guard = lock_room(&room_arc)?;
    let mut c = lock_client(&client)?;

    if room_guard.whitelist_enabled {
        send_success_locked(&mut c, "- Whitelist is currently ENABLED -")?;
        if room_guard.whitelist.is_empty() {
            send_success_locked(&mut c, "  > No users are currently whitelisted")?;
        } else {
            send_success_locked(&mut c, "Whitelisted users:")?;
            for user in &room_guard.whitelist {
                send_message_locked(&mut c, &format!("  > {}", user.cyan()))?;
            }
        }
    } else {
        send_success_locked(&mut c, "- Whitelist is currently DISABLED -")?;
    }

    Ok(CommandResult::Handled)
}

pub fn handle_super_whitelist_toggle(client: Arc<Mutex<Client>>, clients: &Clients, rooms: &Rooms, room: &String) -> io::Result<CommandResult> {
    let rooms_map = lock_rooms(rooms)?;
    let room_arc = match rooms_map.get(room) {
        Some(r) => Arc::clone(r),
        None => {
            let mut c = lock_client(&client)?;
            send_message_locked(&mut c, &format!("Room {room} not found").yellow().to_string())?;
            return Ok(CommandResult::Handled);
        }
    };

    let (enabled_now, whitelist) = {
        let mut room_guard = lock_room(&room_arc)?;
        room_guard.whitelist_enabled = !room_guard.whitelist_enabled;
        let enabled = room_guard.whitelist_enabled;
        let wl = room_guard.whitelist.clone();
        drop(room_guard);
        (enabled, wl)
    };

    drop(rooms_map);

    {
        let fresh_map = lock_rooms(rooms)?;
        if let Err(e) = save_rooms_to_disk(&fresh_map) {
            let mut c = lock_client(&client)?;
            send_error_locked(&mut c, &format!("Failed to save rooms: {e}"))?;
            return Ok(CommandResult::Handled);
        }
    };

    if enabled_now {
        let clients_map = lock_clients(clients)?;
        for c_arc in clients_map.values() {
            if let Ok(mut target_c) = c_arc.try_lock() {
                let (should_kick, u_clone) = if let ClientState::InRoom { username: u, room: r, .. } = &target_c.state {
                    if r == room && !whitelist.contains(u) {
                        let room_guard = lock_room(&room_arc)?;
                        let is_owner = room_guard.users.get(u).map(|ud| ud.role == "owner").unwrap_or(false);
                        drop(room_guard);
                        (!is_owner, Some(u.clone()))
                    } else {
                        (false, None)
                    }
                } else {
                    (false, None)
                };

                if should_kick {
                    if let Some(uname) = u_clone {
                        let _ = writeln!(target_c.stream, "/LOBBY_STATE");
                        let _ = writeln!(target_c.stream, "{}", format!("The whitelist for '{room}' has been enabled, and you are not whitelisted.").red());
                        target_c.state = ClientState::LoggedIn { username: uname };
                    }
                }
            }
        }
    }

    let mut c = lock_client(&client)?;
    if enabled_now {
        send_success_locked(&mut c, "Whitelist is now ENABLED")?;
    } else {
        send_success_locked(&mut c, "Whitelist is now DISABLED")?;
    }
    drop(c);

    let _ = broadcast_room_list_to_all(clients, rooms);

    Ok(CommandResult::Handled)
}

pub fn handle_super_whitelist_add(client: Arc<Mutex<Client>>, clients: &Clients, rooms: &Rooms, room: &String, users: &String) -> io::Result<CommandResult> {
    let rooms_map = lock_rooms(rooms)?;
    let mut c = lock_client(&client)?;
    let room_arc = match rooms_map.get(room) {
        Some(r) => Arc::clone(r),
        None => {
            send_message_locked(&mut c, &format!("Room {room} not found").yellow().to_string())?;
            return Ok(CommandResult::Handled);
        }
    };

    let mut added_any = false;
    {
        let mut room_guard = lock_room(&room_arc)?;
        for user in users.split_whitespace() {
            if room_guard.whitelist.contains(&user.to_string()) {
                send_message_locked(&mut c, &format!("'{user}' is already whitelisted").cyan().to_string())?;
            } else {
                room_guard.whitelist.push(user.to_string());
                send_success_locked(&mut c, &format!("Added '{user}' to the whitelist"))?;
                added_any = true;
            }
        }
        drop(room_guard);
    }

    if added_any {
        drop(c);
        drop(rooms_map);

        {
            let fresh_map = lock_rooms(rooms)?;
            let mut c = lock_client(&client)?;
            if let Err(e) = save_rooms_to_disk(&fresh_map) {
                send_error_locked(&mut c, &format!("Failed to save rooms: {e}"))?;
            }
        }

        let _ = broadcast_room_list_to_all(clients, rooms);
    }

    Ok(CommandResult::Handled)
}

pub fn handle_super_whitelist_remove(client: Arc<Mutex<Client>>, clients: &Clients, rooms: &Rooms, room: &String, users: &String) -> io::Result<CommandResult> {
    let rooms_map = lock_rooms(rooms)?;
    let mut c = lock_client(&client)?;
    let room_arc = match rooms_map.get(room) {
        Some(r) => Arc::clone(r),
        None => {
            send_message_locked(&mut c, &format!("Room {room} not found").yellow().to_string())?;
            return Ok(CommandResult::Handled);
        }
    };

    let mut removed_any = false;
    let mut removed_users = Vec::new();
    let whitelist_enabled = {
        let mut room_guard = lock_room(&room_arc)?;
        for user in users.split_whitespace() {
            if let Some(pos) = room_guard.whitelist.iter().position(|u| u == user) {
                room_guard.whitelist.remove(pos);
                send_success_locked(&mut c, &format!("Removed '{user}' from the whitelist"))?;
                removed_any = true;
                removed_users.push(user.to_string());
            } else {
                send_message_locked(&mut c, &format!("'{user}' is not in the whitelist").cyan().to_string())?;
            }
        }
        let enabled = room_guard.whitelist_enabled;
        drop(room_guard);
        enabled
    };

    if removed_any {
        drop(c);
        drop(rooms_map);

        {
            let fresh_map = lock_rooms(rooms)?;
            let mut c = lock_client(&client)?;
            if let Err(e) = save_rooms_to_disk(&fresh_map) {
                send_error_locked(&mut c, &format!("Failed to save rooms: {e}"))?;
            }
        }

        if whitelist_enabled {
            let room_arc = {
                let fresh_map = lock_rooms(rooms)?;
                match fresh_map.get(room) {
                    Some(r) => Arc::clone(r),
                    None => return Ok(CommandResult::Handled),
                }
            };

            let clients_map = lock_clients(clients)?;
            for c_arc in clients_map.values() {
                if let Ok(mut target_c) = c_arc.try_lock() {
                    let (should_kick, u_clone) = if let ClientState::InRoom { username: u, room: r, .. } = &target_c.state {
                        if r == room && removed_users.contains(u) {
                            let room_guard = lock_room(&room_arc)?;
                            let is_owner = room_guard.users.get(u).map(|ud| ud.role == "owner").unwrap_or(false);
                            drop(room_guard);
                            (!is_owner, Some(u.clone()))
                        } else {
                            (false, None)
                        }
                    } else {
                        (false, None)
                    };

                    if should_kick {
                        if let Some(uname) = u_clone {
                            let _ = writeln!(target_c.stream, "/LOBBY_STATE");
                            let _ = writeln!(target_c.stream, "{}", format!("You have been removed from the whitelist for '{room}' and have been kicked.").red());
                            target_c.state = ClientState::LoggedIn { username: uname };
                        }
                    }
                }
            }
        }

        let _ = broadcast_room_list_to_all(clients, rooms);
    }

    Ok(CommandResult::Handled)
}

pub fn handle_super_limit(client: Arc<Mutex<Client>>, rooms: &Rooms, room: &String) -> io::Result<CommandResult> {
    let rooms_map = lock_rooms(rooms)?;
    let room_arc = match rooms_map.get(room) {
        Some(r) => Arc::clone(r),
        None => {
            let mut c = lock_client(&client)?;
            send_message_locked(&mut c, &format!("Room {room} not found").yellow().to_string())?;
            return Ok(CommandResult::Handled);
        }
    };
    let room_guard = lock_room(&room_arc)?;
    let mut c = lock_client(&client)?;

    let rate_display = if room_guard.msg_rate == 0 {
        "UNLIMITED".to_string()
    } else {
        format!("{}", room_guard.msg_rate)
    };

    let timeout_display = if room_guard.session_timeout == 0 {
        "UNLIMITED".to_string()
    } else {
        format!("{}", room_guard.session_timeout)
    };

    writeln!(c.stream, "{}\n  > Message rate: {} messages per 5 sec\n  > Session timeout: {} sec of inactivity", "Current limits:".green(), rate_display.to_string().green(), timeout_display.to_string().green())?;
    c.stream.flush()?;
    Ok(CommandResult::Handled)
}

pub fn handle_super_limit_rate(client: Arc<Mutex<Client>>, rooms: &Rooms, room: &String, limit: u8) -> io::Result<CommandResult> {            
    let rooms_map   = lock_rooms(rooms)?;
    let room_arc    = match rooms_map.get(room) {
        Some(r) => Arc::clone(r),
        None => {
            let mut c = lock_client(&client)?;
            send_message_locked(&mut c, &"Room not found".yellow().to_string())?;
            return Ok(CommandResult::Handled);
        }
    };
    {
        let mut room_guard = lock_room(&room_arc)?;
        room_guard.msg_rate = limit;
    }

    if let Err(e) = save_rooms_to_disk(&rooms_map) {
        let mut c = lock_client(&client)?;
        send_error_locked(&mut c, &format!("Failed to save rooms: {e}"))?;
        return Ok(CommandResult::Handled);
    }

    let mut c = lock_client(&client)?;
    if limit == 0 {
        send_success_locked(&mut c, "Message rate limit set to UNLIMITED")?;
    } else {
        send_success_locked(&mut c, &format!("Message rate limit set to {limit} sec"))?;
    }
    
    Ok(CommandResult::Handled)
}

pub fn handle_super_limit_session(client: Arc<Mutex<Client>>, rooms: &Rooms, room: &String, limit: u32) -> io::Result<CommandResult> {
    let rooms_map   = lock_rooms(rooms)?;
    let room_arc    = match rooms_map.get(room) {
        Some(r) => Arc::clone(r),
        None => {
            let mut c = lock_client(&client)?;
            send_message_locked(&mut c, &"Room not found".yellow().to_string())?;
            return Ok(CommandResult::Handled);
        }
    };
    {
        let mut room_guard = lock_room(&room_arc)?;
        room_guard.session_timeout = limit;
    }

    if let Err(e) = save_rooms_to_disk(&rooms_map) {
        let mut c = lock_client(&client)?;
        send_error_locked(&mut c, &format!("Failed to save rooms: {e}"))?;
        return Ok(CommandResult::Handled);
    }

    let mut c = lock_client(&client)?;
    if limit == 0 {
        send_success_locked(&mut c, "Session timeout set to UNLIMITED")?;
    } else {
        send_success_locked(&mut c, &format!("Session timeout set to {limit} sec"))?;
    }

    Ok(CommandResult::Handled)
}
