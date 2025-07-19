use std::io::{self, Write};
use colored::*;

use crate::commands::parser::Command;
use crate::commands::command_utils::get_help_message;
use crate::state::types::{Client, ClientState};
use super::CommandResult;

pub fn handle_loggedin_command(cmd: Command, client: &mut Client, username: &String) -> io::Result<CommandResult> {
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

        Command::AccountRegister { .. } => {
            writeln!(client.stream, "{}", "You are already logged in".yellow())?;
            Ok(CommandResult::Handled)
        }

        Command::AccountLogin { .. } => {
            writeln!(client.stream, "{}", "You are already logged in".yellow())?;
            Ok(CommandResult::Handled)
        }

        Command::AccountLogout => {
            client.state = ClientState::Guest;
            writeln!(client.stream, "{}", format!("Logged out: {}", username).green())?;
            
            Ok(CommandResult::Handled)
        }

        Command::Account => {
            writeln!(client.stream, "{}", format!("Currently logged in as: {} (not in a room)", username).green())?;
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
