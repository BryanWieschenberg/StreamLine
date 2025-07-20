use std::io::{self, BufRead, BufReader, Write};
use std::fs::{File, OpenOptions};
use serde::Serialize;
use serde_json::{json, Serializer, Value};
use serde_json::ser::PrettyFormatter;
use std::sync::{Arc, Mutex};
use colored::*;

use crate::commands::parser::Command;
use crate::commands::command_utils::{get_help_message, generate_hash};
use crate::state::types::{Client, Clients, ClientState};
use crate::utils::{lock_client, lock_users};
use super::CommandResult;

pub fn handle_loggedin_command(cmd: Command, client: Arc<Mutex<Client>>, _clients: &Clients, username: &String) -> io::Result<CommandResult> {
    match cmd {
        Command::Help => {
            let mut client = lock_client(&client)?;
            writeln!(client.stream, "{}{}", get_help_message().green(), "\x1b[0m")?;
            io::stdout().flush()?;
            Ok(CommandResult::Handled)
        }

        Command::Ping => {
            let mut client = lock_client(&client)?;
            writeln!(client.stream, "{}", "PONG.".green())?;
            Ok(CommandResult::Handled)
        }

        Command::Quit => {
            let mut client = lock_client(&client)?;
            writeln!(client.stream, "{}", "Exiting...".green())?;
            client.stream.shutdown(std::net::Shutdown::Both)?;
            Ok(CommandResult::Stop)
        }

        Command::AccountRegister { .. } => {
            let mut client = lock_client(&client)?;
            writeln!(client.stream, "{}", "You are already logged in".yellow())?;
            Ok(CommandResult::Handled)
        }

        Command::AccountLogin { .. } => {
            let mut client = lock_client(&client)?;
            writeln!(client.stream, "{}", "You are already logged in".yellow())?;
            Ok(CommandResult::Handled)
        }

        Command::AccountLogout => {
            let mut client = lock_client(&client)?;
            client.state = ClientState::Guest;
            writeln!(client.stream, "{}", format!("Logged out: {}", username).green())?;
            
            Ok(CommandResult::Handled)
        }

        Command::AccountEditUsername { username: new_username } => {
            let mut client = lock_client(&client)?;
            let _lock = lock_users()?;

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
            writeln!(client.stream, "{}", format!("Username changed from {} to: {}", old_username, new_username).green())?;
            Ok(CommandResult::Handled)
        }

        Command::AccountEditPassword { current_password, new_password } => {
            let mut client = lock_client(&client)?;
            let _lock = lock_users()?;

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

            let import_path = format!("data/logs/users/{}", safe_filename);
            let import_file = match File::open(&import_path) {
                Ok(file) => file,
                Err(_) => {
                    let mut client = lock_client(&client)?;
                    writeln!(client.stream, "{}", format!("Error: Could not open '{}'", import_path).yellow())?;
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

            let _lock = lock_users()?;

            let file = File::open("data/users.json")?;
            let reader = BufReader::new(file);
            let mut users: Value = serde_json::from_reader(reader)?;

            if users.get(&username).is_some() {
                let mut client = lock_client(&client)?;
                writeln!(client.stream, "{}", format!("Error: User '{}' already exists", username).yellow())?;
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
            let _lock = lock_users()?;

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

            let export_path = format!("data/logs/users/{}", final_filename);
            let export_file = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&export_path)?;

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

            let _lock = lock_users()?;
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

        Command::InvalidSyntax {err_msg } => {
            let mut client = lock_client(&client)?;
            writeln!(client.stream, "{}", err_msg)?;
            Ok(CommandResult::Handled)
        }

        Command::Unavailable => {
            let mut client = lock_client(&client)?;
            writeln!(client.stream, "{}", "Command unavailable, use /help to see available commands".red())?;
            Ok(CommandResult::Handled)
        }
    }
}
