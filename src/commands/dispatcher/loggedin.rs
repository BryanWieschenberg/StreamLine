use std::io::{self, BufReader, Write};
use std::fs::{File, OpenOptions};
use serde::Serialize;
use serde_json::{Value, Serializer};
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
            writeln!(client.stream, "{}", "Pong!".green())?;
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
