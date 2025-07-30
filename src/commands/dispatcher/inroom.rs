use std::io::{self, Write};
use std::sync::{Arc, Mutex};
use std::collections::{HashMap};
use colored::*;

use crate::commands::parser::Command;
use crate::commands::command_utils::{help_msg_inroom, ColorizeExt, has_permission, save_rooms_to_disk, command_order, RESTRICTED_COMMANDS};
use crate::state::types::{Client, Clients, ClientState, Rooms};
use crate::utils::{lock_client, lock_clients, lock_room, lock_rooms, lock_rooms_storage};
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
                    "afk", "send", "msg", "me", "super", "user", "log", "mod"
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
                let rooms = lock_rooms(rooms)?;
                if let Some(room_arc) = rooms.get(room) {
                    if let Ok(mut room_guard) = room_arc.lock() {
                        room_guard.online_users.retain(|u| u != username);
                    }
                }
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
                "Default".italic().to_string()
            }
            else {
                format!("{}", user_info.color).truecolor_from_hex(&user_info.color).to_string()
            };

            writeln!(client.stream, "{}\n{} {}\n{} {}\n{} {}\n{} {}",
                format!("Status for '{}' in Room '{}':", username, room).green(),
                "> Role:".green(), role,
                "> Nickname:".green(), if user_info.nick.is_empty() { "None".italic().to_string() } else { user_info.nick.clone() },
                "> Color:".green(), color_display,
                "> Session:".green(), format!("{:0>2}:{:0>2}:{:0>2}", hrs, mins, secs)
            )?;
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

            let mut client = lock_client(&client)?;
            writeln!(client.stream, "{}", format!("User data for room '{}':", room).green())?;
            writeln!(client.stream, "{}", "=".repeat(50).green())?;

            for (username, user_data) in &room_guard.users {
                if room_guard.online_users.contains(username) {
                    let role = {
                        let mut c = user_data.role.chars();
                        match c.next() {
                            Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                            None => String::new(),
                        }
                    };
    
                    let color_display = if user_data.color.is_empty() {
                        "Default".italic().to_string()
                    }
                    else {
                        format!("{}", user_data.color).truecolor_from_hex(&user_data.color).to_string()
                    };
    
                    let nickname = if user_data.nick.is_empty() {
                        "None".italic().to_string()
                    }
                    else {
                        user_data.nick.clone()
                    };
    
                    let hidden_status = if user_data.hidden {
                        "Hidden".yellow().to_string()
                    }
                    else {
                        "Visible".green().to_string()
                    };
    
                    let banned_status = if user_data.banned.is_empty() {
                        "Not Banned".green().to_string()
                    }
                    else {
                        format!("Banned ({})", user_data.banned).red().to_string()
                    };
    
                    let muted_status = if user_data.muted.is_empty() {
                        "Not Muted".green().to_string()
                    }
                    else {
                        format!("Muted ({})", user_data.muted).yellow().to_string()
                    };
    
                    writeln!(client.stream, "{}", format!("User: {}", username).cyan())?;
                    writeln!(client.stream, "  > Role: {}", role)?;
                    writeln!(client.stream, "  > Nickname: {}", nickname)?;
                    writeln!(client.stream, "  > Color: {}", color_display)?;
                    writeln!(client.stream, "  > Visibility: {}", hidden_status)?;
                    writeln!(client.stream, "  > Ban Status: {}", banned_status)?;
                    writeln!(client.stream, "  > Mute Status: {}", muted_status)?;                        
                }
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

            if let Ok(_room_save_lock) = crate::state::types::ROOMS_LOCK.lock() {
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

        Command::SuperRoles => {
            let rooms_map = lock_rooms(rooms)?;
            let room_arc = match rooms_map.get(room) {
                Some(r) => Arc::clone(r),
                None => {
                    writeln!(lock_client(&client)?.stream, "{}", format!("Room {room} not found").yellow())?;
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
                    writeln!(client.stream, "{}", "Error: role must be user|mod".yellow())?;
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
                        writeln!(lock_client(&client)?.stream, "{}", format!("Room {room} not found").yellow())?;
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
                    writeln!(lock_client(&client)?.stream, "{}", format!("Failed to save rooms: {e}").red())?;
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
                    writeln!(client.stream, "{}", "Error: role must be user|mod".yellow())?;
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
                        writeln!(lock_client(&client)?.stream, "{}", format!("Room {room} not found").yellow())?;
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
                    writeln!(lock_client(&client)?.stream, "{}", format!("Failed to save rooms: {}", e).red())?;
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
            Ok(CommandResult::Handled)
        }
        Command::SuperRolesRecolor { role, color } => {
            Ok(CommandResult::Handled)
        }

        Command::Account | Command::AccountLogout | Command::AccountEditUsername { .. } | Command::AccountEditPassword { .. } | Command::AccountImport { .. } | Command::AccountExport { .. } | Command::AccountDelete { .. } |
        Command::RoomList | Command::RoomCreate { .. } | Command::RoomJoin { .. } | Command::RoomImport { .. } | Command::RoomDelete { .. } |
        Command::AFK | Command::Send { .. } | Command::Me { .. } | Command::IgnoreList | Command::IgnoreAdd { .. } | Command::IgnoreRemove { .. } |
        Command::SuperExport { .. } | Command::SuperLimitRate { .. } | Command::SuperLimitSession { .. } |
        Command::Users | Command::UsersRename { .. } | Command::UsersRecolor { .. } | Command::UsersHide |
        Command::ModKick { .. } | Command::ModMute { .. } | Command::ModUnmute { .. } | Command::ModBan { .. } | Command::ModUnban { .. } => {
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
