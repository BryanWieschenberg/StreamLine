pub mod guest;
pub mod loggedin;
pub mod inroom;

pub enum CommandResult {
    Handled,
    Stop,
}

use crate::commands::parser::Command;
use crate::state::types::{Client, ClientState};
use std::io;

pub fn dispatch_command(cmd: Command, client: &mut Client) -> io::Result<CommandResult> {
    match client.state.clone() {
        ClientState::Guest => guest::handle_guest_command(cmd, client),
        ClientState::LoggedIn { username } => loggedin::handle_loggedin_command(cmd, client, &username),
        ClientState::InRoom { username, room } => inroom::handle_inroom_command(cmd, client, &username, &room)
    }
}
