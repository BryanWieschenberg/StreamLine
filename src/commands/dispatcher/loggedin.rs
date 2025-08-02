use std::collections::VecDeque;
use std::io::{self, BufRead, BufReader, Write};
use std::fs::{File, OpenOptions};
use serde::Serialize;
use serde_json::{json, Serializer, Value};
use serde_json::ser::PrettyFormatter;
use std::time::{SystemTime, UNIX_EPOCH};
use std::sync::{Arc, Mutex};
use colored::*;

use crate::commands::parser::Command;
use crate::commands::command_utils::{help_msg_loggedin, generate_hash};
use crate::types::{Client, ClientState, Clients, PublicKeys, Room, RoomUser, Rooms};
use crate::utils::{lock_client, lock_clients, lock_users_storage, lock_rooms, lock_room, lock_rooms_storage};
use super::CommandResult;

pub fn loggedin_command(cmd: Command, client: Arc<Mutex<Client>>, clients: &Clients, rooms: &Rooms, username: &String, pubkeys: &PublicKeys) -> io::Result<CommandResult> {
    match cmd {
        Command::Help => {
            let mut client = lock_client(&client)?;
            writeln!(client.stream, "{}{}", help_msg_loggedin().bright_blue(), "\x1b[0m")?;
            Ok(CommandResult::Handled)
        }

        Command::Ping { start_time }=> {
            let mut client = lock_client(&client)?;
            if let Some(start_ms) = start_time {
                writeln!(client.stream, "/PONG {}", start_ms)?;
            }
            Ok(CommandResult::Handled)
        }

        Command::PubKey { pubkey } => {
            let mut map = match pubkeys.lock() {
                Ok(m) => m,
                Err(_) => {
                    let mut client = lock_client(&client)?;
                    writeln!(client.stream, "{}", "Error: failed to lock pubkeys".red())?;
                    return Ok(CommandResult::Handled);
                }
            };

            if map.contains_key(username) {
                let mut client = lock_client(&client)?;
                writeln!(client.stream, "{}", "Error: Public key already registered for this user".yellow())?;
                return Ok(CommandResult::Handled);
            }

            map.insert(username.clone(), pubkey.clone());
            Ok(CommandResult::Handled)
        }

        Command::Quit => {
            {
                let mut pubkeys_map = match pubkeys.lock() {
                    Ok(g) => g,
                    Err(_) => return Ok(CommandResult::Handled),
                };

                pubkeys_map.remove(username);
            }

            let addr = {
                let c = lock_client(&client)?;
                c.addr
            };

            {
                let mut clients = lock_clients(&clients)?;
                clients.remove(&addr);
            }

            let mut client = lock_client(&client)?;
            writeln!(client.stream, "{}", "Exiting...".green())?;
            client.stream.shutdown(std::net::Shutdown::Both)?;
            Ok(CommandResult::Stop)
        }

        Command::IgnoreList => {
            let ignore_list = {
                let client_guard = lock_client(&client)?;
                client_guard.ignore_list.clone()
            };

            let mut client = lock_client(&client)?;
            
            if client.ignore_list.is_empty() {
                writeln!(client.stream, "{}", "You do not currently have anyone ignored".green())?;
            } else {
                writeln!(client.stream, "{}", format!("Currently ignoring: {}", ignore_list.join(", ")).green())?;
            }
            Ok(CommandResult::Handled)
        }
        
        Command::IgnoreAdd { users } => {
            let to_add: Vec<String> = users
                .split_whitespace()
                .filter(|u| !u.is_empty() && u != username)
                .map(|u| u.to_string())
                .collect();

            let (added, already): (Vec<String>, Vec<String>) = {
                let mut client = lock_client(&client)?;
                let mut added = Vec::new();
                let mut already = Vec::new();
                for u in &to_add {
                    if client.ignore_list.contains(u) {
                        already.push(u.clone());
                    } else {
                        client.ignore_list.push(u.clone());
                        added.push(u.clone());
                    }
                }
                (added, already)
            };

            if !added.is_empty() {
                let _ulock = lock_users_storage()?;
                let file = File::open("data/users.json")?;
                let reader = BufReader::new(file);
                let mut users_json: Value = serde_json::from_reader(reader)?;

                if let Some(ignore_arr) = users_json[username]
                    .get_mut("ignore")
                    .and_then(Value::as_array_mut)
                {
                    for u in &added {
                        ignore_arr.push(json!(u));
                    }
                }

                let file = OpenOptions::new().write(true).truncate(true).open("data/users.json")?;
                let mut writer = std::io::BufWriter::new(file);
                let formatter = PrettyFormatter::with_indent(b"    ");
                let mut ser = Serializer::with_formatter(&mut writer, formatter);
                users_json.serialize(&mut ser)?;
            }

            let mut client = lock_client(&client)?;
            if !added.is_empty() {
                writeln!(client.stream, "{}", format!("Added to ignore list: {}", added.join(", ")).green())?;
            }
            if !already.is_empty() {
                writeln!(client.stream, "{}", format!("Already ignored: {}", already.join(", ")).yellow())?;
            }
            Ok(CommandResult::Handled)
        }

        Command::IgnoreRemove { users } => {
            let to_remove: Vec<String> = users
                .split_whitespace()
                .filter(|u| !u.is_empty())
                .map(|u| u.to_string())
                .collect();

            let (removed, not_found): (Vec<String>, Vec<String>) = {
                let mut client = lock_client(&client)?;
                let mut removed = Vec::new();
                let mut not_found = Vec::new();
                for u in &to_remove {
                    if client.ignore_list.contains(u) {
                        removed.push(u.clone());
                    } else {
                        not_found.push(u.clone());
                    }
                }
                client.ignore_list.retain(|u| !removed.contains(u));
                (removed, not_found)
            };

            if !removed.is_empty() {
                let _ulock = lock_users_storage()?;
                let file = File::open("data/users.json")?;
                let reader = BufReader::new(file);
                let mut users_json: Value = serde_json::from_reader(reader)?;

                if let Some(ignore_arr) = users_json[username]
                    .get_mut("ignore")
                    .and_then(Value::as_array_mut)
                {
                    ignore_arr.retain(|v| !removed.iter().any(|u| v == u));
                }

                let file = OpenOptions::new().write(true).truncate(true).open("data/users.json")?;
                let mut writer = std::io::BufWriter::new(file);
                let formatter = PrettyFormatter::with_indent(b"    ");
                let mut ser = Serializer::with_formatter(&mut writer, formatter);
                users_json.serialize(&mut ser)?;
            }

            let mut client = lock_client(&client)?;
            if !removed.is_empty() {
                writeln!(client.stream, "{}", format!("Removed from ignore list: {}", removed.join(", ")).green())?;
            }
            if !not_found.is_empty() {
                writeln!(client.stream, "{}", format!("Not in ignore list: {}", not_found.join(", ")).yellow())?;
            }
            Ok(CommandResult::Handled)
        }

        Command::Leave | Command::Status | Command::AFK | Command::Announce { .. } | Command::Seen { .. } | Command::DM { .. } | Command::Me { .. } |
        Command::SuperUsers | Command::SuperRename { .. } | Command::SuperExport { .. } | Command::SuperWhitelist | Command::SuperWhitelistToggle | Command::SuperWhitelistAdd { .. } | Command::SuperWhitelistRemove { .. } | Command::SuperLimit | Command::SuperLimitRate { .. } | Command::SuperLimitSession { .. } | Command::SuperRoles | Command::SuperRolesAdd { .. } | Command::SuperRolesRevoke { .. } | Command::SuperRolesAssign { .. } | Command::SuperRolesRecolor { .. } |
        Command::Users | Command::UsersRename { .. } | Command::UsersRecolor { .. } | Command::UsersHide |
        Command::ModInfo | Command::ModKick { .. } | Command::ModMute { .. } | Command::ModUnmute { .. } | Command::ModBan { .. } | Command::ModUnban { .. } => {
            let mut client = lock_client(&client)?;
            writeln!(client.stream, "{}", "Must be in a room to perform this command".yellow())?;
            Ok(CommandResult::Handled)
        }

        Command::AccountRegister { .. } | Command::AccountLogin { .. } => {
            let mut client = lock_client(&client)?;
            writeln!(client.stream, "{}", "You are already logged in".yellow())?;
            Ok(CommandResult::Handled)
        }

        Command::AccountLogout => {
            {
                let mut pubkeys_map = match pubkeys.lock() {
                    Ok(g) => g,
                    Err(_) => return Ok(CommandResult::Handled),
                };

                pubkeys_map.remove(username);
            }
            
            let mut client = lock_client(&client)?;
            client.state = ClientState::Guest;
            writeln!(client.stream, "{}", format!("Logged out: {}", username).green())?;
            
            Ok(CommandResult::Handled)
        }

        Command::AccountEditUsername { username: new_username } => {
            let mut client = lock_client(&client)?;
            let _lock = lock_users_storage()?;

            let file = File::open("data/users.json")?;
            let reader = BufReader::new(file);
            let mut users: Value = serde_json::from_reader(reader)?;

            if users.get(&new_username).is_some() {
                writeln!(client.stream, "{}", "Error: Username is already taken".yellow())?;
                return Ok(CommandResult::Handled);
            }

            if let Some(old_data) = users.get(username).cloned() {
                users[&new_username] = old_data;
                if let Some(map) = users.as_object_mut() {
                    map.remove(username);
                }
            } else {
                writeln!(client.stream, "{}", "Error: Original username not found".yellow())?;
                return Ok(CommandResult::Handled);
            }

            let file = OpenOptions::new()
                .write(true)
                .truncate(true)
                .open("data/users.json")?;

            let mut writer = std::io::BufWriter::new(file);
            let formatter = PrettyFormatter::with_indent(b"    ");
            let mut ser = Serializer::with_formatter(&mut writer, formatter);
            users.serialize(&mut ser)?;

            let old_username = username.clone(); // store original
            client.state = ClientState::LoggedIn { username: new_username.clone() };

            writeln!(client.stream, "{}", format!("/GUEST_STATE"))?;
            writeln!(client.stream, "{}", format!("Username changed from {} to: {}", old_username, new_username).green())?;
            Ok(CommandResult::Handled)
        }

        Command::AccountEditPassword { current_password, new_password } => {
            let mut client = lock_client(&client)?;
            let _lock = lock_users_storage()?;

            let file = File::open("data/users.json")?;
            let reader = BufReader::new(file);
            let mut users: Value = serde_json::from_reader(reader)?;

            let user_obj = if let Some(obj) = users.get_mut(username) {
                obj
            }
            else {
                writeln!(client.stream, "{}", "Error: Username not found".yellow())?;
                return Ok(CommandResult::Handled);
            };

            let stored_hash = match user_obj.get("password").and_then(|v| v.as_str()) {
                Some(hash) => hash,
                None => {
                    writeln!(client.stream, "{}", "Error: Password field missing".yellow())?;
                    return Ok(CommandResult::Handled);
                }
            };

            if generate_hash(&current_password) != stored_hash {
                writeln!(client.stream, "{}", "Error: Incorrect current password".yellow())?;
                return Ok(CommandResult::Handled);
            }

            let new_hash = generate_hash(&new_password);
            user_obj["password"] = Value::String(new_hash);

            let file = OpenOptions::new()
                .write(true)
                .truncate(true)
                .open("data/users.json")?;

            let mut writer = std::io::BufWriter::new(file);
            let formatter = PrettyFormatter::with_indent(b"    ");
            let mut ser = Serializer::with_formatter(&mut writer, formatter);
            users.serialize(&mut ser)?;

            writeln!(client.stream, "{}", "Password updated successfully".green())?;
            Ok(CommandResult::Handled)
        }

        Command::AccountImport { filename } => {
            let safe_filename = if !filename.ends_with(".json") {
                format!("{}.json", filename)
            }
            else {
                filename
            };

            let import_path = format!("data/vault/users/{}", safe_filename);
            let import_file = match File::open(&import_path) {
                Ok(file) => file,
                Err(_) => {
                    let mut client = lock_client(&client)?;
                    writeln!(client.stream, "{}", format!("Error: Could not open {}", import_path).yellow())?;
                    return Ok(CommandResult::Handled);
                }
            };

            let import_reader = BufReader::new(import_file);
            let import_user: Value = match serde_json::from_reader(import_reader) {
                Ok(data) => data,
                Err(_) => {
                    let mut client = lock_client(&client)?;
                    writeln!(client.stream, "{}", "Error: Invalid JSON format in import file".yellow())?;
                    return Ok(CommandResult::Handled);
                }
            };

            let (username, user_data) = match import_user.as_object().and_then(|obj| obj.iter().next()) {
                Some((u, data)) => (u.clone(), data.clone()),
                None => {
                    let mut client = lock_client(&client)?;
                    writeln!(client.stream, "{}", "Error: Import file is empty or malformed".yellow())?;
                    return Ok(CommandResult::Handled);
                }
            };

            let _lock = lock_users_storage()?;

            let file = File::open("data/users.json")?;
            let reader = BufReader::new(file);
            let mut users: Value = serde_json::from_reader(reader)?;

            if users.get(&username).is_some() {
                let mut client = lock_client(&client)?;
                writeln!(client.stream, "{}", format!("Error: User {} already exists", username).yellow())?;
                return Ok(CommandResult::Handled);
            }

            users[&username] = user_data;

            let file = OpenOptions::new()
                .write(true)
                .truncate(true)
                .open("data/users.json")?;

            let mut writer = std::io::BufWriter::new(file);
            let formatter = PrettyFormatter::with_indent(b"    ");
            let mut ser = Serializer::with_formatter(&mut writer, formatter);
            users.serialize(&mut ser)?;

            let mut client = lock_client(&client)?;
            writeln!(client.stream, "{}", format!("Imported user: {}", username).green())?;
            Ok(CommandResult::Handled)
        }

        Command::AccountExport { filename } => {
            let _lock = lock_users_storage()?;

            let file = File::open("data/users.json")?;
            let reader = BufReader::new(file);
            let users: Value = serde_json::from_reader(reader)?;

            let user_data = match users.get(&username) {
                Some(data) => data.clone(),
                None => {
                    let mut client = lock_client(&client)?;
                    writeln!(client.stream, "{}", "Error: Your account data could not be found".red())?;
                    return Ok(CommandResult::Handled);
                }
            };

            let final_filename = {
                if filename.is_empty() {
                    let timestamp = chrono::Local::now().format("%y%m%d%H%M%S").to_string();
                    format!("{}_{}.json", username, timestamp)
                } else if !filename.ends_with(".json") {
                    format!("{}.json", filename)
                }
                else {
                    filename
                }
            };

            let export_path = format!("data/vault/users/{}", final_filename);
            let export_file = OpenOptions::new().create(true).write(true).truncate(true).open(&export_path)?;

            let mut writer = std::io::BufWriter::new(export_file);
            let formatter = PrettyFormatter::with_indent(b"    ");
            let mut ser = Serializer::with_formatter(&mut writer, formatter);

            json!({ username: user_data }).serialize(&mut ser)?;

            let mut client = lock_client(&client)?;
            writeln!(client.stream, "{}", format!("Exported account data to: {}", final_filename).green())?;
            Ok(CommandResult::Handled)
        }

        Command::AccountDelete { force } => {
            if !force {
                let mut client = lock_client(&client)?;
                writeln!(client.stream, "{}", "Are you sure you want to delete your account? (y/n): ".red())?;

                let mut reader: BufReader<std::net::TcpStream> = BufReader::new(client.stream.try_clone()?);
                loop {
                    let mut line = String::new();
                    let bytes_read = reader.read_line(&mut line)?;
                    if bytes_read == 0 {
                        writeln!(client.stream, "{}", "Connection closed".yellow())?;
                        return Ok(CommandResult::Stop);
                    }

                    let input = line.trim().to_lowercase();
                    match input.as_str() {
                        "y" => break,
                        "n" => {
                            writeln!(client.stream, "{}", "Account deletion cancelled".yellow())?;
                            return Ok(CommandResult::Handled);
                        },
                        _ => {
                            writeln!(client.stream, "{}", "(y/n): ".red())?;
                        }
                    }
                }
            }

            let _lock = lock_users_storage()?;
            let file = File::open("data/users.json")?;
            let reader = BufReader::new(file);
            let mut users: Value = serde_json::from_reader(reader)?;

            if users.get(&username).is_none() {
                let mut client = lock_client(&client)?;
                writeln!(client.stream, "{}", "Error: User not found in records".red())?;
                return Ok(CommandResult::Handled);
            }

            if let Some(obj) = users.as_object_mut() {
                obj.remove(username);
            }
            else {
                let mut client = lock_client(&client)?;
                writeln!(client.stream, "{}", "Error: Malformed users.json".red())?;
                return Ok(CommandResult::Handled);
            }

            let file = OpenOptions::new()
                .write(true)
                .truncate(true)
                .open("data/users.json")?;

            let mut writer = std::io::BufWriter::new(file);
            let formatter = PrettyFormatter::with_indent(b"    ");
            let mut ser = Serializer::with_formatter(&mut writer, formatter);
            users.serialize(&mut ser)?;

            let mut client = lock_client(&client)?;
            client.state = ClientState::Guest;
            writeln!(client.stream, "{}", format!("Account {} deleted successfully, you are now a guest", username).green())?;

            Ok(CommandResult::Handled)
        }

        Command::Account => {
            let mut client = lock_client(&client)?;
            writeln!(client.stream, "{}", format!("Currently logged in as: {} (not in a room)", username).green())?;
            Ok(CommandResult::Handled)
        }

        Command::RoomList => {
            let _lock = lock_rooms_storage()?;

            let locked_rooms = lock_rooms(rooms)?;
            let mut visible_rooms = Vec::new();

            for (room_name, room_arc) in locked_rooms.iter() {
                if let Ok(room) = room_arc.lock() {
                    if !room.whitelist_enabled || room.whitelist.contains(&username) {
                        let count = room.online_users.len();
                        if count == 1 {
                            visible_rooms.push(format!("> {} ({} user online)", room_name, count));
                        }
                        else {
                            visible_rooms.push(format!("> {} ({} users online)", room_name, count));
                        }
                    }
                }
            }

            let mut client = lock_client(&client)?;
            if visible_rooms.is_empty() {
                writeln!(client.stream, "{}", "No available rooms found".yellow())?;
            } else {
                writeln!(client.stream, "{}", format!("Available rooms:\n{}", visible_rooms.join("\n")).green())?;
            }

            Ok(CommandResult::Handled)
        }

        Command::RoomCreate { name, whitelist } => {
            let _lock = lock_rooms_storage()?;

            {
                let rooms = lock_rooms(rooms)?;
                if rooms.contains_key(&name) {
                    let mut client = lock_client(&client)?;
                    writeln!(client.stream, "{}", "Error: Room already exists".yellow())?;
                    return Ok(CommandResult::Handled);
                }
            }
                
            let new_room = json!({
                "whitelist_enabled": whitelist,
                "whitelist": if whitelist { vec![username.clone()] } else { Vec::<String>::new() },
                "msg_rate": 60,
                "session_timeout": 10,
                "roles": {
                    "moderator": ["afk", "seen", "msg", "me", "super.users", "user", "mod"],
                    "user": ["afk", "seen", "msg", "me", "user"],
                    "colors": {
                        "owner": "#FFD700",
                        "admin": "#FF3030",
                        "mod": "#0080FF",
                        "user": "FFFFFF"
                    }
                },
                "users": {
                    username: {
                        "nick": "",
                        "color": "",
                        "role": "owner",
                        "hidden": false,
                        "muted": "",
                        "banned": ""
                    }
                }
            });

            let file_path = "data/rooms.json";
            let file = File::open(file_path)?;
            let reader = BufReader::new(file);
            let mut rooms_json: Value = serde_json::from_reader(reader)?;

            rooms_json[&name] = new_room.clone();

            let file = OpenOptions::new()
                .write(true)
                .truncate(true)
                .open(file_path)?;
            
            let mut writer = std::io::BufWriter::new(file);
            let formatter = PrettyFormatter::with_indent(b"    ");
            let mut ser = Serializer::with_formatter(&mut writer, formatter);
            rooms_json.serialize(&mut ser)?;

            let roles = match serde_json::from_value(new_room["roles"].clone()) {
                Ok(val) => val,
                Err(e) => {
                    let mut client = lock_client(&client)?;
                    writeln!(client.stream, "{}", format!("Error parsing roles: {e}").red())?;
                    return Ok(CommandResult::Handled);
                }
            };

            let users = match serde_json::from_value(new_room["users"].clone()) {
                Ok(val) => val,
                Err(e) => {
                    let mut client = lock_client(&client)?;
                    writeln!(client.stream, "{}", format!("Error parsing users: {e}").red())?;
                    return Ok(CommandResult::Handled);
                }
            };

            let room_obj = Room {
                whitelist_enabled: whitelist,
                whitelist: if whitelist { vec![username.clone()] } else { vec![] },
                msg_rate: 60,
                session_timeout: 10,
                roles,
                users,
                online_users: Vec::new(),
            };

            {
                let mut rooms = lock_rooms(rooms)?;
                rooms.insert(name.clone(), Arc::new(Mutex::new(room_obj)));
            }

            let mut client = lock_client(&client)?;
            if whitelist {
                writeln!(client.stream, "{}", format!("Whitelisted room {} created successfully", name).green())?;
            }
            else {
                writeln!(client.stream, "{}", format!("Room {} created successfully", name).green())?;                
            }
            Ok(CommandResult::Handled)
        }

        Command::RoomJoin { name } => {
            let _lock = lock_rooms_storage()?;
            let rooms = lock_rooms(rooms)?;

            let room_arc = match rooms.get(&name) {
                Some(r) => Arc::clone(r),
                None => {
                    let mut client = lock_client(&client)?;
                    writeln!(client.stream, "{}", format!("Room {} not found", name).yellow())?;
                    return Ok(CommandResult::Handled);
                }
            };
            
            let mut room = match room_arc.lock() {
                Ok(r) => r,
                Err(_) => {
                    let mut client = lock_client(&client)?;
                    writeln!(client.stream, "{}", "Error: Could not lock room".red())?;
                    return Ok(CommandResult::Handled);
                }
            };

            let is_owner = match room.users.get(username) {
                Some(u) => u.role == "owner",
                None => false,
            };

            if room.whitelist_enabled && !room.whitelist.contains(username) && !is_owner {
                let mut client = lock_client(&client)?;
                writeln!(client.stream, "{}", "You aren't whitelisted for this room".red())?;
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
                        ban_expires = rec.ban_stamp.saturating_add(rec.ban_length as u64)
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
                                format!("{}d {}h {}m {}s remaining", days, hrs, mins, secs)
                            } else if hrs > 0 {
                                format!("{}h {}m {}s remaining", hrs, mins, secs)
                            } else if mins > 0 {
                                format!("{}m {}s remaining", mins, secs)
                            } else {
                                format!("{}s remaining", secs)
                            }
                        };

                        let reason_txt = if rec.ban_reason.is_empty() {
                            format!("You are banned from this room ({})", remaining_text)
                        } else {
                            format!("You are banned from this room ({})\n> {}", rec.ban_reason, remaining_text)
                        };

                        let mut client = lock_client(&client)?;
                        writeln!(client.stream, "{}", reason_txt.red())?;
                        return Ok(CommandResult::Handled);
                    }

                    rec.banned      = false;
                    rec.ban_stamp   = 0;
                    rec.ban_length  = 0;
                    rec.ban_reason.clear();

                    let file = File::open("data/rooms.json")?;
                    let reader = BufReader::new(file);
                    let mut rooms_json: Value = serde_json::from_reader(reader)?;

                    if let Some(room_json) = rooms_json.get_mut(&name) {
                        if let Some(user_json) = room_json["users"].get_mut(username) {
                            user_json["banned"]      = json!(false);
                            user_json["ban_stamp"]   = json!(0);
                            user_json["ban_length"]  = json!(0);
                            user_json["ban_reason"]  = json!("");
                        }
                    }

                    let file = OpenOptions::new().write(true).truncate(true).open("data/rooms.json")?;
                    let mut writer = std::io::BufWriter::new(file);
                    let formatter = PrettyFormatter::with_indent(b"    ");
                    let mut ser = Serializer::with_formatter(&mut writer, formatter);
                    rooms_json.serialize(&mut ser)?;
                }
            }

            // Add user to room's rooms.json users list if it's their first time joining
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

                let file = File::open("data/rooms.json")?;
                let reader = BufReader::new(file);
                let mut rooms_json: Value = serde_json::from_reader(reader)?;

                if let Some(room_json) = rooms_json.get_mut(&name) {
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

                    let file = OpenOptions::new().write(true).truncate(true).open("data/rooms.json")?;
                    let mut writer = std::io::BufWriter::new(file);
                    let formatter = PrettyFormatter::with_indent(b"    ");
                    let mut ser = Serializer::with_formatter(&mut writer, formatter);
                    rooms_json.serialize(&mut ser)?;
                }
            }

            // Add user to online list if not already there
            if !room.online_users.contains(username) {
                room.online_users.push(username.clone());
            }

            // Update client state
            let mut client = lock_client(&client)?;
            client.state = ClientState::InRoom {
                username: username.clone(),
                room: name.clone(),
                room_time: Some(SystemTime::now()),
                msg_timestamps: VecDeque::new(),
                inactive_time: Some(SystemTime::now()),
                is_afk: false
            };

            writeln!(client.stream, "{}", format!("/ROOM_STATE"))?;
            writeln!(client.stream, "{}", format!("Joined room: {}", name).green())?;
            Ok(CommandResult::Handled)
        }

        Command::RoomImport { filename } => {
            let safe_filename = if !filename.ends_with(".json") {
                format!("{}.json", filename)
            } else {
                filename
            };

            let import_path = format!("data/vault/rooms/{}", safe_filename);
            let import_file = match File::open(&import_path) {
                Ok(file) => file,
                Err(_) => {
                    let mut client = lock_client(&client)?;
                    writeln!(client.stream, "{}", format!("Error: Could not open {}", import_path).yellow())?;
                    return Ok(CommandResult::Handled);
                }
            };

            let import_reader = BufReader::new(import_file);
            let import_data: Value = match serde_json::from_reader(import_reader) {
                Ok(data) => data,
                Err(_) => {
                    let mut client = lock_client(&client)?;
                    writeln!(client.stream, "{}", "Error: Invalid JSON format in import file".yellow())?;
                    return Ok(CommandResult::Handled);
                }
            };

            let (room_name, room_value) = match import_data.as_object().and_then(|obj| obj.iter().next()) {
                Some((name, val)) => (name.clone(), val.clone()),
                None => {
                    let mut client = lock_client(&client)?;
                    writeln!(client.stream, "{}", "Error: Import file is empty or malformed".yellow())?;
                    return Ok(CommandResult::Handled);
                }
            };

            let _lock = lock_rooms_storage()?;

            let file = File::open("data/rooms.json")?;
            let reader = BufReader::new(file);
            let mut rooms_json: Value = serde_json::from_reader(reader)?;

            if rooms_json.get(&room_name).is_some() {
                let mut client = lock_client(&client)?;
                writeln!(client.stream, "{}", format!("Error: Room {} already exists", room_name).yellow())?;
                return Ok(CommandResult::Handled);
            }

            rooms_json[&room_name] = room_value.clone();
            let file = OpenOptions::new()
                .write(true)
                .truncate(true)
                .open("data/rooms.json")?;
            let mut writer = std::io::BufWriter::new(file);
            let formatter = PrettyFormatter::with_indent(b"    ");
            let mut ser = Serializer::with_formatter(&mut writer, formatter);
            rooms_json.serialize(&mut ser)?;

            let room_obj: Room = match serde_json::from_value(room_value) {
                Ok(room) => room,
                Err(e) => {
                    let mut client = lock_client(&client)?;
                    writeln!(client.stream, "{}", format!("Error: Failed to parse room data: {e}").red())?;
                    return Ok(CommandResult::Handled);
                }
            };

            {
                let mut rooms = lock_rooms(rooms)?;
                rooms.insert(room_name.clone(), Arc::new(Mutex::new(Room {
                    online_users: vec![],
                    ..room_obj
                })));
            }

            let mut client = lock_client(&client)?;
            writeln!(client.stream, "{}", format!("Imported room: {}", room_name).green())?;
            Ok(CommandResult::Handled)
        }

        Command::RoomDelete { name, force } => {
            let _lock = lock_rooms_storage()?;
            let mut rooms = lock_rooms(rooms)?;

            let room_arc = match rooms.get(&name) {
                Some(r) => Arc::clone(r),
                None => {
                    let mut client = lock_client(&client)?;
                    writeln!(client.stream, "{}", format!("Error: Room {} not found", name).yellow())?;
                    return Ok(CommandResult::Handled);
                }
            };

            {
                let room = lock_room(&room_arc)?;
                match room.users.get(username) {
                    Some(user) if user.role == "owner" => (),
                    _ => {
                        let mut client = lock_client(&client)?;
                        writeln!(client.stream, "{}", "Error: Only the room owner can delete this room".yellow())?;
                        return Ok(CommandResult::Handled);
                    }
                }
            }

            if !force {
                let mut client = lock_client(&client)?;
                writeln!(client.stream, "{}", format!("Are you sure you want to delete room {}? (y/n): ", name).red())?;

                let mut reader = BufReader::new(client.stream.try_clone()?);
                loop {
                    let mut line = String::new();
                    let bytes_read = reader.read_line(&mut line)?;
                    if bytes_read == 0 {
                        writeln!(client.stream, "{}", "Connection closed".yellow())?;
                        return Ok(CommandResult::Stop);
                    }

                    match line.trim().to_lowercase().as_str() {
                        "y" => break,
                        "n" => {
                            writeln!(client.stream, "{}", "Room deletion cancelled".yellow())?;
                            return Ok(CommandResult::Handled);
                        }
                        _ => {
                            writeln!(client.stream, "{}", "(y/n): ".red())?;
                        }
                    }
                }
            }

            rooms.remove(&name);

            let file = File::open("data/rooms.json")?;
            let reader = BufReader::new(file);
            let mut rooms_json: Value = serde_json::from_reader(reader)?;

            if rooms_json.get(&name).is_some() {
                if let Some(map) = rooms_json.as_object_mut() {
                    map.remove(&name);
                } else {
                    let mut client = lock_client(&client)?;
                    writeln!(client.stream, "{}", "Error: Malformed rooms.json".red())?;
                    return Ok(CommandResult::Handled);
                }
                let file = OpenOptions::new().write(true).truncate(true).open("data/rooms.json")?;
                let mut writer = std::io::BufWriter::new(file);
                let formatter = PrettyFormatter::with_indent(b"    ");
                let mut ser = Serializer::with_formatter(&mut writer, formatter);
                rooms_json.serialize(&mut ser)?;
            }

            let mut client = lock_client(&client)?;
            writeln!(client.stream, "{}", format!("Room {} deleted successfully", name).green())?;
            Ok(CommandResult::Handled)
        }

        Command::InvalidSyntax {err_msg } => {
            let mut client = lock_client(&client)?;
            writeln!(client.stream, "{}", err_msg)?;
            Ok(CommandResult::Handled)
        }

        Command::Unavailable => {
            let mut client = lock_client(&client)?;
            writeln!(client.stream, "{}", "Command not available, use /help to see available commands".red())?;
            Ok(CommandResult::Handled)
        }
    }
}
