use std::io::{self, Write};
use std::sync::{Arc, Mutex};
use colored::*;

use crate::commands::parser::Command;
use crate::commands::command_utils::help_msg_inroom;
use crate::state::types::{Client, Clients, ClientState, Rooms};
use crate::utils::{lock_client, lock_clients, lock_room, lock_rooms};
use super::CommandResult;

pub fn inroom_command(cmd: Command, client: Arc<Mutex<Client>>, clients: &Clients, rooms: &Rooms, username: &String, room: &String) -> io::Result<CommandResult> {
    match cmd {
        Command::Help => {
            let mut client = lock_client(&client)?;
            writeln!(client.stream, "{}{}", help_msg_inroom().green(), "\x1b[0m")?;
            io::stdout().flush()?;
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
                    writeln!(client.stream, "{}", format!("Room '{}' not found", room).yellow())?;
                    return Ok(CommandResult::Handled);
                }
            };

            let room = lock_room(&room_arc)?;
            let user_info = match room.users.get(username) {
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
            let mins = duration.as_secs() / 60;
            let secs = duration.as_secs() % 60;

            writeln!(client.stream, "{}", format!(
                "Status for '{}':\nColor: {}\nRole: {}\nTime in room: {}m {}s",
                username,
                if user_info.color.is_empty() { "#FFFFFF" } else { &user_info.color },
                &user_info.role,
                mins, secs
            ).green())?;
            Ok(CommandResult::Handled)
        }

        Command::AccountRegister { .. } => {
            let mut client = lock_client(&client)?;
            writeln!(client.stream, "{}", "Already logged in".yellow())?;
            Ok(CommandResult::Handled)
        }

        Command::AccountLogin { .. } => {
            let mut client = lock_client(&client)?;
            writeln!(client.stream, "{}", "Already logged in".yellow())?;
            Ok(CommandResult::Handled)
        }

        Command::AccountLogout => {
            let mut client = lock_client(&client)?;
            writeln!(client.stream, "{}", "Must be in the lobby to log out".yellow())?;
            Ok(CommandResult::Handled)
        }

        Command::AccountEditUsername { .. } => {
            let mut client = lock_client(&client)?;
            writeln!(client.stream, "{}", "Must be in the lobby to edit your account".yellow())?;
            Ok(CommandResult::Handled)
        }

        Command::AccountEditPassword { .. } => {
            let mut client = lock_client(&client)?;
            writeln!(client.stream, "{}", "Must be in the lobby to edit your account".yellow())?;
            Ok(CommandResult::Handled)
        }

        Command::AccountImport { .. } => {
            let mut client = lock_client(&client)?;
            writeln!(client.stream, "{}", "Must be in the lobby to import an account".yellow())?;
            Ok(CommandResult::Handled)
        }

        Command::AccountExport { .. } => {
            let mut client = lock_client(&client)?;
            writeln!(client.stream, "{}", "Must be in the lobby to export your account".yellow())?;
            Ok(CommandResult::Handled)
        }

        Command::AccountDelete { .. } => {
            let mut client = lock_client(&client)?;
            writeln!(client.stream, "{}", "Must be in the lobby to delete your account".green())?;
            Ok(CommandResult::Handled)
        }

        Command::Account => {
            let mut client = lock_client(&client)?;
            writeln!(client.stream, "{}", format!("Currently logged in as: {} (room: {})", username, room).green())?;
            Ok(CommandResult::Handled)
        }

        Command::RoomList => {
            let mut client = lock_client(&client)?;
            writeln!(client.stream, "{}", "Must be in the lobby to view rooms".yellow())?;
            Ok(CommandResult::Handled)
        }

        Command::RoomCreate { .. } => {
            let mut client = lock_client(&client)?;
            writeln!(client.stream, "{}", "Must be in the lobby to create a room".yellow())?;
            Ok(CommandResult::Handled)
        }

        Command::RoomJoin { .. } => {
            let mut client = lock_client(&client)?;
            writeln!(client.stream, "{}", "Must be in the lobby to join a room".yellow())?;
            Ok(CommandResult::Handled)
        }

        Command::RoomImport { .. } => {
            let mut client = lock_client(&client)?;
            writeln!(client.stream, "{}", "Must be in the lobby to import a room".yellow())?;
            Ok(CommandResult::Handled)
        }

        Command::RoomDelete { .. } => {
            let mut client = lock_client(&client)?;
            writeln!(client.stream, "{}", "Must log in to delete a room".yellow())?;
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
