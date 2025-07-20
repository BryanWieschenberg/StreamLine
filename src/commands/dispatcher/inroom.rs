use std::io::{self, Write};
use std::sync::{Arc, Mutex};
use colored::*;

use crate::commands::parser::Command;
use crate::commands::command_utils::help_msg_inroom;
use crate::state::types::{Client, Clients, Rooms};
use crate::utils::{lock_client, lock_clients, lock_rooms};
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
