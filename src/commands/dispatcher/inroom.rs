use std::io::{self, BufRead, Write};
use std::sync::{Arc, Mutex};
use std::collections::{HashMap};
use std::time::{SystemTime, UNIX_EPOCH};
use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter};
use serde::Serialize;
use serde_json::{json, Serializer, Value};
use serde_json::ser::PrettyFormatter;
use colored::*;

use crate::commands::parser::Command;
use crate::commands::command_utils::{help_msg_inroom, ColorizeExt, has_permission, save_rooms_to_disk, command_order, RESTRICTED_COMMANDS, unix_timestamp, parse_duration};
use crate::types::{Client, Clients, ClientState, Rooms, RoomUser};
use crate::utils::{broadcast_message, check_mute, lock_client, lock_clients, lock_room, lock_rooms, lock_rooms_storage, lock_users_storage};
use super::CommandResult;

pub fn inroom_command(cmd: Command, client: Arc<Mutex<Client>>, clients: &Clients, rooms: &Rooms, username: &String, room: &String) -> io::Result<CommandResult> {
    if !has_permission(&cmd, client.clone(), rooms, username, room)? {
        return Ok(CommandResult::Handled);
    }

    match cmd {
        Command::Help => {
            let rooms_map = lock_rooms(rooms)?;
            let room_arc = match rooms_map.get(room) {
                Some(arc) => arc,
                None => {
                    let mut client = lock_client(&client)?;
                    writeln!(client.stream, "{}", "Error: Room not found".red())?;
                    return Ok(CommandResult::Handled);
                }
            };

            let room_guard = lock_room(&room_arc)?;

            let role = match room_guard.users.get(username) {
                Some(u) => u.role.as_str(),
                None => "user",
            };

            let role_cmds: Vec<&str> = match role {
                "moderator" => room_guard.roles.moderator.iter().map(|s| s.as_str()).collect(),
                "user" => room_guard.roles.user.iter().map(|s| s.as_str()).collect(),
                "admin" | "owner" => vec![
                    "afk", "announce", "seen", "msg", "me", "super", "user", "mod"
                ],
                _ => Vec::new(),
            };

            let mut client = lock_client(&client)?;
            writeln!(client.stream, "{}", help_msg_inroom(role_cmds).bright_blue())?;
            Ok(CommandResult::Handled)
        }

        Command::Ping => {
            let mut client = lock_client(&client)?;
            writeln!(client.stream, "{}", "PONG.".green())?;
            Ok(CommandResult::Handled)
        }

        Command::Quit => {
            let addr = {
                let c = lock_client(&client)?;
                c.addr
            };

            {
                let rooms_map = lock_rooms(rooms)?;
                if let Some(room_arc) = rooms_map.get(room) {
                    if let Ok(mut room_guard) = room_arc.lock() {
                        room_guard.online_users.retain(|u| u != username);
                    }
                }
            }

            // Get current unix time and update client's last_seen value for their current room
            if let Err(e) = unix_timestamp(rooms, room, username) {
                eprintln!("Error updating last_seen for {username} in {room}: {e}");
            }

            {
                let mut clients = lock_clients(&clients)?;
                clients.remove(&addr);
            }
            
            let mut client = lock_client(&client)?;
            writeln!(client.stream, "{}", "Exiting...".green())?;
            client.stream.shutdown(std::net::Shutdown::Both)?;
            Ok(CommandResult::Stop)
        }

        Command::Leave => {
            {
                let rooms_map = lock_rooms(rooms)?;
                if let Some(room_arc) = rooms_map.get(room) {
                    if let Ok(mut room) = room_arc.lock() {
                        room.online_users.retain(|u| u != username);
                    }
                }
            }

            // Get current unix time and update client's last_seen value for their current room
            if let Err(e) = unix_timestamp(rooms, room, username) {
                eprintln!("Error updating last_seen for {username} in {room}: {e}");
            }

            let mut client = lock_client(&client)?;
            client.state = ClientState::LoggedIn {
                username: username.clone()
            };

            writeln!(client.stream, "{}", format!("You have left {}", room).green())?;
            Ok(CommandResult::Handled)
        }
        
        Command::Status => {
            let rooms_map = lock_rooms(rooms)?;
            let room_arc = match rooms_map.get(room) {
                Some(r) => Arc::clone(r),
                None => {
                    let mut client = lock_client(&client)?;
                    writeln!(client.stream, "{}", format!("Room {} not found", room).yellow())?;
                    return Ok(CommandResult::Handled);
                }
            };

            let room_guard = lock_room(&room_arc)?;
            let user_info = match room_guard.users.get(username) {
                Some(info) => info,
                None => {
                    let mut client = lock_client(&client)?;
                    writeln!(client.stream, "{}", "Error: Your user info is missing in this room".red())?;
                    return Ok(CommandResult::Handled);
                }
            };

            let mut client = lock_client(&client)?;
            let joined_at = match &client.state {
                ClientState::InRoom { room_time: Some(t), .. } => *t,
                _ => {
                    writeln!(client.stream, "{}", "Error: Could not determine join time".red())?;
                    return Ok(CommandResult::Handled);
                }
            };

            let duration = joined_at.elapsed().map_err(|e| {
                io::Error::new(io::ErrorKind::Other, format!("Time error: {e}"))
            })?;
            let secs = duration.as_secs() % 60;
            let mins = duration.as_secs() / 60;
            let hrs = mins / 60;

            let role = {
                let mut c = user_info.role.chars();
                match c.next() {
                    Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                    None => String::new(),
                }
            };
            let color_display = if user_info.color.is_empty() {
                "Default".to_string()
            } else {
                format!("{}", user_info.color).truecolor_from_hex(&user_info.color).to_string()
            };

            let visibility = if user_info.hidden {
                "True".yellow().to_string()
            } else {
                "False".green().to_string()
            };

            let mute_status = if user_info.muted {
                if user_info.mute_length == 0 {
                    "Muted (Permanent)".red().to_string()
                } else {
                    let now = match SystemTime::now().duration_since(UNIX_EPOCH) {
                        Ok(dur) => dur.as_secs(),
                        Err(_) => 0,
                    };
                    let expire = user_info.mute_stamp.saturating_add(user_info.mute_length);
                    if now >= expire {
                        "Not Muted".green().to_string()
                    } else {
                        let rem = expire - now;
                        let d = rem / 86_400;
                        let h = (rem % 86_400) / 3_600;
                        let m = (rem % 3_600) / 60;
                        let s = rem % 60;
                        format!("Muted ({}d {}h {}m {}s left)", d, h, m, s).red().to_string()
                    }
                }
            } else {
                "False".green().to_string()
            };

            writeln!(client.stream, "{}\n{} {}\n{} {}\n{} {}\n{} {}\n{} {}\n{} {}",
                format!("Status for {} in {}:", username, room).green(),
                "> Role:", role,
                "> Nickname:", if user_info.nick.is_empty() { "None".to_string() } else { user_info.nick.italic().to_string() },
                "> Color:", color_display,
                "> Hidden:", visibility,
                "> Mute:", mute_status,
                "> Session:", format!("{:0>2}:{:0>2}:{:0>2}", hrs, mins, secs)
            )?;
            Ok(CommandResult::Handled)
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

        Command::AFK => {
            let mut client = lock_client(&client)?;
            if let ClientState::InRoom { is_afk, .. } = &mut client.state {
                *is_afk = true;
            }
            writeln!(client.stream, "{}", "You are now set as AFK".yellow())?;
            Ok(CommandResult::Handled)
        }

        Command::DM { recipient, message } => {
            let rooms_map = lock_rooms(rooms)?;
            let room_arc = match rooms_map.get(room) {
                Some(r) => Arc::clone(r),
                None => {
                    let mut client = lock_client(&client)?;
                    writeln!(client.stream, "{}", format!("Room {} not found", room).yellow())?;
                    return Ok(CommandResult::Handled);
                }
            };

            let room_guard = lock_room(&room_arc)?;
            if !room_guard.online_users.contains(&recipient) {
                let mut client = lock_client(&client)?;
                writeln!(client.stream, "{}", format!("{} is not currently online", recipient).yellow())?;
                return Ok(CommandResult::Handled);
            }

            {
                let mut client = lock_client(&client)?;
                if client.ignore_list.contains(&recipient) {
                    writeln!(client.stream, "{}", format!("Cannot send message to {}, you have them ignored", recipient).yellow())?;
                    return Ok(CommandResult::Handled);
                }
            }

            let clients_map = lock_clients(clients)?;
            let mut found = false;
            for client_arc in clients_map.values() {
                let mut c = match client_arc.lock() {
                    Ok(guard) => guard,
                    Err(poisoned) => {
                        eprintln!("Client lock poisoned: {poisoned}");
                        continue;
                    }
                };

                match &c.state {
                    ClientState::InRoom { username: uname, room: rname, .. }
                        if uname == &recipient && rname == room => {
                            if c.ignore_list.contains(username) {
                                found = true;
                                break;
                            }

                            writeln!(c.stream, "{}", format!("(Private) {}: {}", username, message).cyan().italic().to_string())?;
                            found = true;
                            break;
                        }
                    _ => continue,
                }
            }

            let mut client = lock_client(&client)?;
            if found {
                writeln!(client.stream, "{}", format!("Message sent to {}", recipient).green())?;
            }
            else {
                writeln!(client.stream, "{}", format!("Failed to deliver message to {}", recipient).red())?;
            }

            Ok(CommandResult::Handled)
        }

        Command::Me { action } => {
            if let Some(msg) = check_mute(rooms, room, username)? {
                let mut client = lock_client(&client)?;
                writeln!(client.stream, "{}", msg.red())?;
                return Ok(CommandResult::Handled);
            }

            let msg = format!("* {} {}", username, action).bright_green().to_string();
            broadcast_message(clients, room, username, &msg, true, false)?;
            Ok(CommandResult::Handled)
        }

        Command::Seen { username } => { 
            let rooms_map = lock_rooms(rooms)?;
            let room_arc = match rooms_map.get(room) {
                Some(r) => Arc::clone(r),
                None => {
                    let mut client = lock_client(&client)?;
                    writeln!(client.stream, "{}", "Room not found".yellow())?;
                    return Ok(CommandResult::Handled);
                }
            };
            let room_guard = lock_room(&room_arc)?;

            let is_online = room_guard.online_users.iter().any(|u| u == &username);

            let response = if is_online {
                format!("{username} is online now").green().to_string()
            } else {
                match room_guard.users.get(&username) {
                    Some(info) => {
                        let now_secs = match SystemTime::now().duration_since(UNIX_EPOCH) {
                            Ok(d)  => d.as_secs(),
                            Err(_) => 0,
                        };
                        let diff = now_secs.saturating_sub(info.last_seen); // last_seen is u64

                        let days = diff / 86_400;
                        let hrs  = (diff % 86_400) / 3_600;
                        let mins = (diff % 3_600) / 60;
                        let secs = diff % 60;

                        format!(
                            "{username} was last seen {}d {}h {}m {}s ago",
                            days, hrs, mins, secs
                        ).green().to_string()
                    }
                    None => format!("{username} has never joined this room").yellow().to_string(),
                }
            };

            let mut client = lock_client(&client)?;
            writeln!(client.stream, "{response}")?;
            Ok(CommandResult::Handled)
        }

        Command::Announce { message } => {
            if let Some(msg) = check_mute(rooms, room, username)? {
                let mut client = lock_client(&client)?;
                writeln!(client.stream, "{}", msg.red())?;
                return Ok(CommandResult::Handled);
            }

            let msg = format!("Announcement: {}", message).bright_yellow().to_string();
            broadcast_message(clients, room, username, &msg, true, true)?;
            Ok(CommandResult::Handled)
        }

        Command::AccountRegister { .. } | Command::AccountLogin { .. } => {
            let mut client = lock_client(&client)?;
            writeln!(client.stream, "{}", "Already logged in".yellow())?;
            Ok(CommandResult::Handled)
        }

        Command::SuperUsers => {
            let rooms_map = lock_rooms(rooms)?;
            let room_arc = match rooms_map.get(room) {
                Some(r) => Arc::clone(r),
                None => {
                    let mut client = lock_client(&client)?;
                    writeln!(client.stream, "{}", format!("Room {} not found", room).yellow())?;
                    return Ok(CommandResult::Handled);
                }
            };
            let room_guard = lock_room(&room_arc)?;

            let mut cli = lock_client(&client)?;
            writeln!(cli.stream, "{}", format!("User data for {}:", room).green())?;
            drop(cli);
            
            let clients_map = lock_clients(clients)?;

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
                    format!("{}", udata.color).truecolor_from_hex(&udata.color).to_string()
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

                let afk_status = {
                    let mut afk = false;
                    for c_arc in clients_map.values() {
                        if let Ok(c) = c_arc.lock() {
                            if let ClientState::InRoom { username, room: rnm, is_afk, .. } = &c.state {
                                if username == uname && rnm == room {
                                    afk = *is_afk;
                                    break;
                                }
                            }
                        }
                    }
                    if afk {
                        "True".yellow().to_string()
                    } else {
                        "False".green().to_string()
                    }
                };

                let session_time = {
                    let mut secs = 0u64;
                    for c_arc in clients_map.values() {
                        if let Ok(c) = c_arc.lock() {
                            if let ClientState::InRoom {username: u, room: rnm, room_time: Some(t), ..} = &c.state {
                                if u == uname && rnm == room {
                                    if let Ok(elapsed) = t.elapsed() {
                                        secs = elapsed.as_secs();
                                    }
                                    break;
                                }
                            }
                        }
                    }
                    let h = secs / 3600;
                    let m = (secs % 3600) / 60;
                    let s = secs % 60;
                    format!("{:0>2}:{:0>2}:{:0>2}", h, m, s)
                };

                let mut client = lock_client(&client)?;
                writeln!(client.stream, "> {} - Role: {}, Nickname: {}, Color: {}, Hidden: {}, AFK: {}, Session: {}",
                    uname.green(),
                    role,
                    nickname,
                    color_display,
                    hidden_status,
                    afk_status,
                    session_time
                )?;
            }

            Ok(CommandResult::Handled)
        }

        Command::SuperRename { name } => {
            let mut rooms_map = lock_rooms(rooms)?;
            let old_name = room.clone();

            if rooms_map.contains_key(&name) {
                let mut client = lock_client(&client)?;
                writeln!(client.stream, "{}", format!("Room name '{}' is already taken", name).yellow())?;
                return Ok(CommandResult::Handled);
            }

            let room_arc = match rooms_map.remove(&old_name) {
                Some(r) => r,
                None => {
                    let mut client = lock_client(&client)?;
                    writeln!(client.stream, "{}", format!("Room '{}' not found", old_name).yellow())?;
                    return Ok(CommandResult::Handled);
                }
            };

            rooms_map.insert(name.clone(), Arc::clone(&room_arc));

            // Update all clients currently in the renamed room
            let clients_map = lock_clients(clients)?;
            for client_arc in clients_map.values() {
                let mut c = lock_client(client_arc)?;
                if let ClientState::InRoom { room: r, .. } = &mut c.state {
                    if r == &old_name {
                        *r = name.clone();
                    }
                }
            }

            // Save updated room list to disk
            let _room_save_lock = lock_rooms_storage()?;
            let serializable_map: HashMap<_, _> = rooms_map.iter()
                .filter_map(|(k, v)| {
                    match lock_room(v) {
                        Ok(guard) => Some((k.clone(), guard.clone())),
                        Err(e) => {
                            eprintln!("Failed to lock room '{}': {}", k, e);
                            None
                        }
                    }
                })
                .collect();

            let serialized = match serde_json::to_string_pretty(&serializable_map) {
                Ok(json) => json,
                Err(e) => {
                    let mut client = lock_client(&client)?;
                    writeln!(client.stream, "{}", format!("Failed to serialize rooms: {e}").red())?;
                    return Ok(CommandResult::Handled);
                }
            };

            if let Err(e) = std::fs::write("data/rooms.json", serialized) {
                let mut client = lock_client(&client)?;
                writeln!(client.stream, "{}", format!("Failed to write to disk: {e}").red())?;
                return Ok(CommandResult::Handled);
            }

            let mut client = lock_client(&client)?;
            writeln!(client.stream, "{}", format!("Room renamed from '{}' to '{}'", old_name, name).green())?;
            Ok(CommandResult::Handled)
        }

        Command::SuperExport { filename } => {
            let file = match std::fs::File::open("data/rooms.json") {
                Ok(f)  => f,
                Err(e) => {
                    let mut client = lock_client(&client)?;
                    writeln!(client.stream, "{}", format!("Error opening rooms.json: {e}").red())?;
                    return Ok(CommandResult::Handled);
                }
            };
            
            let reader = BufReader::new(file);
            let rooms_json: serde_json::Value = match serde_json::from_reader(reader) {
                Ok(v)  => v,
                Err(e) => {
                    let mut client = lock_client(&client)?;
                    writeln!(client.stream, "{}", format!("Malformed rooms.json: {e}").red())?;
                    return Ok(CommandResult::Handled);
                }
            };

            let room_data = match rooms_json.get(room) {
                Some(v) => v.clone(),
                None => {
                    let mut client = lock_client(&client)?;
                    writeln!(client.stream, "{}", "Error: room data not found".yellow())?;
                    return Ok(CommandResult::Handled);
                }
            };

            let final_filename = if filename.is_empty() {
                let stamp = chrono::Local::now().format("%y%m%d%H%M%S").to_string();
                format!("{room}_{stamp}.json")
            } else if filename.ends_with(".json") {
                filename
            } else {
                format!("{filename}.json")
            };

            let export_path = format!("data/vault/rooms/{final_filename}");
            let export_file = match OpenOptions::new().create(true).write(true).truncate(true).open(&export_path) {
                Ok(f)  => f,
                Err(e) => {
                    let mut client = lock_client(&client)?;
                    writeln!(client.stream, "{}", format!("Error creating {export_path}: {e}").red())?;
                    return Ok(CommandResult::Handled);
                }
            };

            let mut writer = BufWriter::new(export_file);
            let formatter = PrettyFormatter::with_indent(b"    ");
            let mut ser = Serializer::with_formatter(&mut writer, formatter);
            json!({ room: room_data }).serialize(&mut ser)?;

            let mut client = lock_client(&client)?;
            writeln!(client.stream, "{}", format!("Exported room data to: {final_filename}").green())?;
            Ok(CommandResult::Handled)
        }

        Command::SuperWhitelist => {
            let rooms_map = lock_rooms(rooms)?;
            let room_arc = match rooms_map.get(room) {
                Some(arc) => arc,
                None => {
                    let mut client = lock_client(&client)?;
                    writeln!(client.stream, "{}", format!("Room '{}' not found", room).yellow())?;
                    return Ok(CommandResult::Handled);
                }
            };

            let room_guard = lock_room(room_arc)?;
            let mut client = lock_client(&client)?;

            if room_guard.whitelist_enabled {
                writeln!(client.stream, "{}", "- Whitelist is currently ENABLED -".green())?;

                if room_guard.whitelist.is_empty() {
                    writeln!(client.stream, "{}", "  > No users are currently whitelisted".green())?;
                } else {
                    writeln!(client.stream, "{}", "Whitelisted users:".green())?;
                    for user in &room_guard.whitelist {
                        writeln!(client.stream, "  > {}", user.cyan())?;
                    }
                }
            } else {
                writeln!(client.stream, "{}", "- Whitelist is currently DISABLED -".green())?;
            }

            Ok(CommandResult::Handled)
        }

        Command::SuperWhitelistToggle => {
            let rooms_map = lock_rooms(rooms)?;
            let room_arc = match rooms_map.get(room) {
                Some(arc) => Arc::clone(arc),
                None => {
                    let mut client = lock_client(&client)?;
                    writeln!(client.stream, "{}", format!("Room '{}' not found", room).yellow())?;
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
                            let mut client = lock_client(&client)?;
                            writeln!(client.stream, "{}", format!("Failed to write rooms.json: {}", e).red())?;
                            return Ok(CommandResult::Handled);
                        }
                    }
                    Err(e) => {
                        let mut client = lock_client(&client)?;
                        writeln!(client.stream, "{}", format!("Failed to serialize rooms: {}", e).red())?;
                        return Ok(CommandResult::Handled);
                    }
                }
            } else {
                let mut client = lock_client(&client)?;
                writeln!(client.stream, "{}", "Failed to acquire room save lock".red())?;
                return Ok(CommandResult::Handled);
            }

            let room_guard = lock_room(&room_arc)?;
            let mut client = lock_client(&client)?;
            if room_guard.whitelist_enabled {
                writeln!(client.stream, "{}", "Whitelist is now ENABLED".green())?;
            } else {
                writeln!(client.stream, "{}", "Whitelist is now DISABLED".green())?;
            }

            Ok(CommandResult::Handled)
        }

        Command::SuperWhitelistAdd { users } => {
            let rooms_map = lock_rooms(rooms)?;
            let room_arc = match rooms_map.get(room) {
                Some(arc) => Arc::clone(arc),
                None => {
                    let mut client = lock_client(&client)?;
                    writeln!(client.stream, "{}", format!("Room '{}' not found", room).yellow())?;
                    return Ok(CommandResult::Handled);
                }
            };

            let mut client = lock_client(&client)?;
            let mut added_any = false;
            {
                let mut room_guard = lock_room(&room_arc)?;

                for user in users.split_whitespace() {
                    if room_guard.whitelist.contains(&user.to_string()) {
                        writeln!(client.stream, "{}", format!("'{}' is already whitelisted", user).cyan())?;
                    } else {
                        room_guard.whitelist.push(user.to_string());
                        writeln!(client.stream, "{}", format!("Added '{}' to the whitelist", user).green())?;
                        added_any = true;
                    }
                }
            }

            if added_any {
                if let Err(e) = save_rooms_to_disk(&rooms_map) {
                    writeln!(client.stream, "{}", format!("Failed to save rooms: {}", e).red())?;
                }
            }

            Ok(CommandResult::Handled)
        }

        Command::SuperWhitelistRemove { users } => {
            let rooms_map = lock_rooms(rooms)?;
            let room_arc = match rooms_map.get(room) {
                Some(arc) => Arc::clone(arc),
                None => {
                    let mut client = lock_client(&client)?;
                    writeln!(client.stream, "{}", format!("Room '{}' not found", room).yellow())?;
                    return Ok(CommandResult::Handled);
                }
            };

            let mut client = lock_client(&client)?;
            let mut removed_any = false;
            {
                let mut room_guard = lock_room(&room_arc)?;

                for user in users.split_whitespace() {
                    if let Some(pos) = room_guard.whitelist.iter().position(|u| u == user) {
                        room_guard.whitelist.remove(pos);
                        writeln!(client.stream, "{}", format!("Removed '{}' from the whitelist", user).green())?;
                        removed_any = true;
                    } else {
                        writeln!(client.stream, "{}", format!("'{}' is not in the whitelist", user).cyan())?;
                    }
                }
            }

            if removed_any {
                if let Err(e) = save_rooms_to_disk(&rooms_map) {
                    writeln!(client.stream, "{}", format!("Failed to save rooms: {}", e).red())?;
                }
            }

            Ok(CommandResult::Handled)
        }

        Command::SuperLimit => {
            let rooms_map = lock_rooms(rooms)?;
            let room_arc = match rooms_map.get(room) {
                Some(r) => Arc::clone(r),
                None => {
                    let mut client = lock_client(&client)?;
                    writeln!(client.stream, "{}", format!("Room {} not found", room).yellow())?;
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

            let mut client = lock_client(&client)?;
            writeln!(client.stream, "{}\n  > Message rate: {} messages per 5 sec\n  > Session timeout: {} sec of inactivity", "Current limits:".green(), format!("{}", rate_display).green(), format!("{}", timeout_display).green())?;
            Ok(CommandResult::Handled)
        }

        Command::SuperLimitRate { limit } => {            
            let rooms_map   = lock_rooms(rooms)?;

            {
                let _store_lock = lock_rooms_storage()?;
                let room_arc    = match rooms_map.get(room) {
                    Some(r) => Arc::clone(r),
                    None => {
                        let mut client = lock_client(&client)?;
                        writeln!(client.stream, "{}", "Room not found".yellow())?;
                        return Ok(CommandResult::Handled);
                    }
                };
                let mut room = lock_room(&room_arc)?;
                room.msg_rate = limit;
            }

            let mut client = lock_client(&client)?;
            
            if let Err(e) = save_rooms_to_disk(&rooms_map) {
                writeln!(client.stream, "{}", format!("Failed to save rooms: {e}").red())?;
                return Ok(CommandResult::Handled);
            }

            if limit == 0 {
                writeln!(client.stream, "{}", "Message rate limit set to UNLIMITED".green())?;
            } else {
                writeln!(client.stream, "{}", format!("Message rate limit set to {limit} sec").green())?;
            }
            
            Ok(CommandResult::Handled)
        }

        Command::SuperLimitSession { limit } => {
            let rooms_map   = lock_rooms(rooms)?;

            {
                let _store_lock = lock_rooms_storage()?;
                let room_arc    = match rooms_map.get(room) {
                    Some(r) => Arc::clone(r),
                    None => {
                        let mut client = lock_client(&client)?;
                        writeln!(client.stream, "{}", "Room not found".yellow())?;
                        return Ok(CommandResult::Handled);
                    }
                };
                let mut room = lock_room(&room_arc)?;
                room.session_timeout = limit;
            }

            let mut client = lock_client(&client)?;

            if let Err(e) = save_rooms_to_disk(&rooms_map) {
                writeln!(client.stream, "{}", format!("Failed to save rooms: {e}").red())?;
                return Ok(CommandResult::Handled);
            }

            if limit == 0 {
                writeln!(client.stream, "{}", "Session timeout set to UNLIMITED".green())?;
            } else {
                writeln!(client.stream, "{}", format!("Session timeout set to {limit} sec").green())?;
            }

            Ok(CommandResult::Handled)
        }

        Command::SuperRoles => {
            let rooms_map = lock_rooms(rooms)?;
            let room_arc = match rooms_map.get(room) {
                Some(r) => Arc::clone(r),
                None => {
                    let mut client = lock_client(&client)?;
                    writeln!(client.stream, "{}", format!("Room {room} not found").yellow())?;
                    return Ok(CommandResult::Handled);
                }
            };
            let room_guard = lock_room(&room_arc)?;

            let mod_cmds = &room_guard.roles.moderator;
            let user_cmds = &room_guard.roles.user;

            let all_cmds: Vec<&str> = command_order()
                .into_iter()
                .filter(|c| RESTRICTED_COMMANDS.contains(c))
                .collect();

            let mut lines = Vec::<String>::new();
            lines.push("Role info:".to_string());
            lines.push("(Owners and admins can access every command)".to_string());

            for cmd in all_cmds {
                let m_disp = if mod_cmds.contains(&cmd.to_string()) {
                    "M".bright_yellow().bold().to_string()   // orange, bold
                } else {
                    " ".to_string()
                };

                let u_disp = if user_cmds.contains(&cmd.to_string()) {
                    "U".white().bold().to_string()                         // white, bold
                } else {
                    " ".to_string()
                };

                let indent = if cmd.contains('.') { "   " } else { "" };
                lines.push(format!("  > {m_disp} {u_disp} {indent}{cmd}"));
            }

            let mut client = lock_client(&client)?;
            writeln!(client.stream, "{}", lines.join("\n").green())?;
            Ok(CommandResult::Handled)
        }
                
        Command::SuperRolesAdd { role, commands } => {
            let target_role = match role.to_lowercase().as_str() {
                "user" => "user",
                "mod" | "moderator" => "moderator",
                _ => {
                    let mut client = lock_client(&client)?;
                    writeln!(client.stream, "{}", "Error: Role must be user|mod".yellow())?;
                    return Ok(CommandResult::Handled);
                }
            };

            let cmd_tokens: Vec<&str> = commands.split_whitespace().collect();

            let invalid: Vec<String> = cmd_tokens.iter().filter(|c| !RESTRICTED_COMMANDS.contains(**c)).map(|c| (*c).to_string()).collect();
            if !invalid.is_empty() {
                let mut client = lock_client(&client)?;
                writeln!(client.stream, "{}", format!("Error: Unknown commands: {}", invalid.join(", ")).yellow())?;
                return Ok(CommandResult::Handled);
            }

            let mut added = Vec::<String>::new();
            {
                let _store_lock = lock_rooms_storage()?;
                let rooms_map   = lock_rooms(rooms)?;
                let room_arc    = match rooms_map.get(room) {
                    Some(r) => Arc::clone(r),
                    None => {
                        let mut client = lock_client(&client)?;
                        writeln!(client.stream, "{}", format!("Room {room} not found").yellow())?;
                        return Ok(CommandResult::Handled);
                    }
                };
                let mut room_guard = lock_room(&room_arc)?;

                let list = if target_role == "moderator" {
                    &mut room_guard.roles.moderator
                } else {
                    &mut room_guard.roles.user
                };

                for &c in &cmd_tokens {
                    let c_str = c.to_string();
                    if !list.contains(&c_str) {
                        list.push(c_str.clone());
                        added.push(c_str);
                    }
                }
            }

            if !added.is_empty() {
                let rooms_map = lock_rooms(rooms)?;
                if let Err(e) = save_rooms_to_disk(&rooms_map) {
                    let mut client = lock_client(&client)?;
                    writeln!(client.stream, "{}", format!("Failed to save rooms: {e}").red())?;
                    return Ok(CommandResult::Handled);
                }
            }

            let mut client = lock_client(&client)?;
            if added.is_empty() {
                writeln!(client.stream, "{}", "No changes made".yellow())?;
            } else {
                writeln!(client.stream, "{}", format!("Added for {target_role}: {}", added.join(", ")).green())?;
            }
            Ok(CommandResult::Handled)
        }
        
        Command::SuperRolesRevoke { role, commands } => {
            let target_role = match role.to_lowercase().as_str() {
                "user" => "user",
                "mod" | "moderator" => "moderator",
                _ => {
                    let mut client = lock_client(&client)?;
                    writeln!(client.stream, "{}", "Error: Role must be user|mod".yellow())?;
                    return Ok(CommandResult::Handled);
                }
            };

            let cmd_tokens: Vec<&str> = commands.split_whitespace().collect();

            let invalid: Vec<String> = cmd_tokens.iter().filter(|c| !RESTRICTED_COMMANDS.contains(**c)).map(|c| (*c).to_string()).collect();
            if !invalid.is_empty() {
                let mut client = lock_client(&client)?;
                writeln!(client.stream, "{}", format!("Error: Unknown commands: {}", invalid.join(", ")).yellow())?;
                return Ok(CommandResult::Handled);
            }

            let mut removed = Vec::<String>::new();
            {
                let _store_lock = lock_rooms_storage()?;
                let rooms_map   = lock_rooms(rooms)?;
                let room_arc    = match rooms_map.get(room) {
                    Some(r) => Arc::clone(r),
                    None => {
                        let mut client = lock_client(&client)?;
                        writeln!(client.stream, "{}", format!("Room {room} not found").yellow())?;
                        return Ok(CommandResult::Handled);
                    }
                };
                let mut room_guard = lock_room(&room_arc)?;

                let list = if target_role == "moderator" {
                    &mut room_guard.roles.moderator
                } else {
                    &mut room_guard.roles.user
                };

                list.retain(|existing| {
                    let keep = !cmd_tokens.iter().any(|c| c == existing);
                    if !keep {
                        removed.push(existing.clone());
                    }
                    keep
                });
            }

            if !removed.is_empty() {
                let rooms_map = lock_rooms(rooms)?;
                if let Err(e) = save_rooms_to_disk(&rooms_map) {
                    let mut client = lock_client(&client)?;
                    writeln!(client.stream, "{}", format!("Failed to save rooms: {}", e).red())?;
                    return Ok(CommandResult::Handled);
                }
            }

            let mut client = lock_client(&client)?;
            if removed.is_empty() {
                writeln!(client.stream, "{}", "No changes made".yellow())?;
            } else {
                writeln!(client.stream, "{}", format!("Revoked for {target_role}: {}", removed.join(", ")).green())?;
            }
            Ok(CommandResult::Handled)
        }
        
        Command::SuperRolesAssign { role, users } => {
            let target_role = match role.to_lowercase().as_str() {
                "usr" | "user" => "user",
                "mod" | "moderator" => "mod",
                "admin" | "administrator" => "admin",
                "owner" | "creator" | "founder" => "owner",
                _ => {
                    let mut client = lock_client(&client)?;
                    writeln!(client.stream, "{}", "Error: Role must be user|mod|admin|owner".yellow())?;
                    return Ok(CommandResult::Handled);
                }
            };

            let users_vec: Vec<&str> = users.split_whitespace().collect();
            if users_vec.is_empty() {
                let mut client = lock_client(&client)?;
                writeln!(client.stream, "{}", "Error: No users specified".yellow())?;
                return Ok(CommandResult::Handled);
            }

            if target_role == "owner" {
                if users_vec.len() != 1 {
                    let mut client = lock_client(&client)?;
                    writeln!(client.stream, "{}", "Error: Only 1 user may be assigned to owner".yellow())?;
                    return Ok(CommandResult::Handled);
                }
            }

            let mut assigned = Vec::<String>::new();
            {
                let _store_lock = lock_rooms_storage()?;
                let rooms_map   = lock_rooms(rooms)?;
                let room_arc    = match rooms_map.get(room) {
                    Some(r) => Arc::clone(r),
                    None => {
                        let mut client = lock_client(&client)?;
                        writeln!(client.stream, "{}", format!("Room {room} not found").yellow())?;
                        return Ok(CommandResult::Handled);
                    }
                };
                let mut room_guard = lock_room(&room_arc)?;

                if target_role == "owner" {
                    match room_guard.users.get(username) {
                        Some(u) if u.role == "owner" => (),
                        _ => {
                            let mut client = lock_client(&client)?;
                            writeln!(client.stream, "{}", "Error: Only the room owner can transfer ownership".yellow())?;
                            return Ok(CommandResult::Handled);
                        }
                    }
                }

                let new_owner = users_vec[0];
                if target_role == "owner" {
                    let mut client = lock_client(&client)?;
                    writeln!(client.stream, "{}", format!("Assigning {new_owner} as owner will transfer room ownership to them. Are you sure you want to do this? (y/n): ").red())?;
                    let mut reader = std::io::BufReader::new(client.stream.try_clone()?);
                    loop {
                        let mut line = String::new();
                        let bytes = reader.read_line(&mut line)?;
                        if bytes == 0 {
                            writeln!(client.stream, "{}", "Connection closed".yellow())?;
                            return Ok(CommandResult::Stop);
                        }
                        match line.trim().to_lowercase().as_str() {
                            "y" => break,
                            "n" => {
                                writeln!(client.stream, "{}", "Owner transfer cancelled".yellow())?;
                                return Ok(CommandResult::Handled);
                            }
                            _ => {
                                writeln!(client.stream, "{}", "(y/n): ".red())?;
                            }
                        }
                    }
                }

                for &u in &users_vec {
                    let entry = room_guard.users.entry(u.to_string()).or_insert(RoomUser {
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
                    if entry.role != target_role {
                        entry.role = target_role.to_string();
                        assigned.push(u.to_string());
                    }
                }

                if new_owner != username {
                    if let Some(cur_owner) = room_guard.users.get_mut(username) {
                        if cur_owner.role == "owner" {
                            cur_owner.role = "admin".to_string();
                        }
                    }
                }

                for &u in &users_vec {
                    if !assigned.contains(&u.to_string()) {
                    }
                }
            }

            if !assigned.is_empty() {
                let rooms_map = lock_rooms(rooms)?;
                if let Err(e) = save_rooms_to_disk(&rooms_map) {
                    let mut client = lock_client(&client)?;
                    writeln!(client.stream, "{}", format!("Failed to save rooms: {e}").red())?;
                    return Ok(CommandResult::Handled);
                }
            }

            let mut client = lock_client(&client)?;
            if assigned.is_empty() {
                writeln!(client.stream, "{}", "No role changes made".yellow())?;
            } else {
                writeln!(client.stream, "{}", format!("Assigned role '{target_role}' to: {}", assigned.join(", ")).green())?;
            }

            Ok(CommandResult::Handled)
        }

        Command::SuperRolesRecolor { role, color } => {
            let role_key = match role.to_lowercase().as_str() {
                "user"        => "user",
                "mod" | "moderator" => "mod",
                "admin"       => "admin",
                "owner"       => "owner",
                _ => {
                    let mut client = lock_client(&client)?;
                    writeln!(client.stream, "{}", "Error: Role must be user|mod|admin|owner".yellow())?;
                    return Ok(CommandResult::Handled);
                }
            };

            let hex = color.trim().trim_start_matches('#');
            if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
                let mut client = lock_client(&client)?;
                writeln!(client.stream, "{}", "Error: Color must be a 6‑digit hex value".yellow())?;
                return Ok(CommandResult::Handled);
            }
            let hex_with_hash = format!("#{hex}");

            {
                let _store_lock = lock_rooms_storage()?;
                let rooms_map   = lock_rooms(rooms)?;
                let room_arc    = match rooms_map.get(room) {
                    Some(r) => Arc::clone(r),
                    None => {
                        let mut client = lock_client(&client)?;
                        writeln!(client.stream, "{}", format!("Room {room} not found").yellow())?;
                        return Ok(CommandResult::Handled);
                    }
                };
                let mut room_guard = lock_room(&room_arc)?;
                room_guard.roles.colors.insert(role_key.to_string(), hex_with_hash.clone());
            }

            let rooms_map = lock_rooms(rooms)?;
            let mut client = lock_client(&client)?;
            
            if let Err(e) = save_rooms_to_disk(&rooms_map) {
                writeln!(client.stream, "{}", format!("Failed to save rooms: {e}").red())?;
                return Ok(CommandResult::Handled);
            }

            writeln!(client.stream, "{} {}", format!("Color for {role_key} role changed to").green(), hex_with_hash.clone().truecolor_from_hex(&hex_with_hash))?;
            Ok(CommandResult::Handled)
        }

        Command::Users => {
            let rooms_map = lock_rooms(rooms)?;
            let room_arc = match rooms_map.get(room) {
                Some(r) => Arc::clone(r),
                None => {
                    let mut client = lock_client(&client)?;
                    writeln!(client.stream, "{}", format!("Room {} not found", room).yellow())?;
                    return Ok(CommandResult::Handled);
                }
            };
            let room_guard = lock_room(&room_arc)?;

            let mut cli = lock_client(&client)?;
            writeln!(cli.stream, "{}", format!("Users in {}:", room).green())?;
            drop(cli);

            let clients_map = lock_clients(clients)?;
            for (uname, udata) in &room_guard.users {
                if !room_guard.online_users.contains(uname) || udata.hidden {
                    continue;
                }

                let role = {
                    let mut ch = udata.role.chars();
                    match ch.next() {
                        Some(f) => f.to_uppercase().collect::<String>() + ch.as_str(),
                        None => String::new(),
                    }
                };

                let nickname = if udata.nick.is_empty() {
                    "None".to_string()
                } else {
                    udata.nick.italic().to_string()
                };

                let color_display = if udata.color.is_empty() {
                    "Default".to_string()
                } else {
                    format!("{}", udata.color).truecolor_from_hex(&udata.color).to_string()
                };

                let afk_status = {
                    let mut afk = false;
                    for c_arc in clients_map.values() {
                        if let Ok(c) = c_arc.lock() {
                            if let ClientState::InRoom { username: u, room: rnm, is_afk, .. } = &c.state {
                                if u == uname && rnm == room {
                                    afk = *is_afk;
                                    break;
                                }
                            }
                        }
                    }
                    if afk { "True".yellow().to_string() } else { "False".green().to_string() }
                };

                let mut client = lock_client(&client)?;
                writeln!(client.stream, "> {} - Role: {}, Nickname: {}, Color: {}, AFK: {}",
                    uname.cyan(),
                    role,
                    nickname,
                    color_display,
                    afk_status
                )?;
            }

            Ok(CommandResult::Handled)
        }

        Command::UsersRename { name } => {
            let rooms_map = lock_rooms(rooms)?;
            let room_arc = match rooms_map.get(room) {
                Some(r) => Arc::clone(r),
                None => {
                    let mut client = lock_client(&client)?;
                    writeln!(client.stream, "{}", format!("Room {room} not found").yellow())?;
                    return Ok(CommandResult::Handled);
                }
            };

            {
                let mut room_guard = lock_room(&room_arc)?;
                match room_guard.users.get_mut(username) {
                    Some(u) => {
                        if name == "*" {
                            u.nick.clear();
                        } else {
                            u.nick = name.clone();
                        }
                    }
                    None => {
                        let mut client = lock_client(&client)?;
                        writeln!(client.stream, "{}", "Error: user record missing".red())?;
                        return Ok(CommandResult::Handled);
                    }
                }
            }

            if let Err(e) = save_rooms_to_disk(&rooms_map) {
                let mut client = lock_client(&client)?;
                writeln!(client.stream, "{}", format!("Failed to save rooms: {e}").red())?;
                return Ok(CommandResult::Handled);
            }

            let mut client = lock_client(&client)?;
            if name == "*" {
                writeln!(client.stream, "{}", "Nickname cleared".green())?;
            } else {
                writeln!(client.stream, "{}", format!("Nickname set to '{}'", name.italic()).green())?;
            }

            Ok(CommandResult::Handled)
        }
        
        Command::UsersRecolor { color } => {
            let rooms_map = lock_rooms(rooms)?;
            let room_arc = match rooms_map.get(room) {
                Some(r) => Arc::clone(r),
                None => {
                    let mut client = lock_client(&client)?;
                    writeln!(client.stream, "{}", format!("Room {room} not found").yellow())?;
                    return Ok(CommandResult::Handled);
                }
            };

            let formatted_color = if color == "*" {
                String::new()
            } else {
                let c = color.trim_start_matches('#').to_uppercase();

                if c.len() != 6 || !c.chars().all(|ch| ch.is_ascii_hexdigit()) {
                    let mut client = lock_client(&client)?;
                    writeln!(client.stream, "{}", "Error: Color must be * or a 6-digit hex value (0-9, A-F)".yellow())?;
                    return Ok(CommandResult::Handled);
                }
                
                format!("#{c}")
            };

            {
                let mut room_guard = lock_room(&room_arc)?;
                match room_guard.users.get_mut(username) {
                    Some(u) => {
                        u.color = formatted_color.clone();
                    }
                    None => {
                        let mut client = lock_client(&client)?;
                        writeln!(client.stream, "{}", "Error: user record missing".red())?;
                        return Ok(CommandResult::Handled);
                    }
                }
            }

            if let Err(e) = save_rooms_to_disk(&rooms_map) {
                let mut client = lock_client(&client)?;
                writeln!(client.stream, "{}", format!("Failed to save rooms: {e}").red())?;
                return Ok(CommandResult::Handled);
            }

            let mut client = lock_client(&client)?;
            if formatted_color.is_empty() {
                writeln!(client.stream, "{}", "Color cleared".green())?;
            } else {
                writeln!(client.stream, "{} {}", "Color set to".green(), formatted_color.clone().truecolor_from_hex(&formatted_color))?;
            }

            Ok(CommandResult::Handled)
        }
        
        Command::UsersHide => {
            let rooms_map = lock_rooms(rooms)?;
            let room_arc = match rooms_map.get(room) {
                Some(r) => Arc::clone(r),
                None => {
                    let mut client = lock_client(&client)?;
                    writeln!(client.stream, "{}", format!("Room {room} not found").yellow())?;
                    return Ok(CommandResult::Handled);
                }
            };

            let hidden_now = {
                let mut room_guard = lock_room(&room_arc)?;
                match room_guard.users.get_mut(username) {
                    Some(u) => {
                        u.hidden = !u.hidden;
                        u.hidden
                    }
                    None => {
                        let mut client = lock_client(&client)?;
                        writeln!(client.stream, "{}", "Error: user record missing".red())?;
                        return Ok(CommandResult::Handled);
                    }
                }
            };

            if let Err(e) = save_rooms_to_disk(&rooms_map) {
                let mut client = lock_client(&client)?;
                writeln!(client.stream, "{}", format!("Failed to save rooms: {e}").red())?;
                return Ok(CommandResult::Handled);
            }

            let mut client = lock_client(&client)?;
            if hidden_now {
                writeln!(client.stream, "{}", "You are now hidden".green())?;
            } else {
                writeln!(client.stream, "{}", "You are now unhidden".green())?;
            }

            Ok(CommandResult::Handled)
        }

        Command::ModInfo => {
            let rooms_map = lock_rooms(rooms)?;
            let room_arc = match rooms_map.get(room) {
                Some(r) => Arc::clone(r),
                None => {
                    let mut client = lock_client(&client)?;
                    writeln!(client.stream, "{}", "Room not found".yellow())?;
                    return Ok(CommandResult::Handled);
                }
            };

            let mut changed = false;
            let (banned, muted) = {
                let mut room_guard = lock_room(&room_arc)?;
                let now = match SystemTime::now().duration_since(UNIX_EPOCH) {
                    Ok(d) => d.as_secs(),
                    Err(_) => 0,
                };

                let mut banned_vec = Vec::<String>::new();
                let mut muted_vec  = Vec::<String>::new();

                for (uname, rec) in room_guard.users.iter_mut() {
                    // Ban handling
                    if rec.banned {
                        let still_banned = if rec.ban_length == 0 {
                            true
                        } else {
                            let expire = rec.ban_stamp.saturating_add(rec.ban_length);
                            if now >= expire {
                                // Ban expired, clear flags
                                rec.banned = false;
                                rec.ban_stamp = 0;
                                rec.ban_length = 0;
                                rec.ban_reason.clear();
                                changed = true;
                                false
                            } else {
                                true
                            }
                        };

                        if still_banned {
                            let remaining = if rec.ban_length == 0 {
                                "Permanent".to_string()
                            } else {
                                let rem = rec.ban_stamp.saturating_add(rec.ban_length) - now;
                                let d = rem / 86_400;
                                let h = (rem % 86_400) / 3_600;
                                let m = (rem % 3_600) / 60;
                                let s = rem % 60;
                                format!("{d}d {h}h {m}m {s}s left")
                            };
                            let reason = if rec.ban_reason.is_empty() { "" } else { " - " };
                            banned_vec.push(format!("{uname} ({remaining}){reason}{}", rec.ban_reason));
                        }
                    }

                    // Mute handling
                    if rec.muted {
                        let still_muted = if rec.mute_length == 0 {
                            true
                        } else {
                            let expire = rec.mute_stamp.saturating_add(rec.mute_length);
                            if now >= expire {
                                rec.muted = false;
                                rec.mute_stamp = 0;
                                rec.mute_length = 0;
                                rec.mute_reason.clear();
                                changed = true;
                                false
                            } else {
                                true
                            }
                        };

                        if still_muted {
                            let remaining = if rec.mute_length == 0 {
                                "Permanent".to_string()
                            } else {
                                let rem = rec.mute_stamp.saturating_add(rec.mute_length) - now;
                                let d = rem / 86_400;
                                let h = (rem % 86_400) / 3_600;
                                let m = (rem % 3_600) / 60;
                                let s = rem % 60;
                                format!("{d}d {h}h {m}m {s}s left")
                            };
                            let reason = if rec.mute_reason.is_empty() { "" } else { " - " };
                            muted_vec.push(format!("{uname} ({remaining}){reason}{}", rec.mute_reason));
                        }
                    }
                }
                (banned_vec, muted_vec)
            };

            if changed {
                if let Err(e) = save_rooms_to_disk(&rooms_map) {
                    let mut client = lock_client(&client)?;
                    writeln!(client.stream, "{}", format!("Failed to save rooms: {e}").red())?;
                    return Ok(CommandResult::Handled);
                }
            }

            let mut client = lock_client(&client)?;
            if banned.is_empty() && muted.is_empty() {
                writeln!(client.stream, "{}", "No users are currently banned or muted".green())?;
            } else {
                if !banned.is_empty() {
                    writeln!(client.stream, "{}", "- Banned users -".green())?;
                    for line in &banned {
                        writeln!(client.stream, "  > {line}")?;
                    }
                }
                if !muted.is_empty() {
                    writeln!(client.stream, "{}", "- Muted users -".green())?;
                    for line in &muted {
                        writeln!(client.stream, "  > {line}")?;
                    }
                }
            }

            Ok(CommandResult::Handled)
        }

        Command::ModKick { username, reason } => {
            let room_arc = {
                let rooms_map = lock_rooms(rooms)?;
                match rooms_map.get(room) {
                    Some(r) => Arc::clone(r),
                    None => {
                        let mut client = lock_client(&client)?;
                        writeln!(client.stream, "{}", "Room not found".yellow())?;
                        return Ok(CommandResult::Handled);
                    }
                }
            };

            {
                let mut rg = lock_room(&room_arc)?;
                if !rg.online_users.contains(&username) {
                    let mut client = lock_client(&client)?;
                    writeln!(client.stream, "{}", format!("{username} is not currently online").yellow())?;
                    return Ok(CommandResult::Handled);
                }
                rg.online_users.retain(|u| u != &username);
            }

            if let Err(e) = unix_timestamp(rooms, room, &username) {
                eprintln!("Failed to update last_seen for {username} in {room}: {e}");
            }

            let clients_map = lock_clients(clients)?;
            let mut kicked = false;
            for c_arc in clients_map.values() {
                let mut c = match c_arc.lock() {
                    Ok(g) => g,
                    Err(p) => {
                        eprintln!("Poisoned client lock: {p}");
                        continue;
                    }
                };

                if let ClientState::InRoom { username: u, room: rnm, .. } = &c.state {
                    if u == &username && rnm == room {
                        let msg = if reason.trim().is_empty() {
                            format!("You have been kicked from {room}")
                        } else {
                            format!("You have been kicked from {room}: {reason}")
                        };
                        writeln!(c.stream, "{}", msg.red())?;
                        c.state = ClientState::LoggedIn { username: username.clone() };
                        kicked = true;
                        break;
                    }
                }
            }
            drop(clients_map);

            {
                let rooms_map = lock_rooms(rooms)?;
                if let Err(e) = save_rooms_to_disk(&rooms_map) {
                    let mut client = lock_client(&client)?;
                    writeln!(client.stream, "{}", format!("Failed to save rooms: {e}").red())?;
                    return Ok(CommandResult::Handled);
                }
            }

            let mut client = lock_client(&client)?;
            if kicked {
                if reason.trim().is_empty() {
                    writeln!(client.stream, "{}", format!("Kicked {username}").green())?;
                } else {
                    writeln!(client.stream, "{}", format!("Kicked {username}: {reason}").green())?;
                }
            } else {
                writeln!(client.stream, "{}", format!("Failed to kick {username}").yellow())?;
            }

            Ok(CommandResult::Handled)
        }

        Command::ModBan { username, duration, reason } => {
            let room_arc = {
                let rooms_map = lock_rooms(rooms)?;
                match rooms_map.get(room) {
                    Some(r) => Arc::clone(r),
                    None => {
                        let mut client = lock_client(&client)?;
                        writeln!(client.stream, "{}", "Room not found".yellow())?;
                        return Ok(CommandResult::Handled);
                    }
                }
            };

            let ban_secs = match parse_duration(&duration) {
                Ok(v) => v,
                Err(e) => {
                    let mut client = lock_client(&client)?;
                    writeln!(client.stream, "{}", format!("Bad duration: {e}").yellow())?;
                    return Ok(CommandResult::Handled);
                }
            };

            let now = match SystemTime::now().duration_since(UNIX_EPOCH) {
                Ok(d) => d.as_secs(),
                Err(_) => 0,
            };

            // Apply/create ban record
            {
                let mut rg = lock_room(&room_arc)?;

                // Add entry if not present
                let user_rec = rg.users.entry(username.clone()).or_insert(RoomUser {
                    nick: "".to_string(),
                    color: "".to_string(),
                    role: "user".to_string(),
                    hidden: false,
                    last_seen: now,
                    banned: false,
                    ban_stamp: 0,
                    ban_length: 0,
                    ban_reason: "".to_string(),
                    muted: false,
                    mute_stamp: 0,
                    mute_length: 0,
                    mute_reason: "".to_string(),
                });

                user_rec.banned      = true;
                user_rec.ban_stamp   = now;
                user_rec.ban_length  = ban_secs;
                user_rec.ban_reason  = reason.clone();
                user_rec.last_seen   = now;

                // Remove from online list if present
                rg.online_users.retain(|u| u != &username);
            }

            // Refresh last_seen helper
            let _ = unix_timestamp(rooms, room, &username);

            // Kick client if online
            let clients_map = lock_clients(clients)?;
            let human_len = if ban_secs == 0 {
                "PERMANENT".to_string()
            } else {
                let mut rem = ban_secs;
                let d = rem / 86_400;
                rem %= 86_400;
                let h = rem / 3_600;
                rem %= 3_600;
                let m = rem / 60;
                let s = rem % 60;
                format!("{}d {}h {}m {}s", d, h, m, s)
            };

            for c_arc in clients_map.values() {
                let mut c = match c_arc.lock() {
                    Ok(g) => g,
                    Err(_) => continue,
                };
                if let ClientState::InRoom { username: u, room: rnm, .. } = &c.state {
                    if u == &username && rnm == room {
                        let msg = if reason.trim().is_empty() {
                            format!("You have been banned from {room} ({human_len})")
                        } else {
                            format!("You have been banned from {room} ({reason})\n> {human_len}")
                        };
                        writeln!(c.stream, "{}", msg.red())?;
                        c.state = ClientState::LoggedIn { username: username.clone() };
                        break;
                    }
                }
            }
            drop(clients_map);

            {
                let rooms_map = lock_rooms(rooms)?;
                if let Err(e) = save_rooms_to_disk(&rooms_map) {
                    let mut client = lock_client(&client)?;
                    writeln!(client.stream, "{}", format!("Failed to save rooms: {e}").red())?;
                    return Ok(CommandResult::Handled);
                }
            }

            let mut client = lock_client(&client)?;
            let len_disp = if ban_secs == 0 {
                "PERMANENT".to_string()
            } else {
                format!("{human_len}")
            };

            if reason.trim().is_empty() {
                writeln!(client.stream, "{}", format!("Banned {username} ({len_disp})").green())?;
            } else {
                writeln!(client.stream, "{}", format!("Banned {username} ({len_disp}): {reason}").green())?;
            }

            Ok(CommandResult::Handled)
        }

        Command::ModUnban { username } => {
            let room_arc = {
                let rooms_map = lock_rooms(rooms)?;
                match rooms_map.get(room) {
                    Some(r) => Arc::clone(r),
                    None => {
                        let mut client = lock_client(&client)?;
                        writeln!(client.stream, "{}", "Room not found".yellow())?;
                        return Ok(CommandResult::Handled);
                    }
                }
            };

            let mut actually_unbanned = false;
            {
                let mut rg = lock_room(&room_arc)?;

                if let Some(rec) = rg.users.get_mut(&username) {
                    if rec.banned {
                        rec.banned = false;
                        rec.ban_stamp = 0;
                        rec.ban_length = 0;
                        rec.ban_reason.clear();
                        actually_unbanned = true;
                    }
                }
            }

            if !actually_unbanned {
                let mut client = lock_client(&client)?;
                writeln!(client.stream, "{}", format!("{username} is not currently banned").yellow())?;
                return Ok(CommandResult::Handled);
            }

            {
                let rooms_map = lock_rooms(rooms)?;
                if let Err(e) = save_rooms_to_disk(&rooms_map) {
                    let mut client = lock_client(&client)?;
                    writeln!(client.stream, "{}", format!("Failed to save rooms: {e}").red())?;
                    return Ok(CommandResult::Handled);
                }
            }

            let mut client = lock_client(&client)?;
            writeln!(client.stream, "{}", format!("Unbanned {username}").green())?;
            Ok(CommandResult::Handled)
        }

        Command::ModMute { username, duration, reason } => {
            let room_arc = {
                let rooms_map = lock_rooms(rooms)?;
                match rooms_map.get(room) {
                    Some(r) => Arc::clone(r),
                    None => {
                        let mut client = lock_client(&client)?;
                        writeln!(client.stream, "{}", "Room not found".yellow())?;
                        return Ok(CommandResult::Handled);
                    }
                }
            };

            let mute_secs = match parse_duration(&duration) {
                Ok(v) => v,
                Err(e) => {
                    let mut client = lock_client(&client)?;
                    writeln!(client.stream, "{}", format!("Bad duration: {e}").yellow())?;
                    return Ok(CommandResult::Handled);
                }
            };

            let now = match SystemTime::now().duration_since(UNIX_EPOCH) {
                Ok(d) => d.as_secs(),
                Err(_) => 0,
            };

            // Apply/create mute record
            {
                let mut rg = lock_room(&room_arc)?;
                let rec = rg.users.entry(username.clone()).or_insert(RoomUser {
                    nick: "".into(),
                    color: "".into(),
                    role: "user".into(),
                    hidden: false,
                    last_seen: now,
                    banned: false,
                    ban_stamp: 0,
                    ban_length: 0,
                    ban_reason: "".into(),
                    muted: false,
                    mute_stamp: 0,
                    mute_length: 0,
                    mute_reason: "".into(),
                });

                rec.muted       = true;
                rec.mute_stamp  = now;
                rec.mute_length = mute_secs;
                rec.mute_reason = reason.clone();
                rec.last_seen   = now;
            }

            // Refresh last_seen helper
            let _ = unix_timestamp(rooms, room, &username);

            // Notify live client
            let clients_map = lock_clients(clients)?;
            let human_len = if mute_secs == 0 {
                "PERMANENT".to_string()
            } else {
                let mut rem = mute_secs;
                let d = rem / 86_400;
                rem %= 86_400;
                let h = rem / 3_600;
                rem %= 3_600;
                let m = rem / 60;
                let s = rem % 60;
                format!("{d}d {h}h {m}m {s}s")
            };

            for c_arc in clients_map.values() {
                let mut c = match c_arc.lock() {
                    Ok(g) => g,
                    Err(_) => continue,
                };
                if let ClientState::InRoom { username: u, room: rnm, .. } = &c.state {
                    if u == &username && rnm == room {
                        let msg = if reason.trim().is_empty() {
                            format!("You have been muted in {room} ({human_len})")
                        } else {
                            format!("You have been muted in {room}: {reason}\n> {human_len}")
                        };
                        writeln!(c.stream, "{}", msg.red())?;
                        break;
                    }
                }
            }
            drop(clients_map);

            {
                let rooms_map = lock_rooms(rooms)?;
                if let Err(e) = save_rooms_to_disk(&rooms_map) {
                    let mut client = lock_client(&client)?;
                    writeln!(client.stream, "{}", format!("Failed to save rooms: {e}").red())?;
                    return Ok(CommandResult::Handled);
                }
            }

            let mut client = lock_client(&client)?;
            let len_disp = if mute_secs == 0 {
                "PERMANENT".into()
            } else {
                format!("{human_len}")
            };

            if reason.trim().is_empty() {
                writeln!(client.stream, "{}", format!("Muted {username} ({len_disp})").green())?;
            } else {
                writeln!(client.stream, "{}", format!("Muted {username} ({len_disp}): {reason}").green())?;
            }

            Ok(CommandResult::Handled)
        }

        Command::ModUnmute { username } => {
            let room_arc = {
                let rooms_map = lock_rooms(rooms)?;
                match rooms_map.get(room) {
                    Some(r) => Arc::clone(r),
                    None => {
                        let mut client = lock_client(&client)?;
                        writeln!(client.stream, "{}", "Room not found".yellow())?;
                        return Ok(CommandResult::Handled);
                    }
                }
            };

            let mut unmuted = false;
            {
                let mut rg = lock_room(&room_arc)?;
                if let Some(rec) = rg.users.get_mut(&username) {
                    if rec.muted {
                        rec.muted = false;
                        rec.mute_stamp = 0;
                        rec.mute_length = 0;
                        rec.mute_reason.clear();
                        unmuted = true;
                    }
                }
            }

            if !unmuted {
                let mut client = lock_client(&client)?;
                writeln!(client.stream, "{}", format!("{username} is not currently muted").yellow())?;
                return Ok(CommandResult::Handled);
            }

            {
                let rooms_map = lock_rooms(rooms)?;
                if let Err(e) = save_rooms_to_disk(&rooms_map) {
                    let mut client = lock_client(&client)?;
                    writeln!(client.stream, "{}", format!("Failed to save rooms: {e}").red())?;
                    return Ok(CommandResult::Handled);
                }
            }

            let clients_map = lock_clients(clients)?;
            for c_arc in clients_map.values() {
                let mut c = match c_arc.lock() {
                    Ok(g) => g,
                    Err(_) => continue,
                };
                if let ClientState::InRoom { username: u, room: rnm, .. } = &c.state {
                    if u == &username && rnm == room {
                        writeln!(c.stream, "{}", "You have been unmuted".green())?;
                        break;
                    }
                }
            }
            drop(clients_map);

            let mut client = lock_client(&client)?;
            writeln!(client.stream, "{}", format!("Unmuted {username}").green())?;
            Ok(CommandResult::Handled)
        }

        Command::Account | Command::AccountLogout | Command::AccountEditUsername { .. } | Command::AccountEditPassword { .. } | Command::AccountImport { .. } | Command::AccountExport { .. } | Command::AccountDelete { .. } |
        Command::RoomList | Command::RoomCreate { .. } | Command::RoomJoin { .. } | Command::RoomImport { .. } | Command::RoomDelete { .. } => {
            let mut client = lock_client(&client)?;
            writeln!(client.stream, "{}", "Must be in the lobby to perform this command".yellow())?;
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
