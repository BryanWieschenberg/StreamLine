use std::io::{self, Write};
use std::sync::{Arc, Mutex};
use colored::*;

use crate::commands::parser::Command;
use crate::commands::command_utils::get_help_message;
use crate::state::types::{Client, Clients};
use super::CommandResult;
use crate::utils::{lock_client};

pub fn handle_inroom_command(cmd: Command, client: Arc<Mutex<Client>>, _clients: &Clients, username: &String, room: &String) -> io::Result<CommandResult> {
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
            writeln!(client.stream, "{}", "You must be in the lobby to log out".yellow())?;
            Ok(CommandResult::Handled)
        }

        Command::Account => {
            let mut client = lock_client(&client)?;
            writeln!(client.stream, "{}", format!("Currently logged in as: {} (room: {})", username, room).green())?;
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
