use std::io::{self, Write};
use std::sync::{Arc, Mutex};
use colored::*;

use crate::commands::parser::Command;
use crate::commands::command_utils::{help_msg_inroom, ColorizeExt, has_permission};
use crate::state::types::{Client, Clients, ClientState, Rooms};
use crate::utils::{lock_client, lock_clients, lock_room, lock_rooms};
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

        Command::Account | Command::AccountLogout | Command::AccountEditUsername { .. } | Command::AccountEditPassword { .. } | Command::AccountImport { .. } | Command::AccountExport { .. } | Command::AccountDelete { .. } |
        Command::RoomList | Command::RoomCreate { .. } | Command::RoomJoin { .. } | Command::RoomImport { .. } | Command::RoomDelete { .. } |
        Command::AFK | Command::Send { .. } | Command::Me { .. } | Command::IgnoreList | Command::IgnoreAdd { .. } | Command::IgnoreRemove { .. } |
        Command::SuperRename { .. } | Command::SuperExport { .. } | Command::SuperWhitelist | Command::SuperWhitelistToggle | Command::SuperWhitelistAdd { .. } | Command::SuperWhitelistRemove { .. } | Command::SuperLimitRate { .. } | Command::SuperLimitSession { .. } | Command::SuperRoles | Command::SuperRolesPerms | Command::SuperRolesAdd { .. } | Command::SuperRolesRevoke { .. } | Command::SuperRolesAssign { .. } | Command::SuperRolesRecolor { .. } |
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
