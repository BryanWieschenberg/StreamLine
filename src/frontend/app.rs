use std::collections::HashMap;
use std::sync::{Condvar, Mutex};
use once_cell::sync::Lazy;

pub enum ClientState {
    Guest,
    LoggedIn,
    InRoom,
}

pub static MEMBERS: Lazy<(Mutex<HashMap<String, String>>, Condvar)> =
    Lazy::new(|| (Mutex::new(HashMap::new()), Condvar::new()));

pub static MY_STATE: Lazy<Mutex<ClientState>> = Lazy::new(|| Mutex::new(ClientState::Guest));

pub static CURRENT_USER: Lazy<Mutex<String>> = Lazy::new(|| Mutex::new(String::new()));
pub static CURRENT_ROOM: Lazy<Mutex<String>> = Lazy::new(|| Mutex::new(String::new()));
pub static MY_ROLE: Lazy<Mutex<String>> = Lazy::new(|| Mutex::new(String::new()));
pub static ALLOWED_COMMANDS: Lazy<Mutex<Vec<String>>> = Lazy::new(|| Mutex::new(Vec::new()));

pub fn get_room_members() -> HashMap<String, String> {
    let (lock, _) = &*MEMBERS;
    match lock.lock() {
        Ok(m) => m.clone(),
        Err(e) => {
            eprintln!("Failed to lock MEMBERS: {e}");
            HashMap::new()
        }
    }
}

pub const COMMANDS_ALWAYS: &[&str] = &[
    "/help",
    "/clear",
    "/quit",
    "/ping",
];

pub const COMMANDS_GUEST: &[&str] = &[
    "/account register",
    "/account login",
    "/account import",
];

pub const COMMANDS_LOGGEDIN: &[&str] = &[
    "/account",
    "/account logout",
    "/account edit username",
    "/account edit password",
    "/account export",
    "/account delete",
    "/room list",
    "/room join",
    "/room create",
    "/room import",
    "/room delete",
];

pub const COMMANDS_IGNORE: &[&str] = &[
    "/ignore",
    "/ignore list",
    "/ignore add",
    "/ignore remove",
];

pub const COMMANDS_INROOM_BASE: &[&str] = &[
    "/leave",
    "/status",
];

pub enum AppMessage {
    ServerMessage(String),
    NetworkError(String),
    ControlResult(String),
}

pub struct Autocomplete {
    pub candidates: Vec<String>,
    pub index: Option<usize>,
    pub prefix: String,
}

impl Autocomplete {
    pub fn new() -> Self {
        Self { candidates: Vec::new(), index: None, prefix: String::new() }
    }

    pub fn populate(&mut self, input: &str, members: &[String]) {
        self.prefix = input.to_string();
        self.candidates.clear();
        self.index = None;

        if input.starts_with('/') {
            let state = MY_STATE.lock().ok();

            let mut available: Vec<&str> = COMMANDS_ALWAYS.to_vec();

            match state.as_deref() {
                Some(ClientState::Guest) | None => {
                    available.extend(COMMANDS_GUEST.iter().copied());
                }
                Some(ClientState::LoggedIn) => {
                    available.extend(COMMANDS_LOGGEDIN.iter().copied());
                }
                Some(ClientState::InRoom) => {
                    available.extend(COMMANDS_IGNORE.iter().copied());
                    available.extend(COMMANDS_INROOM_BASE.iter().copied());
                    
                    if let Ok(allowed) = ALLOWED_COMMANDS.lock() {
                        for cmd in allowed.iter() {
                            let full_cmd = if cmd.starts_with('/') {
                                cmd.clone()
                            } else {
                                format!("/{}", cmd.replace('.', " "))
                            };

                            if full_cmd.starts_with(input) {
                                if !self.candidates.contains(&full_cmd) {
                                    self.candidates.push(full_cmd);
                                }
                            }
                        }
                    }
                }
            }
            for cmd in available {
                if cmd.starts_with(input) {
                    if !self.candidates.contains(&cmd.to_string()) {
                        self.candidates.push(cmd.to_string());
                    }
                }
            }
        }

        self.candidates.sort();

        if self.candidates.is_empty() {
             if let Some(at_pos) = input.rfind('@') {
                let name_prefix = &input[at_pos + 1..];

                if !name_prefix.contains(' ') {
                    let base = &input[..at_pos]; 
                    for m in members {
                        if m.to_lowercase().starts_with(&name_prefix.to_lowercase()) {
                            self.candidates.push(format!("{base}{m}"));
                        }
                    }
                }
            }
        }
    }

    pub fn reset(&mut self) {
        self.candidates.clear();
        self.index = None;
        self.prefix.clear();
    }
}

pub struct App {
    pub messages: Vec<String>,
    pub input: String,
    pub should_quit: bool,
    pub autocomplete: Autocomplete,
    pub member_names: Vec<String>,

    pub status: String,
    pub scroll_offset: usize,
    pub input_history: Vec<String>,
    pub history_pos: Option<usize>,
    pub input_draft: String,
    pub popup_visible: bool,
    pub popup_selected: usize,
    pub popup_candidates: Vec<String>,
}

impl App {
    pub fn new() -> App {
        App {
            messages: Vec::new(),
            input: String::new(),
            should_quit: false,
            autocomplete: Autocomplete::new(),
            member_names: Vec::new(),

            status: String::from("Not logged in"),
            scroll_offset: 0,
            input_history: Vec::new(),
            history_pos: None,
            input_draft: String::new(),
            popup_visible: false,
            popup_selected: 0,
            popup_candidates: Vec::new(),
        }
    }

    pub fn push(&mut self, msg: String) {
        self.messages.push(msg);
    }

    pub fn refresh_member_names(&mut self) {
        if let Ok(m) = MEMBERS.0.lock() {
            self.member_names = m.keys().cloned().collect();
        }
    }

    pub fn update_status(&mut self) {
        let user = CURRENT_USER.lock().map(|u| u.clone()).unwrap_or_default();
        let room = CURRENT_ROOM.lock().map(|r| r.clone()).unwrap_or_default();
        self.status = match (user.is_empty(), room.is_empty()) {
            (true, _) => "Not logged in  ·  Press /help for commands".into(),
            (false, true) => format!("  {user}  ·  In lobby"),
            (false, false) => format!("  {user}  ·  #{room}"),
        };
    }
}
