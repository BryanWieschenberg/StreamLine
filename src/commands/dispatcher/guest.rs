use std::io::{self, BufReader, Write};
use std::fs::{File, OpenOptions};
use serde::Serialize;
use serde_json::{Value, json, Serializer};
use serde_json::ser::PrettyFormatter;
use std::sync::{Arc, Mutex};
use colored::*;

use crate::commands::parser::Command;
use crate::commands::command_utils::{help_msg_guest, generate_hash, is_user_logged_in};
use crate::state::types::{Client, Clients, ClientState, Rooms};
use crate::utils::{lock_client, lock_clients, lock_users_storage};
use super::CommandResult;

pub fn guest_command(cmd: Command, client: Arc<Mutex<Client>>, clients: &Clients, _rooms: &Rooms) -> io::Result<CommandResult> {
    match cmd {
        Command::Help => {
            let mut client = lock_client(&client)?;
            writeln!(client.stream, "{}{}", help_msg_guest().bright_blue(), "\x1b[0m")?;
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
                let mut clients = lock_clients(&clients)?;
                clients.remove(&addr);
            }

            let mut client = lock_client(&client)?;
            writeln!(client.stream, "{}", "Exiting...".green())?;
            client.stream.shutdown(std::net::Shutdown::Both)?;     
            Ok(CommandResult::Stop)
        }

        Command::Leave | Command::Status | Command::DM { .. } | Command::AFK | Command::Send { .. } | Command::Me { .. } | Command::IgnoreList | Command::IgnoreAdd { .. } | Command::IgnoreRemove { .. } |
        Command::SuperUsers | Command::SuperRename { .. } | Command::SuperExport { .. } | Command::SuperWhitelist | Command::SuperWhitelistToggle | Command::SuperWhitelistAdd { .. } | Command::SuperWhitelistRemove { .. } | Command::SuperLimitRate { .. } | Command::SuperLimitSession { .. } | Command::SuperRoles | Command::SuperRolesAdd { .. } | Command::SuperRolesRevoke { .. } | Command::SuperRolesAssign { .. } | Command::SuperRolesRecolor { .. } |
        Command::Users | Command::UsersRename { .. } | Command::UsersRecolor { .. } | Command::UsersHide |
        Command::ModKick { .. } | Command::ModMute { .. } | Command::ModUnmute { .. } | Command::ModBan { .. } | Command::ModUnban { .. } => {
            let mut client = lock_client(&client)?;
            writeln!(client.stream, "{}", "Must be in a room to perform this command".yellow())?;
            Ok(CommandResult::Handled)
        }

        Command::AccountRegister {username, password, confirm} => {
            if password != confirm {
                let mut client = lock_client(&client)?;
                writeln!(client.stream, "{}", "Error: Passwords don't match".yellow())?;
                return Ok(CommandResult::Handled)
            }

            let _lock = lock_users_storage()?;

            let file = File::open("data/users.json")?;
            let reader = BufReader::new(file);
            let mut users: Value = serde_json::from_reader(reader)?;
            
            if users.get(&username).is_some() {
                let mut client = lock_client(&client)?;
                writeln!(client.stream, "{}", "Error: Name is already taken".yellow())?;
                return Ok(CommandResult::Handled);
            }

            let password_hash = generate_hash(&password);

            users[&username] = json!({
                "password": password_hash,
                "ignore": []
            });

            let file = OpenOptions::new()
                .write(true)
                .truncate(true)
                .open("data/users.json")?;

            let mut writer = std::io::BufWriter::new(file);
            let formatter = PrettyFormatter::with_indent(b"    ");
            let mut ser = Serializer::with_formatter(&mut writer, formatter);
            users.serialize(&mut ser)?;

            let mut client = lock_client(&client)?;
            client.state = ClientState::LoggedIn { username: username.clone() };
            writeln!(client.stream, "{}", format!("User Registered: {}", username).green())?;

            Ok(CommandResult::Handled)
        }

        Command::AccountLogin {username, password} => {
            if is_user_logged_in(clients, &username) {
                let mut client = lock_client(&client)?;
                writeln!(client.stream, "{}", format!("Error: {} is already logged in", username).yellow())?;
                return Ok(CommandResult::Handled);
            }

            let _lock = lock_users_storage()?;

            let file = File::open("data/users.json")?;
            let reader = BufReader::new(file);
            let users: Value = serde_json::from_reader(reader)?;

            match users.get(&username) {
                Some(user_obj) => {
                    let stored_hash = match user_obj.get("password").and_then(|v| v.as_str()) {
                        Some(hash) => hash,
                        None => {
                            let mut client = lock_client(&client)?;
                            writeln!(client.stream, "{}", "Error: Malformed user data".yellow())?;
                            return Ok(CommandResult::Handled);
                        }
                    };
                    if generate_hash(&password) == stored_hash {
                        let mut client = lock_client(&client)?;
                        client.state = ClientState::LoggedIn { username: username.clone() };
                        client.ignore_list = user_obj.get("ignore")
                            .and_then(|v| v.as_array())
                            .map_or_else(Vec::new, |arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect());
                        writeln!(client.stream, "{}", format!("Logged in as: {}", username).green())?;
                    } else {
                        let mut client = lock_client(&client)?;
                        writeln!(client.stream, "{}", "Error: Incorrect password".yellow())?;
                    }
                }
                None => {
                    let mut client = lock_client(&client)?;
                    writeln!(client.stream, "{}", "Error: Username not found".yellow())?;
                }
            }

            Ok(CommandResult::Handled)
        }

        Command::AccountLogout | Command::AccountEditUsername { .. } | Command::AccountEditPassword { .. } => {
            let mut client = lock_client(&client)?;
            writeln!(client.stream, "{}", "You are not currently logged in".yellow())?;
            Ok(CommandResult::Handled)
        }

        Command::AccountImport { filename } => {
            let safe_filename = if !filename.ends_with(".json") {
                format!("{}.json", filename)
            }
            else {
                filename
            };

            let import_path = format!("data/logs/users/{}", safe_filename);
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

        Command::AccountExport { .. } => {
            let mut client = lock_client(&client)?;
            writeln!(client.stream, "{}", "Currently a guest, please register or log into an account to export account data".green())?;
            Ok(CommandResult::Handled)
        }

        Command::AccountDelete { .. } => {
            let mut client = lock_client(&client)?;
            writeln!(client.stream, "{}", "Currently a guest, cannot delete an account".green())?;
            Ok(CommandResult::Handled)
        }

        Command::Account => {
            let mut client = lock_client(&client)?;
            writeln!(client.stream, "{}", "Must register or log into an account to join a room".green())?;
            Ok(CommandResult::Handled)
        }

        Command::RoomList | Command::RoomCreate { .. } | Command::RoomJoin { .. } | Command::RoomImport { .. } | Command::RoomDelete { .. } => {
            let mut client = lock_client(&client)?;
            writeln!(client.stream, "{}", "Must log in to perform this command".yellow())?;
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
