use std::collections::VecDeque;
use std::io::{self, BufRead, BufReader, Write};
use std::fs::File;
use serde_json::{json, Value};
use std::time::{SystemTime, UNIX_EPOCH};
use std::sync::{Arc, Mutex};
use colored::*;

use crate::shared::types::{Client, ClientState, Room, RoomUser, Rooms};
use crate::shared::utils::{lock_client, lock_rooms, lock_rooms_storage, send_success, send_error, send_message, load_json, save_json, send_error_locked, send_success_locked};
use crate::backend::command_utils::sync_room_members;
use crate::backend::dispatcher::CommandResult;
use crate::shared::types::{Clients, PublicKeys};

pub fn handle_room_list(client: Arc<Mutex<Client>>, rooms: &Rooms, username: &String) -> io::Result<CommandResult> {
    let locked_rooms = lock_rooms(rooms)?;
    let _lock = lock_rooms_storage()?;

    let mut visible_rooms = Vec::new();

    for (room_name, room_arc) in locked_rooms.iter() {
        if let Ok(room) = room_arc.lock() {
            if !room.whitelist_enabled || room.whitelist.contains(username) {
                let count = room.online_users.len();
                if count == 1 {
                    visible_rooms.push(format!("> {room_name} ({count} user online)"));
                }
                else {
                    visible_rooms.push(format!("> {room_name} ({count} users online)"));
                }
            }
        }
    }

    if visible_rooms.is_empty() {
        send_error(&client, "No available rooms found")?;
    } else {
        send_success(&client, &format!("Available rooms:\n{}", visible_rooms.join("\n")))?;
    }

    Ok(CommandResult::Handled)
}

pub fn handle_room_create(client: Arc<Mutex<Client>>, rooms: &Rooms, username: &String, name: &String, whitelist: bool) -> io::Result<CommandResult> {
    let id_exists = {
        let _c = lock_client(&client)?;
        let rooms_map = lock_rooms(rooms)?;
        rooms_map.contains_key(name)
    };

    if id_exists {
        send_error(&client, "Room already exists")?;
        return Ok(CommandResult::Handled);
    }
    
    let mut rooms_map = lock_rooms(rooms)?;
    let _lock = lock_rooms_storage()?;
        
    let new_room = json!({
        "whitelist_enabled": whitelist,
        "whitelist": if whitelist { vec![username.clone()] } else { Vec::<String>::new() },
        "msg_rate": 10,
        "session_timeout": 3600,
        "roles": {
            "moderator": ["afk", "seen", "msg", "me", "super.users", "user", "mod"],
            "user": ["afk", "seen", "msg", "me", "user"],
            "colors": {
                "owner": "#FFD700",
                "admin": "#FF3030",
                "moderator": "#0080FF",
                "user": "FFFFFF"
            }
        },
        "users": {
            username: {
                "nick": "",
                "color": "",
                "role": "owner",
                "hidden": false,
                "last_seen": 0,
                "banned": false,
                "ban_stamp": 0,
                "ban_length": 0,
                "ban_reason": "",
                "muted": false,
                "mute_stamp": 0,
                "mute_length": 0,
                "mute_reason": ""
            }
        }
    });

    let file_path = "data/rooms.json";
    let mut rooms_json = load_json(file_path)?;

    rooms_json[name] = new_room.clone();

    save_json(file_path, &rooms_json)?;

    let roles = match serde_json::from_value(new_room["roles"].clone()) {
        Ok(val) => val,
        Err(e) => {
            send_error(&client, &format!("Error parsing roles: {e}"))?;
            return Ok(CommandResult::Handled);
        }
    };

    let users = match serde_json::from_value(new_room["users"].clone()) {
        Ok(val) => val,
        Err(e) => {
            send_error(&client, &format!("Error parsing users: {e}"))?;
            return Ok(CommandResult::Handled);
        }
    };

    let room_obj = Room {
        whitelist_enabled: whitelist,
        whitelist: if whitelist { vec![username.clone()] } else { vec![] },
        msg_rate: 10,
        session_timeout: 3600,
        roles,
        users,
        online_users: Vec::new(),
    };

    rooms_map.insert(name.clone(), Arc::new(Mutex::new(room_obj)));

    if whitelist {
        send_success(&client, &format!("Whitelisted room {name} created successfully"))?;
    }
    else {
        send_success(&client, &format!("Room {name} created successfully"))?;                
    }
    Ok(CommandResult::Handled)
}

pub fn handle_room_join(client: Arc<Mutex<Client>>, clients: &Clients, rooms: &Rooms, pubkeys: &PublicKeys, username: &String, name: &String) -> io::Result<CommandResult> {
    let mut c = lock_client(&client)?;

    let room_arc = {
        let rooms_map = lock_rooms(rooms)?;
        match rooms_map.get(name) {
            Some(r) => Arc::clone(r),
            None => {
                send_error_locked(&mut c, &format!("Room {name} not found"))?;
                return Ok(CommandResult::Handled);
            }
        }
    };
    
    let _lock = lock_rooms_storage()?;
    let mut room = match room_arc.lock() {
        Ok(r) => r,
        Err(_) => {
            send_error_locked(&mut c, "Could not lock room")?;
            return Ok(CommandResult::Handled);
        }
    };

    let is_owner = match room.users.get(username) {
        Some(u) => u.role == "owner",
        None => false,
    };

    if room.whitelist_enabled && !room.whitelist.contains(username) && !is_owner {
        send_error_locked(&mut c, "You aren't whitelisted for this room")?;
        return Ok(CommandResult::Handled);
    }

    let now_ts: u64 = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d)  => d.as_secs(),
        Err(_) => 0,
    };

    if let Some(rec) = room.users.get_mut(username) {
        let mut inf = false;
        let mut ban_expires: u64 = 0;
        if rec.banned {
            if rec.ban_length == 0 {
                inf = true;
            } else {
                ban_expires = rec.ban_stamp.saturating_add(rec.ban_length)
            }

            if now_ts < ban_expires || inf {
                let remaining_text = if inf {
                    "Permanent".to_string()
                } else {
                    let remaining_secs = ban_expires.saturating_sub(now_ts);
                    let days = remaining_secs / 86_400;
                    let hrs = (remaining_secs % 86_400) / 3_600;
                    let mins = (remaining_secs % 3_600) / 60;
                    let secs = remaining_secs % 60;

                    if days > 0 {
                        format!("{days}d {hrs}h {mins}m {secs}s remaining")
                    } else if hrs > 0 {
                        format!("{hrs}h {mins}m {secs}s remaining")
                    } else if mins > 0 {
                        format!("{mins}m {secs}s remaining")
                    } else {
                        format!("{secs}s remaining")
                    }
                };

                let reason_txt = if rec.ban_reason.is_empty() {
                    format!("You are banned from this room ({remaining_text})")
                } else {
                    format!("You are banned from this room ({})\n> {}", rec.ban_reason, remaining_text)
                };

                send_error_locked(&mut c, &reason_txt)?;
                return Ok(CommandResult::Handled);
            }

            rec.banned      = false;
            rec.ban_stamp   = 0;
            rec.ban_length  = 0;
            rec.ban_reason.clear();

            let mut rooms_json = load_json("data/rooms.json")?;

            if let Some(room_json) = rooms_json.get_mut(name) {
                if let Some(user_json) = room_json["users"].get_mut(username) {
                    user_json["banned"]      = json!(false);
                    user_json["ban_stamp"]   = json!(0);
                    user_json["ban_length"]  = json!(0);
                    user_json["ban_reason"]  = json!("");
                }
            }

            save_json("data/rooms.json", &rooms_json)?;
        }
    }

    if !room.users.contains_key(username) {
        room.users.insert(username.clone(), RoomUser {
            nick: "".to_string(),
            color: "".to_string(),
            role: "user".to_string(),
            hidden: false,
            last_seen: 0,
            banned: false,
            ban_stamp: 0,
            ban_length: 0,
            ban_reason: "".to_string(),
            muted: false,
            mute_stamp: 0,
            mute_length: 0,
            mute_reason: "".to_string()
        });

        let mut rooms_json = load_json("data/rooms.json")?;

        if let Some(room_json) = rooms_json.get_mut(name) {
            room_json["users"][username] = json!({
                "nick": "",
                "color": "",
                "role": "user",
                "hidden": false,
                "last_seen": 0,
                "banned": false,
                "ban_stamp": 0,
                "ban_length": 0,
                "ban_reason": "",
                "muted": false,
                "mute_stamp": 0,
                "mute_length": 0,
                "mute_reason": ""
            });

            save_json("data/rooms.json", &rooms_json)?;
        }
    }

    if !room.online_users.contains(username) {
        room.online_users.push(username.clone());
    }

    c.state = ClientState::InRoom {
        username: username.clone(),
        room: name.clone(),
        room_time: Some(SystemTime::now()),
        msg_timestamps: VecDeque::new(),
        inactive_time: Some(SystemTime::now()),
        is_afk: false
    };

    writeln!(c.stream, "/ROOM_STATE")?;
    writeln!(c.stream, "/ROOM_NAME {name}")?;

    let user_role = room.users.get(username)
        .map(|u| u.role.as_str())
        .unwrap_or("user");
    writeln!(c.stream, "/ROLE {user_role}")?;

    send_success_locked(&mut c, &format!("Joined room: {name}"))?;
    drop(room);
    drop(c);
    let _ = sync_room_members(rooms, clients, pubkeys, name);

    Ok(CommandResult::Handled)
}

pub fn handle_room_import(client: Arc<Mutex<Client>>, rooms: &Rooms, filename: &String) -> io::Result<CommandResult> {
    let safe_filename = if !filename.ends_with(".json") {
        format!("{filename}.json")
    } else {
        filename.clone()
    };

    let import_path = format!("data/vault/rooms/{safe_filename}");
    let import_file = match File::open(&import_path) {
        Ok(file) => file,
        Err(_) => {
            send_message(&client, &format!("Error: Could not open {import_path}").yellow().to_string())?;
            return Ok(CommandResult::Handled);
        }
    };

    let import_reader = BufReader::new(import_file);
    let import_data: Value = match serde_json::from_reader(import_reader) {
        Ok(data) => data,
        Err(_) => {
            send_message(&client, &"Error: Invalid JSON format in import file".yellow().to_string())?;
            return Ok(CommandResult::Handled);
        }
    };

    let (room_name, room_value) = match import_data.as_object().and_then(|obj| obj.iter().next()) {
        Some((name, val)) => (name.clone(), val.clone()),
        None => {
            send_message(&client, &"Error: Import file is empty or malformed".yellow().to_string())?;
            return Ok(CommandResult::Handled);
        }
    };

    let mut rooms_map = lock_rooms(rooms)?;
    let _lock = lock_rooms_storage()?;

    let mut rooms_json = load_json("data/rooms.json")?;

    if rooms_json.get(&room_name).is_some() {
        send_message(&client, &format!("Error: Room {room_name} already exists").yellow().to_string())?;
        return Ok(CommandResult::Handled);
    }

    rooms_json[&room_name] = room_value.clone();
    save_json("data/rooms.json", &rooms_json)?;

    let room_obj: Room = match serde_json::from_value(room_value) {
        Ok(room) => room,
        Err(e) => {
            send_error(&client, &format!("Error: Failed to parse room data: {e}"))?;
            return Ok(CommandResult::Handled);
        }
    };

    rooms_map.insert(room_name.clone(), Arc::new(Mutex::new(Room {
        online_users: vec![],
        ..room_obj
    })));

    send_success(&client, &format!("Imported room: {room_name}"))?;
    Ok(CommandResult::Handled)
}

pub fn handle_room_delete(client: Arc<Mutex<Client>>, rooms: &Rooms, username: &String, name: &String, force: bool) -> io::Result<CommandResult> {
    let mut rooms_map = lock_rooms(rooms)?;
    let _lock = lock_rooms_storage()?;

    let room_arc = match rooms_map.get(name) {
        Some(r) => Arc::clone(r),
        None => {
            send_message(&client, &format!("Error: Room {name} not found").yellow().to_string())?;
            return Ok(CommandResult::Handled);
        }
    };

    {
        let room = match room_arc.lock() {
            Ok(g) => g,
            Err(_) => {
                send_error(&client, "Could not lock room")?;
                return Ok(CommandResult::Handled);
            }
        };
        match room.users.get(username) {
            Some(user) if user.role == "owner" => (),
            _ => {
                send_message(&client, &"Error: Only the room owner can delete this room".yellow().to_string())?;
                return Ok(CommandResult::Handled);
            }
        }
    }

    if !force {
        let mut c = lock_client(&client)?;
        writeln!(c.stream, "{}", format!("Are you sure you want to delete room {name}? (y/n): ").red())?;

        let mut reader = BufReader::new(c.stream.try_clone()?);
        loop {
            let mut line = String::new();
            let bytes_read = reader.read_line(&mut line)?;
            if bytes_read == 0 {
                let clone = Arc::clone(&client);
                send_error(&clone, "Connection closed")?;
                return Ok(CommandResult::Stop);
            }

            match line.trim().to_lowercase().as_str() {
                "y" => break,
                "n" => {
                    send_message(&client, &"Room deletion cancelled".yellow().to_string())?;
                    return Ok(CommandResult::Handled);
                }
                _ => {
                    send_error(&client, "(y/n): ")?;
                }
            }
        }
    }

    rooms_map.remove(name);

    let mut rooms_json = load_json("data/rooms.json")?;

    if rooms_json.get(name).is_some() {
        if let Some(map) = rooms_json.as_object_mut() {
            map.remove(name);
        } else {
            send_error(&client, "Error: Malformed rooms.json")?;
            return Ok(CommandResult::Handled);
        }
        save_json("data/rooms.json", &rooms_json)?;
    }

    send_success(&client, &format!("Room {name} deleted successfully"))?;
    Ok(CommandResult::Handled)
}
