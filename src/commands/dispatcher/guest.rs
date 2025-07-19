use std::io::{self, BufReader, Write};
use std::fs::{File, OpenOptions};
use serde::Serialize;
use serde_json::{Value, json, Serializer};
use serde_json::ser::PrettyFormatter;
use colored::*;

use crate::commands::parser::Command;
use crate::commands::command_utils::{get_help_message, generate_hash};
use crate::state::types::{Client, ClientState};
use crate::utils::{lock_users};
use super::CommandResult;

pub fn handle_guest_command(cmd: Command, client: &mut Client) -> io::Result<CommandResult> {
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
                return Ok(CommandResult::Handled)
            }

            let _lock = lock_users()?;

            let file = File::open("data/users.json")?;
            let reader = BufReader::new(file);
            let mut users: Value = serde_json::from_reader(reader)?;
            
            if users.get(&username).is_some() {
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

            client.state = ClientState::LoggedIn { username: username.clone() };
            writeln!(client.stream, "{}", format!("User Registered: {}", username).green())?;

            Ok(CommandResult::Handled)
        }

        Command::AccountLogin {username, password} => {
            let _lock = lock_users()?;

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

        Command::AccountLogout => {
            writeln!(client.stream, "{}", "You are not currently logged in".yellow())?;
            Ok(CommandResult::Handled)
        }

        Command::Account => {
            writeln!(client.stream, "{}", "Currently a guest, please register or log into an account to join a room".green())?;
            Ok(CommandResult::Handled)
        }

        Command::InvalidSyntax {err_msg } => {
            writeln!(client.stream, "{}", err_msg)?;
            Ok(CommandResult::Handled)
        }

        Command::Unavailable => {
            writeln!(client.stream, "{}", "Command unavailable, use /help to see available commands".red())?;
            Ok(CommandResult::Handled)
        }
    }
}
