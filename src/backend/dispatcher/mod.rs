use std::sync::{Arc, Mutex};

use crate::shared::utils::{lock_client};

pub mod guest;
pub mod loggedin;
pub mod inroom;

pub enum CommandResult {
    Handled,
    Stop,
}

use crate::backend::parser::Command;
use crate::shared::types::{Client, ClientState, Clients, PublicKeys, Rooms};
use std::io;

pub fn dispatch_command(cmd: Command, client: Arc<Mutex<Client>>, clients: &Clients, rooms: &Rooms, pubkeys: &PublicKeys) -> io::Result<CommandResult> {
    let state = {
        let locked = lock_client(&client)?;
        locked.state.clone()
    };
    
    match state {
        ClientState::Guest => guest::guest_command(cmd, client, clients, rooms),
        ClientState::LoggedIn { username } => loggedin::loggedin_command(cmd, client, clients, rooms, &username, pubkeys),
        ClientState::InRoom { username, room, .. } => inroom::inroom_command(cmd, client, clients, rooms, &username, &room, pubkeys)
    }
}
