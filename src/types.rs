use std::collections::{HashMap, VecDeque};
use std::net::{TcpStream, SocketAddr};
use std::sync::{Arc, Mutex};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::time::Instant;

#[derive(Clone)]
pub enum ClientState {
    Guest,
    LoggedIn {username: String},
    InRoom {
        username: String,
        room: String,
        room_time: Option<std::time::SystemTime>,
        msg_timestamps: VecDeque<Instant>,
        inactive_time: Option<std::time::SystemTime>,
        is_afk: bool
    }
}

pub struct Client {
    pub stream: TcpStream,
    pub addr: SocketAddr,
    pub state: ClientState,
    pub ignore_list: Vec<String>
}

pub type Clients = Arc<Mutex<HashMap<SocketAddr, Arc<Mutex<Client>>>>>;

#[derive(Serialize, Deserialize, Clone)]
pub struct Room {
    pub whitelist_enabled: bool,
    pub whitelist: Vec<String>,
    pub msg_rate: u8,
    pub session_timeout: u32,
    pub roles: Roles,
    pub users: HashMap<String, RoomUser>,
    #[serde(default, skip_serializing, skip_deserializing)]
    pub online_users: Vec<String>
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Roles {
    pub moderator: Vec<String>,
    pub user: Vec<String>,
    pub colors: HashMap<String, String>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct RoomUser {
    pub nick: String,
    pub color: String,
    pub role: String,
    pub hidden: bool,
    pub last_seen: u64,
    pub banned: bool,
    pub ban_stamp: u64,
    pub ban_length: u64,
    pub ban_reason: String,
    pub muted: bool,
    pub mute_stamp: u64,
    pub mute_length: u64,
    pub mute_reason: String
}

pub type Rooms = Arc<Mutex<HashMap<String, Arc<Mutex<Room>>>>>;

// User file access lock
pub static USERS_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));
pub static ROOMS_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

// --- For future TUI implementation ---
#[allow(dead_code)]
pub enum InputMode {
    Normal,
    Autocomplete,
    History,
}

#[allow(dead_code)]
pub struct AppState {
    pub input: String,                  // What the user is currently typing
    pub messages: Vec<String>,          // Message history (chat window)
    pub suggestions: Vec<String>,       // Autocomplete suggestions
    pub selected_suggestion: usize,     // Index of selected suggestion (if any)
    pub input_mode: InputMode,          // Typing vs navigating suggestions
    pub history_index: Option<usize>,   // For up/down command history cycling
    pub msg_history: Vec<String>,       // Message history
}
// -------------------------------------
