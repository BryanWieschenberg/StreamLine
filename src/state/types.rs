use std::collections::{HashMap, HashSet};
use std::net::{TcpStream, SocketAddr};
use std::sync::{Arc, Mutex};
use once_cell::sync::Lazy;

#[allow(dead_code)]
pub enum InputMode {
    Normal,
    Autocomplete,
    History,
}

#[allow(dead_code)]
pub struct App {
    pub input: String,                 // What the user is currently typing
    pub messages: Vec<String>,         // Message history (chat window)
    pub suggestions: Vec<String>,      // Autocomplete suggestions
    pub selected_suggestion: usize,    // Index of selected suggestion (if any)
    pub input_mode: InputMode,         // Typing vs navigating suggestions
    pub current_room: Option<String>,  // Current joined room, if any
    pub logged_in: bool,               // Whether the user is authenticated
    pub username: Option<String>,      // Logged-in username
    pub history_index: Option<usize>,  // For up/down command history cycling
    pub command_history: Vec<String>,  // Command history
}

#[allow(dead_code)]
pub enum ClientState {
    Guest,
    LoggedIn {username: String},
    InRoom {username: String, room: String}
}

#[allow(dead_code)]
pub struct Client {
    pub stream: TcpStream,
    pub addr: SocketAddr,
    pub state: ClientState
}

#[allow(dead_code)]
pub type Clients = Arc<Mutex<HashMap<SocketAddr, Arc<Mutex<Client>>>>>;

#[allow(dead_code)]
pub struct Room {
    pub name: String,
    pub users: HashSet<String>,
    pub creator: String,
    pub whitelist_enabled: bool,
    pub whitelist: HashSet<String>,
    pub roles: HashMap<String, String>
}

pub type Rooms = Arc<Mutex<HashMap<String, Room>>>;

// User file access lock
pub static USERS_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));
pub static ROOMS_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));
