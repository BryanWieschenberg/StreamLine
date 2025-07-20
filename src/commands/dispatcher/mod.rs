use std::sync::{Arc, Mutex};

use crate::utils::{lock_client};

pub mod guest;
pub mod loggedin;
pub mod inroom;

pub enum CommandResult {
    Handled,
    Stop,
}

use crate::commands::parser::Command;
use crate::state::types::{Client, Clients, ClientState};
use std::io;

pub fn dispatch_command(cmd: Command, client: Arc<Mutex<Client>>, clients: &Clients) -> io::Result<CommandResult> {
    let state = {
        let locked = lock_client(&client)?;
        locked.state.clone()
    };
    
    match state {
        ClientState::Guest => guest::handle_guest_command(cmd, client, clients),
        ClientState::LoggedIn { username } => loggedin::handle_loggedin_command(cmd, client, clients, &username),
        ClientState::InRoom { username, room } => inroom::handle_inroom_command(cmd, client, clients, &username, &room)
    }
}
