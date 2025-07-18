use std::io::{self, BufReader, Write};
use std::fs::{File, OpenOptions};
use serde::Serialize;
use serde_json::{Value, json, Serializer};
use serde_json::ser::PrettyFormatter;
use colored::*;

use crate::commands::parser::Command;
use crate::commands::command_utils::{get_help_message, generate_hash};

use crate::state::types::{Client, ClientState, USERS_LOCK, ROOMS_LOCK};

#[allow(dead_code)]
pub enum CommandResult {
    Handled,
    Stop,
    // PermissionDenied,
    // InvalidArgs,
    // Error(String),
}

#[allow(dead_code)]
pub fn dispatch_command(cmd: Command, client: &mut Client) -> io::Result<CommandResult> {
    match cmd {
        Command::Help => {
            writeln!(client.stream, "{}{}", get_help_message().green(), "\x1b[0m")?;
            io::stdout().flush()?;
            Ok(CommandResult::Handled)
        }

        Command::Ping => {
            writeln!(client.stream, "{}", "Pong!".green())?;
            Ok(CommandResult::Handled)
        }

        Command::Quit => {
            writeln!(client.stream, "{}", "Exiting...".green())?;
            client.stream.shutdown(std::net::Shutdown::Both)?;
            Ok(CommandResult::Stop)
        }

        Command::AccountRegister {username, password, confirm} => {
            if password != confirm {
                writeln!(client.stream, "{}", "Error: Passwords don't match".yellow())?;
            }
            else {
                let _lock = match USERS_LOCK.lock() {
                    Ok(lock) => lock,
                    Err(_) => {
                        writeln!(client.stream, "{}", "Error: Could not acquire user lock".yellow())?;
                        return Ok(CommandResult::Handled);
                    }
                };
            
                let file = File::open("data/users.json")?;
                let reader = BufReader::new(file);
                let mut users: Value = serde_json::from_reader(reader)?;

                if users.get(&username).is_some() {
                    writeln!(client.stream, "{}", "Error: Name is already taken".yellow())?;
                }
                else {
                    let password_hash = generate_hash(&password);
    
                    // Add a new user
                    users[&username] = json!({
                        "password": password_hash,
                        "ignore": []
                    });

                    // Write back to file
                    let file = OpenOptions::new()
                        .write(true)
                        .truncate(true)
                        .open("data/users.json")?;

                    // Append the new user to users.json
                    let mut writer = std::io::BufWriter::new(file);
                    let formatter = PrettyFormatter::with_indent(b"    ");
                    let mut ser = Serializer::with_formatter(&mut writer, formatter);
                    users.serialize(&mut ser)?;

                    client.state = ClientState::LoggedIn { username: username.clone() };

                    writeln!(client.stream, "{}", format!("User Registered: {}", username).green())?;
                }
            }

            Ok(CommandResult::Handled)
        }

        Command::AccountLogin {username, password} => {
            let _lock = match USERS_LOCK.lock() {
                Ok(lock) => lock,
                Err(_) => {
                    writeln!(client.stream, "{}", "Error: Could not acquire user lock".yellow())?;
                    return Ok(CommandResult::Handled);
                }
            };

            let file = File::open("data/users.json")?;
            let reader = BufReader::new(file);
            let users: Value = serde_json::from_reader(reader)?;

            match users.get(&username) {
                Some(user_obj) => {
                    let stored_hash = user_obj.get("password").and_then(|v| v.as_str()).unwrap_or("");
                    if generate_hash(&password) == stored_hash {
                        client.state = ClientState::LoggedIn { username: username.clone() };
                        writeln!(client.stream, "{}", format!("Logged in as: {}", username).green())?;
                    } else {
                        writeln!(client.stream, "{}", "Error: Incorrect password".yellow())?;
                    }
                }
                None => {
                    writeln!(client.stream, "{}", "Error: Username not found".yellow())?;
                }
            }

            Ok(CommandResult::Handled)
        }

        Command::AccountLogout {} => {
            writeln!(client.stream, "Logged out")?;
            // TODO: validate password, set session
            Ok(CommandResult::Handled)
        }

        // Command::AccountEditUsername(new_username) => {
        //     writeln!(stream, "Editing username to: {}", new_username)?;
        //     // TODO: update username
        //     Ok(CommandResult::Handled)
        // }

        // Command::AccountEditPassword { new, confirm } => {
        //     writeln!(stream, "Editing password...")?;
        //     // TODO: verify & update password
        //     Ok(CommandResult::Handled)
        // }


        // Command::AccountExport(Some(filename)) => {
        //     writeln!(stream, "Exporting account to: {}", filename)?;
        //     // TODO: write account data to file
        //     Ok(CommandResult::Handled)
        // }

        // Command::AccountExport(None) => {
        //     writeln!(stream, "Exporting account to default timestamped file...")?;
        //     Ok(CommandResult::Handled)
        // }

        // Command::AccountDelete => {
        //     writeln!(stream, "Deleting account...")?;
        //     // TODO: confirm y/n, then remove
        //     Ok(CommandResult::Handled)
        // }

        // Command::AccountImport(filename) => {
        //     writeln!(stream, "Importing account from: {}", filename)?;
        //     // TODO: load account from JSON
        //     Ok(CommandResult::Handled)
        // }

        // // Command::RoomList => {
        // //     writeln!(stream, "Showing available rooms...")?;
        // //     // TODO: send actual room list from shared state
        // //     Ok(CommandResult::Handled)
        // // }

        // // Command::RoomJoin(name) => {
        // //     writeln!(stream, "Joining room: {}", name)?;
        // //     // TODO: logic to change client's current room
        // //     Ok(CommandResult::Handled)
        // // }

        // // Command::RoomCreate(name) => {
        // //     writeln!(stream, "Creating room: {}", name)?;
        // //     // TODO: create room, assign creator
        // //     Ok(CommandResult::Handled)
        // // }

        // // Command::RoomDelete(name) => {
        // //     writeln!(stream, "Deleting room: {}", name)?;
        // //     // TODO: delete room logic
        // //     Ok(CommandResult::Handled)
        // // }

        // // Command::RoomImport(filename) => {
        // //     writeln!(stream, "Importing room from: {}", filename)?;
        // //     // TODO: read file, create room
        // //     Ok(CommandResult::Handled)
        // // }

        Command::Unknown => {
            writeln!(client.stream, "{}", "Invalid command, use /help to see command formats".red())?;
            Ok(CommandResult::Handled)
        }
    }
}
