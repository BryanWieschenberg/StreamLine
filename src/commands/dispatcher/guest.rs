use std::io::{self, BufReader, Write};
use std::fs::{File, OpenOptions};
use serde::Serialize;
use serde_json::{Value, json, Serializer};
use serde_json::ser::PrettyFormatter;
use std::sync::{Arc, Mutex};
use colored::*;

use crate::commands::parser::Command;
use crate::commands::command_utils::{get_help_message, generate_hash, is_user_logged_in};
use crate::state::types::{Client, Clients, ClientState};
use crate::utils::{lock_client, lock_users};
use super::CommandResult;

pub fn handle_guest_command(cmd: Command, client: Arc<Mutex<Client>>, clients: &Clients) -> io::Result<CommandResult> {
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

        Command::AccountRegister {username, password, confirm} => {
            if password != confirm {
                let mut client = lock_client(&client)?;
                writeln!(client.stream, "{}", "Error: Passwords don't match".yellow())?;
                return Ok(CommandResult::Handled)
            }

            let _lock = lock_users()?;

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
                writeln!(client.stream, "{}", "Error: User is already logged in".yellow())?;
                return Ok(CommandResult::Handled);
            }

            let _lock = lock_users()?;

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

        Command::AccountLogout => {
            let mut client = lock_client(&client)?;
            writeln!(client.stream, "{}", "You are not currently logged in".yellow())?;
            Ok(CommandResult::Handled)
        }

        Command::AccountEditUsername { .. } => {
            let mut client = lock_client(&client)?;
            writeln!(client.stream, "{}", "You are not currently logged in".yellow())?;
            Ok(CommandResult::Handled)
        }

        Command::AccountEditPassword { .. } => {
            let mut client = lock_client(&client)?;
            writeln!(client.stream, "{}", "You are not currently logged in".yellow())?;
            Ok(CommandResult::Handled)
        }

        Command::Account => {
            let mut client = lock_client(&client)?;
            writeln!(client.stream, "{}", "Currently a guest, please register or log into an account to join a room".green())?;
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
