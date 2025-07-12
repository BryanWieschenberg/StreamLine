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
            println!("{}", "
                Available commands:\n
                /exit OR /quit -> Exits the program\n
                /leave -> Leaves your current room\n
                /join (room name) -> Joins the specified room number or name\n
                /ping -> Checks your connection\n
                /users OR /room -> Shows all users currently in your room
                /perm (username)
                /perm (username) +/-(permissions) -> Adds (+) or revokes (-) the specified 
                /perms -> 
                /nick OR /rename -> Changes your username
                /msg OR /whisper -> Privately messages a user currently in your room
                /help -> Shows all available commands
            ".green());
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
