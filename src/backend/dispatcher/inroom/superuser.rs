use std::io::{self, BufReader, BufWriter, Write};
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use serde_json::{json, Serializer};
use serde::Serialize;
use serde_json::ser::PrettyFormatter;
use std::fs::OpenOptions;
use colored::*;

use crate::shared::types::{Client, ClientState, Clients, Rooms};
use crate::shared::utils::{lock_client, lock_clients, lock_rooms, lock_room, lock_rooms_storage, send_success, send_error, send_message, save_rooms_to_disk, ColorizeExt, send_message_locked, send_error_locked, send_success_locked};
use crate::backend::dispatcher::CommandResult;

pub fn handle_super_users(client: Arc<Mutex<Client>>, clients: &Clients, rooms: &Rooms, room: &String) -> io::Result<CommandResult> {
    let mut c = lock_client(&client)?;
    let rooms_map = lock_rooms(rooms)?;
    let room_arc = match rooms_map.get(room) {
        Some(r) => Arc::clone(r),
        None => {
            send_message_locked(&mut c, &format!("Room {room} not found").yellow().to_string())?;
            return Ok(CommandResult::Handled);
        }
    };
    let room_guard = lock_room(&room_arc)?;

    writeln!(c.stream, "{}", format!("User data for {room}:").green())?;
    c.stream.flush()?;
    drop(c);
    
    let clients_map = lock_clients(clients)?;

    let mut status_map = std::collections::HashMap::new();
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
    drop(clients_map);

    let mut c = match lock_client(&client) {
        Ok(guard) => guard,
        Err(_) => return Ok(CommandResult::Handled),
    };

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
    let mut rooms_map = {
        let _c = lock_client(&client)?;
        lock_rooms(rooms)?
    };
    let old_name = room.clone();

    if rooms_map.contains_key(new_name) {
        send_message(&client, &format!("Room name '{new_name}' is already taken").yellow().to_string())?;
        return Ok(CommandResult::Handled);
    }

    let room_arc = match rooms_map.remove(&old_name) {
        Some(r) => r,
        None => {
            send_message(&client, &format!("Room '{old_name}' not found").yellow().to_string())?;
            return Ok(CommandResult::Handled);
        }
    };

    rooms_map.insert(new_name.clone(), Arc::clone(&room_arc));

    let clients_map = lock_clients(clients)?;
    for client_arc in clients_map.values() {
        let mut c = lock_client(client_arc)?;
        if let ClientState::InRoom { room: r, .. } = &mut c.state {
            if r == &old_name {
                *r = new_name.clone();
            }
        }
    }

    let _room_save_lock = lock_rooms_storage()?;
    let serializable_map: HashMap<_, _> = rooms_map.iter()
        .filter_map(|(k, v)| {
            match lock_room(v) {
                Ok(guard) => Some((k.clone(), guard.clone())),
                Err(e) => {
                    eprintln!("Failed to lock room '{k}': {e}");
                    None
                }
            }
        })
        .collect();

    let serialized = match serde_json::to_string_pretty(&serializable_map) {
        Ok(json) => json,
        Err(e) => {
            send_error(&client, &format!("Failed to serialize rooms: {e}"))?;
            return Ok(CommandResult::Handled);
        }
    };

    if let Err(e) = std::fs::write("data/rooms.json", serialized) {
        send_error(&client, &format!("Failed to write to disk: {e}"))?;
        return Ok(CommandResult::Handled);
    }

    send_success(&client, &format!("Room renamed from '{old_name}' to '{new_name}'"))?;
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
    let mut c = lock_client(&client)?;
    let rooms_map = lock_rooms(rooms)?;
    let room_arc = match rooms_map.get(room) {
        Some(r) => Arc::clone(r),
        None => {
            send_message_locked(&mut c, &format!("Room {room} not found").yellow().to_string())?;
            return Ok(CommandResult::Handled);
        }
    };

    let room_guard = lock_room(&room_arc)?;

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

pub fn handle_super_whitelist_toggle(client: Arc<Mutex<Client>>, rooms: &Rooms, room: &String) -> io::Result<CommandResult> {
    let mut c = lock_client(&client)?;
    let rooms_map = lock_rooms(rooms)?;
    let room_arc = match rooms_map.get(room) {
        Some(r) => Arc::clone(r),
        None => {
            send_message_locked(&mut c, &format!("Room {room} not found").yellow().to_string())?;
            return Ok(CommandResult::Handled);
        }
    };

    {
        let mut room_guard = lock_room(&room_arc)?;
        room_guard.whitelist_enabled = !room_guard.whitelist_enabled;
    }

    if let Ok(_store_lock) = lock_rooms_storage() {
        let mut serializable_map = HashMap::new();
        for (k, arc_mutex_room) in rooms_map.iter() {
            if let Ok(room_guard) = arc_mutex_room.lock() {
                serializable_map.insert(k.clone(), room_guard.clone());
            }
        }
        match serde_json::to_string_pretty(&serializable_map) {
            Ok(json) => {
                if let Err(e) = std::fs::write("data/rooms.json", json) {
                    send_error_locked(&mut c, &format!("Failed to write rooms.json: {e}"))?;
                    return Ok(CommandResult::Handled);
                }
            }
            Err(e) => {
                send_error_locked(&mut c, &format!("Failed to serialize rooms: {e}"))?;
                return Ok(CommandResult::Handled);
            }
        }
    } else {
        send_error_locked(&mut c, "Failed to acquire room save lock")?;
        return Ok(CommandResult::Handled);
    }

    let room_guard = lock_room(&room_arc)?;
    if room_guard.whitelist_enabled {
        send_success_locked(&mut c, "Whitelist is now ENABLED")?;
    } else {
        send_success_locked(&mut c, "Whitelist is now DISABLED")?;
    }

    Ok(CommandResult::Handled)
}

pub fn handle_super_whitelist_add(client: Arc<Mutex<Client>>, rooms: &Rooms, room: &String, users: &String) -> io::Result<CommandResult> {
    let mut c = lock_client(&client)?;
    let rooms_map = lock_rooms(rooms)?;
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
    }

    if added_any {
        if let Err(e) = save_rooms_to_disk(&rooms_map) {
            send_error_locked(&mut c, &format!("Failed to save rooms: {e}"))?;
        }
    }

    Ok(CommandResult::Handled)
}

pub fn handle_super_whitelist_remove(client: Arc<Mutex<Client>>, rooms: &Rooms, room: &String, users: &String) -> io::Result<CommandResult> {
    let mut c = lock_client(&client)?;
    let rooms_map = lock_rooms(rooms)?;
    let room_arc = match rooms_map.get(room) {
        Some(r) => Arc::clone(r),
        None => {
            send_message_locked(&mut c, &format!("Room {room} not found").yellow().to_string())?;
            return Ok(CommandResult::Handled);
        }
    };

    let mut removed_any = false;
    {
        let mut room_guard = lock_room(&room_arc)?;
        for user in users.split_whitespace() {
            if let Some(pos) = room_guard.whitelist.iter().position(|u| u == user) {
                room_guard.whitelist.remove(pos);
                send_success_locked(&mut c, &format!("Removed '{user}' from the whitelist"))?;
                removed_any = true;
            } else {
                send_message_locked(&mut c, &format!("'{user}' is not in the whitelist").cyan().to_string())?;
            }
        }
    }

    if removed_any {
        if let Err(e) = save_rooms_to_disk(&rooms_map) {
            send_error_locked(&mut c, &format!("Failed to save rooms: {e}"))?;
        }
    }

    Ok(CommandResult::Handled)
}

pub fn handle_super_limit(client: Arc<Mutex<Client>>, rooms: &Rooms, room: &String) -> io::Result<CommandResult> {
    let mut c = lock_client(&client)?;
    let rooms_map = lock_rooms(rooms)?;
    let room_arc = match rooms_map.get(room) {
        Some(r) => Arc::clone(r),
        None => {
            send_message_locked(&mut c, &format!("Room {room} not found").yellow().to_string())?;
            return Ok(CommandResult::Handled);
        }
    };
    let room_guard = lock_room(&room_arc)?;

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
    let mut c = lock_client(&client)?;
    let rooms_map   = lock_rooms(rooms)?;

    {
        let room_arc    = match rooms_map.get(room) {
            Some(r) => Arc::clone(r),
            None => {
                send_message_locked(&mut c, &"Room not found".yellow().to_string())?;
                return Ok(CommandResult::Handled);
            }
        };
        let _store_lock = lock_rooms_storage()?;
        let mut room_guard = lock_room(&room_arc)?;
        room_guard.msg_rate = limit;
    }

    if let Err(e) = save_rooms_to_disk(&rooms_map) {
        send_error_locked(&mut c, &format!("Failed to save rooms: {e}"))?;
        return Ok(CommandResult::Handled);
    }

    if limit == 0 {
        send_success_locked(&mut c, "Message rate limit set to UNLIMITED")?;
    } else {
        send_success_locked(&mut c, &format!("Message rate limit set to {limit} sec"))?;
    }
    
    Ok(CommandResult::Handled)
}

pub fn handle_super_limit_session(client: Arc<Mutex<Client>>, rooms: &Rooms, room: &String, limit: u32) -> io::Result<CommandResult> {
    let mut c = lock_client(&client)?;
    let rooms_map   = lock_rooms(rooms)?;

    {
        let room_arc    = match rooms_map.get(room) {
            Some(r) => Arc::clone(r),
            None => {
                send_message_locked(&mut c, &"Room not found".yellow().to_string())?;
                return Ok(CommandResult::Handled);
            }
        };
        let _store_lock = lock_rooms_storage()?;
        let mut room_guard = lock_room(&room_arc)?;
        room_guard.session_timeout = limit;
    }

    if let Err(e) = save_rooms_to_disk(&rooms_map) {
        send_error_locked(&mut c, &format!("Failed to save rooms: {e}"))?;
        return Ok(CommandResult::Handled);
    }

    if limit == 0 {
        send_success_locked(&mut c, "Session timeout set to UNLIMITED")?;
    } else {
        send_success_locked(&mut c, &format!("Session timeout set to {limit} sec"))?;
    }

    Ok(CommandResult::Handled)
}
