use std::net::TcpStream;
use std::io;
use colored::*;

pub enum Command {
    Exit,
    Ping,
    Help,
    Unknown,
}

pub enum CommandResult {
    Exit,
    Handled,
    NotACommand,
}

pub fn parse_command(input: &str) -> Command {
    match input {
        "/exit" | "/quit" | "/e" | "/q" => Command::Exit,
        "/ping" => Command::Ping,
        "/help" => Command::Help,
        _ => Command::Unknown,
    }
}

pub fn handle_cmd(input: &str, stream: &mut TcpStream) -> io::Result<CommandResult> {
    match parse_command(input) {
        Command::Ping => {
            println!("{}", "Pong!".green());
            Ok(CommandResult::Handled)
        }
        Command::Help => {
            println!("{}", "Available commands: /exit /ping /help".green());
            Ok(CommandResult::Handled)
        }
        Command::Exit => {
            println!("{}", "Exiting...".green());
            stream.shutdown(std::net::Shutdown::Both)?;
            Ok(CommandResult::Exit)
        }
        Command::Unknown => Ok(CommandResult::NotACommand),
    }
}
