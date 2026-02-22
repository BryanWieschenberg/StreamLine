use std::io::{self, BufReader, Write};
use std::time::Instant;
use std::fs::File;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use colored::*;

use crate::backend::parser::Command;
use crate::backend::command_utils::{help_msg_guest, generate_hash, is_user_logged_in};
use crate::shared::types::{Client, ClientState, Clients, Rooms};
use crate::shared::utils::{lock_client, lock_clients, lock_users_storage, load_json, save_json, send_message, send_error, send_success};
use super::CommandResult;

pub fn guest_command(cmd: Command, client: Arc<Mutex<Client>>, clients: &Clients, _rooms: &Rooms) -> io::Result<CommandResult> {
    match cmd {
        Command::Help => {
            send_message(&client, &format!("{}{}", help_msg_guest().bright_blue(), "\x1b[0m"))?;
            Ok(CommandResult::Handled)
        }

        Command::Ping { start_time } => {
            if let Some(start_ms) = start_time {
                send_message(&client, &format!("/PONG {start_ms}"))?;
            }
            Ok(CommandResult::Handled)
        }

        Command::PubKey { .. } => {
            send_message(&client, &"Public keys are handled automatically when logging in".yellow().to_string())?;
            Ok(CommandResult::Handled)
        }

        Command::Quit => {
            let addr = {
                let c = lock_client(&client)?;
                c.addr
            };

            {
                let mut clients = lock_clients(clients)?;
                clients.remove(&addr);
            }

            let mut client = lock_client(&client)?;
            writeln!(client.stream, "{}", "Exiting...".green())?;
            client.stream.shutdown(std::net::Shutdown::Both)?;     
            Ok(CommandResult::Stop)
        }

        Command::Leave | Command::Status | Command::AFK | Command::Announce { .. } | Command::Seen { .. } | Command::DM { .. } | Command::Me { .. } | Command::IgnoreList | Command::IgnoreAdd { .. } | Command::IgnoreRemove { .. } |
        Command::SuperUsers | Command::SuperRename { .. } | Command::SuperExport { .. } | Command::SuperWhitelist | Command::SuperWhitelistToggle | Command::SuperWhitelistAdd { .. } | Command::SuperWhitelistRemove { .. } | Command::SuperLimit | Command::SuperLimitRate { .. } | Command::SuperLimitSession { .. } | Command::SuperRoles | Command::SuperRolesAdd { .. } | Command::SuperRolesRevoke { .. } | Command::SuperRolesAssign { .. } | Command::SuperRolesRecolor { .. } |
        Command::Users | Command::UsersRename { .. } | Command::UsersRecolor { .. } | Command::UsersHide |
        Command::ModInfo | Command::ModKick { .. } | Command::ModMute { .. } | Command::ModUnmute { .. } | Command::ModBan { .. } | Command::ModUnban { .. } => {
            send_message(&client, &"Must be in a room to perform this command".yellow().to_string())?;
            Ok(CommandResult::Handled)
        }

        Command::AccountRegister {username, password, confirm} => {
            {
                let mut c = lock_client(&client)?;
                let now = Instant::now();
                c.login_attempts.retain(|t| now.duration_since(*t).as_secs() < 60);
                if c.login_attempts.len() >= 5 {
                    writeln!(c.stream, "{}", "Too many attempts, try again later".yellow())?;
                    return Ok(CommandResult::Handled);
                }
                c.login_attempts.push_back(now);
            }

            if password != confirm {
                send_message(&client, &"Error: Passwords don't match".yellow().to_string())?;
                return Ok(CommandResult::Handled)
            }

            let _lock = lock_users_storage()?;

            let mut users = load_json("data/users.json")?;
            
            if users.get(&username).is_some() {
                send_message(&client, &"Error: Name is already taken".yellow().to_string())?;
                return Ok(CommandResult::Handled);
            }

            let password_hash = generate_hash(&password);

            users[&username] = json!({
                "password": password_hash,
                "ignore": []
            });

            save_json("data/users.json", &users)?;

            let mut c = lock_client(&client)?;
            c.state = ClientState::LoggedIn { username: username.clone() };
            writeln!(c.stream, "{}", format!("/LOGIN_OK {}", username))?;

            writeln!(c.stream, "{}", format!("User Registered: {username}").green())?;
            Ok(CommandResult::Handled)
        }

        Command::AccountLogin {username, password} => {
            {
                let mut c = lock_client(&client)?;
                let now = Instant::now();
                c.login_attempts.retain(|t| now.duration_since(*t).as_secs() < 60);
                if c.login_attempts.len() >= 5 {
                    writeln!(c.stream, "{}", "Too many login attempts, try again later".yellow())?;
                    return Ok(CommandResult::Handled);
                }
                c.login_attempts.push_back(now);
            }

            if is_user_logged_in(clients, &username) {
                send_message(&client, &format!("Error: {username} is already logged in").yellow().to_string())?;
                return Ok(CommandResult::Handled);
            }

            let _lock = lock_users_storage()?;

            let users = load_json("data/users.json")?;

            match users.get(&username) {
                Some(user_obj) => {
                    let stored_hash = match user_obj.get("password").and_then(|v| v.as_str()) {
                        Some(hash) => hash,
                        None => {
                            send_message(&client, &"Error: Malformed user data".yellow().to_string())?;
                            return Ok(CommandResult::Handled);
                        }
                    };
                    if generate_hash(&password) == stored_hash {
                        let mut client = lock_client(&client)?;
                        client.state = ClientState::LoggedIn { username: username.clone() };
                        client.ignore_list = user_obj.get("ignore")
                            .and_then(|v| v.as_array())
                            .map_or_else(Vec::new, |arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect());
                        writeln!(client.stream, "{}", format!("/LOGIN_OK {}", username))?;

                        writeln!(client.stream, "{}", format!("Logged in as: {username}").green())?;
                    } else {
                        send_message(&client, &"Error: Incorrect password".yellow().to_string())?;
                    }
                }
                None => {
                    send_message(&client, &"Error: Username not found".yellow().to_string())?;
                }
            }

            Ok(CommandResult::Handled)
        }

        Command::AccountLogout | Command::AccountEditUsername { .. } | Command::AccountEditPassword { .. } => {
            send_message(&client, &"You are not currently logged in".yellow().to_string())?;
            Ok(CommandResult::Handled)
        }

        Command::AccountImport { filename } => {
            let safe_filename = if !filename.ends_with(".json") {
                format!("{filename}.json")
            }
            else {
                filename
            };

            let import_path = format!("data/vault/users/{safe_filename}");
            let import_file = match File::open(&import_path) {
                Ok(file) => file,
                Err(_) => {
                    send_message(&client, &format!("Error: Could not open {import_path}").yellow().to_string())?;
                    return Ok(CommandResult::Handled);
                }
            };

            let import_reader = BufReader::new(import_file);
            let import_user: Value = match serde_json::from_reader(import_reader) {
                Ok(data) => data,
                Err(_) => {
                    send_message(&client, &"Error: Invalid JSON format in import file".yellow().to_string())?;
                    return Ok(CommandResult::Handled);
                }
            };

            let (username, user_data) = match import_user.as_object().and_then(|obj| obj.iter().next()) {
                Some((u, data)) => (u.clone(), data.clone()),
                None => {
                    send_message(&client, &"Error: Import file is empty or malformed".yellow().to_string())?;
                    return Ok(CommandResult::Handled);
                }
            };

            let _lock = lock_users_storage()?;
            let mut users = load_json("data/users.json")?;

            if users.get(&username).is_some() {
                send_message(&client, &format!("Error: User {username} already exists").yellow().to_string())?;
                return Ok(CommandResult::Handled);
            }

            users[&username] = user_data;

            save_json("data/users.json", &users)?;

            send_success(&client, &format!("Imported user: {username}"))?;
            Ok(CommandResult::Handled)
        }

        Command::AccountExport { .. } => {
            send_success(&client, "Currently a guest, please register or log into an account to export account data")?;
            Ok(CommandResult::Handled)
        }

        Command::AccountDelete { .. } => {
            send_success(&client, "Currently a guest, cannot delete an account")?;
            Ok(CommandResult::Handled)
        }

        Command::Account => {
            send_success(&client, "Must register or log into an account to join a room")?;
            Ok(CommandResult::Handled)
        }

        Command::RoomList | Command::RoomCreate { .. } | Command::RoomJoin { .. } | Command::RoomImport { .. } | Command::RoomDelete { .. } => {
            send_message(&client, &"Must log in to perform this command".yellow().to_string())?;
            Ok(CommandResult::Handled)
        }

        Command::InvalidSyntax {err_msg } => {
            send_message(&client, &err_msg)?;
            Ok(CommandResult::Handled)
        }

        Command::Unavailable => {
            send_error(&client, "Command not available, use /help to see available commands")?;
            Ok(CommandResult::Handled)
        }
    }
}
