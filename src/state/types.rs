// src/state/types.rs
use std::collections::{HashMap, HashSet};
use std::net::TcpStream;
use std::sync::{Arc, Mutex};

pub struct Client {
    pub stream: TcpStream,
    pub username: String,
    pub current_room: String,
}

pub type RoomName = String;

pub struct Room {
    pub name: RoomName,
    pub users: HashSet<String>,
    pub creator: String,
    pub whitelist_enabled: bool,
    pub whitelist: HashSet<String>,
    pub roles: HashMap<String, String>
}

pub type Clients = Arc<Mutex<HashMap<String, Client>>>;
pub type Rooms = Arc<Mutex<HashMap<RoomName, Room>>>;
