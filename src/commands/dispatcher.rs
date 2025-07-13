use std::io::{self, Write};
use std::net::TcpStream;
use crate::commands::parser::Command;
use crate::commands::utils::{get_help_message, generate_hash};

pub enum CommandResult {
    Handled,
    Exit,
    NotACommand,
}

pub fn dispatch_command(cmd: Command, stream: &mut TcpStream) -> io::Result<CommandResult> {
    match cmd {
        Command::Help => {
            writeln!(stream, "{}", get_help_message())?;
            Ok(CommandResult::Handled)
        }

        Command::Ping => {
            writeln!(stream, "Pong!")?;
            Ok(CommandResult::Handled)
        }

        Command::Quit => {
            writeln!(stream, "[!] Exiting...!")?;
            stream.shutdown(std::net::Shutdown::Both)?;
            Ok(CommandResult::Exit)
        }

        Command::Clear => {
            writeln!(stream, "\x1B[2J\x1B[H")?;
            Ok(CommandResult::Handled)
        }

        Command::RoomList => {
            writeln!(stream, "Showing available rooms...")?;
            // TODO: send actual room list from shared state
            Ok(CommandResult::Handled)
        }

        Command::RoomJoin(name) => {
            writeln!(stream, "Joining room: {}", name)?;
            // TODO: logic to change client's current room
            Ok(CommandResult::Handled)
        }

        Command::RoomCreate(name) => {
            writeln!(stream, "Creating room: {}", name)?;
            // TODO: create room, assign creator
            Ok(CommandResult::Handled)
        }

        Command::RoomDelete(name) => {
            writeln!(stream, "Deleting room: {}", name)?;
            // TODO: delete room logic
            Ok(CommandResult::Handled)
        }

        Command::RoomImport(filename) => {
            writeln!(stream, "Importing room from: {}", filename)?;
            // TODO: read file, create room
            Ok(CommandResult::Handled)
        }

        Command::AccountRegister { username, password, confirm } => {
            writeln!(stream, "Registering user: {}", username)?;
            // TODO: verify passwords match, hash & save user
            Ok(CommandResult::Handled)
        }

        Command::AccountEditUsername(new_username) => {
            writeln!(stream, "Editing username to: {}", new_username)?;
            // TODO: update username
            Ok(CommandResult::Handled)
        }

        Command::AccountEditPassword { new, confirm } => {
            writeln!(stream, "Editing password...")?;
            // TODO: verify & update password
            Ok(CommandResult::Handled)
        }

        Command::AccountLogin { username, password } => {
            writeln!(stream, "Logging in: {}", username)?;
            // TODO: validate password, set session
            Ok(CommandResult::Handled)
        }

        Command::AccountExport(Some(filename)) => {
            writeln!(stream, "Exporting account to: {}", filename)?;
            // TODO: write account data to file
            Ok(CommandResult::Handled)
        }

        Command::AccountExport(None) => {
            writeln!(stream, "Exporting account to default timestamped file...")?;
            Ok(CommandResult::Handled)
        }

        Command::AccountDelete => {
            writeln!(stream, "Deleting account...")?;
            // TODO: confirm y/n, then remove
            Ok(CommandResult::Handled)
        }

        Command::AccountImport(filename) => {
            writeln!(stream, "Importing account from: {}", filename)?;
            // TODO: load account from JSON
            Ok(CommandResult::Handled)
        }

        Command::Unknown => {
            writeln!(stream, "Unrecognized command.")?;
            Ok(CommandResult::NotACommand)
        }
    }
}
