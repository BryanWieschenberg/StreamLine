use std::collections::{HashSet, HashMap};
use once_cell::sync::Lazy;
use sha2::{Sha256, Digest};
use colored::Colorize;
use std::io::{self, Write};
use std::sync::{Arc, Mutex};
use std::fs::write;
use crate::state::types::{Clients, ClientState};
use crate::state::types::Roles;
use crate::commands::parser::Command;
use crate::state::types::{Client, Rooms};
use crate::utils::{lock_client, lock_room, lock_rooms, lock_rooms_storage};
use crate::state::types::Room;

pub static DESCRIPTIONS: Lazy<HashMap<&'static str, &'static str>> = Lazy::new(|| {
    HashMap::from([
        ("afk",             "> /afk                Set yourself as away"),
        ("send",            "> /send <user> <file> Send a file to the room"),
        ("msg",             "> /msg <user> <msg>   Send a private message"),
        ("me",              "> /me <msg>           Send an emote message"),
        ("super",           "> /super              Administrator commands"),
        ("super.users",     "> /super users        Show all room user data"),
        ("super.rename",    "> /super rename       Changes room name"),
        ("super.export",    "> /super export       Saves room data"),
        ("super.whitelist", "> /super whitelist    Manage room whitelist"),
        ("super.limit",     "> /super limit        Manage room rate limits"),
        ("super.roles",     "> /super roles        Manage room roles and permissions"),
        ("user",            "> /user               Manage user settings"),
        ("user.list",       "> /user list          Show all visible room users"),
        ("user.rename",     "> /user rename        Changes your name in the room"),
        ("user.recolor",    "> /user recolor       Changes your name color in the room"),
        ("user.ignore",     "> /user ignore        Stops messages from certain users"),
        ("user.hide",       "> /user hide          Hides you from /user list"),
        ("mod",             "> /mod                Use chat moderation tools"),
        ("mod.kick",        "> /mod kick           Kick users from the chat"),
        ("mod.mute",        "> /mod mute           Disable certain users from speaking"),
        ("mod.ban",         "> /mod ban            Disable certain users from joining")
    ])
});

pub static RESTRICTED_COMMANDS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    HashSet::from([
        "afk", "send", "msg", "me",
        "super", "super.users", "super.rename", "super.export", "super.whitelist", "super.limit", "super.roles",
        "user", "user.list", "user.rename", "user.recolor", "user.ignore", "user.hide",
        "mod", "mod.kick", "mod.ban", "mod.mute",
    ])
});

pub fn command_order() -> Vec<&'static str> {
    vec![
        "help", "clear", "ping", "quit", "leave", "status", "ignore",
        "afk", "send", "msg", "me",
        "super", "super.users", "super.rename", "super.export", "super.whitelist", "super.limit", "super.roles",
        "user", "user.list", "user.rename", "user.recolor", "user.ignore", "user.hide",
        "mod", "mod.kick", "mod.ban", "mod.mute"
    ]
}

pub fn always_visible() -> Vec<&'static str> {
    vec![
        "Available commands:",
        "> /help               Show this help menu",
        "> /clear              Clear the chat screen",
        "> /ping               Check connection to the server",
        "> /quit               Exit the application",
        "> /leave              Leave your current room",
        "> /status             Show your current room info",
        "> /ignore             Manage ignore list"
    ]
}

pub fn help_msg_guest() -> &'static str {
r#"Available commands:
> /help               Show this help menu
> /clear              Clear the chat screen
> /ping               Check connection to the server
> /quit               Exit the application
> /account            Manage your account"#
}

pub fn help_msg_loggedin() -> &'static str {
r#"Available commands:
> /help               Show this help menu
> /clear              Clear the chat screen
> /ping               Check connection to the server
> /quit               Exit the application
> /account            Manage your account
> /room               Manage chat rooms
> /ignore             Manage ignore list"#
}

pub fn help_msg_inroom(extra_cmds: Vec<&str>) -> String {
    let shown_cmds: HashSet<String> = extra_cmds.into_iter().map(|s| s.to_string()).collect();
    let descriptions = &*DESCRIPTIONS;

    let mut ordered_filtered = always_visible().into_iter().map(String::from).collect::<Vec<_>>();

    for cmd in command_order() {
        if shown_cmds.contains(cmd) {
            if let Some(desc) = descriptions.get(cmd) {
                ordered_filtered.push(desc.to_string());
            }
        }
    }

    ordered_filtered.join("\n")
}

pub trait ColorizeExt {
    fn truecolor_from_hex(self, hex: &str) -> colored::ColoredString;
}

impl<'a> ColorizeExt for String {
    fn truecolor_from_hex(self, hex: &str) -> colored::ColoredString {
        let hex = hex.trim_start_matches('#');
        if hex.len() != 6 {
            return self.normal();
        }
        let r = u8::from_str_radix(&hex[0..2], 16).map_or(255, |v| v);
        let g = u8::from_str_radix(&hex[2..4], 16).map_or(255, |v| v);
        let b = u8::from_str_radix(&hex[4..6], 16).map_or(255, |v| v);
        self.truecolor(r, g, b)
    }
}

pub fn generate_hash(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input);
    let result = hasher.finalize();

    hex::encode(result)
}

pub fn is_user_logged_in(clients: &Clients, username: &str) -> bool {
    if let Ok(locked) = clients.lock() {
        for client_arc in locked.values() {
            if let Ok(client) = client_arc.lock() {
                match &client.state {
                    ClientState::LoggedIn { username: u } |
                    ClientState::InRoom { username: u, .. } if u == username => return true,
                    _ => continue,
                }
            }
        }
    }
    false
}

pub fn check_role_permissions(role: &str, command: &str, roles: &Roles) -> bool {
    match role {
        "owner" | "admin" => true, // full access
        "moderator" => roles.moderator.iter().any(|c| c == command),
        "user" => roles.user.iter().any(|c| c == command),
        _ => false,
    }
}

pub fn has_permission(cmd: &Command, client: Arc<Mutex<Client>>, rooms: &Rooms, username: &String, room: &String) -> io::Result<bool> {
    let cmd_str = cmd.to_string();

    if cmd_str.is_empty() || !RESTRICTED_COMMANDS.contains(cmd_str.as_str()) {
        return Ok(true)
    }

    let rooms_map = lock_rooms(rooms)?;
    let room_arc = match rooms_map.get(room) {
        Some(r) => Arc::clone(r),
        None => {
            let mut client = lock_client(&client)?;
            writeln!(client.stream, "{}", format!("Room {} not found", room).yellow())?;
            return Ok(false)
        }
    };

    let room_guard = lock_room(&room_arc)?;
    let role = match room_guard.users.get(username) {
        Some(u) => u.role.as_str(),
        None => {
            let mut client = lock_client(&client)?;
            writeln!(client.stream, "{}", "Error: You are not registered in this room".red())?;
            return Ok(false);
        }
    };

    if !check_role_permissions(role, cmd_str.as_str(), &room_guard.roles) {
        let mut client = lock_client(&client)?;
        writeln!(client.stream, "{}", "You don't have permission to run this command".red())?;
        return Ok(false)
    }

    return Ok(true)
}

pub fn save_rooms_to_disk(map: &HashMap<String, Arc<Mutex<Room>>>) -> std::io::Result<()> {
    let _lock = lock_rooms_storage()?; // handle poisoned lock gracefully

    let mut serializable_map = HashMap::new();
    for (k, arc_mutex_room) in map.iter() {
        match lock_room(arc_mutex_room) {
            Ok(room_guard) => {
                serializable_map.insert(k.clone(), room_guard.clone());
            }
            Err(e) => {
                eprintln!("Failed to lock room '{}': {}", k, e);
            }
        }
    }

    let json = serde_json::to_string_pretty(&serializable_map)?;
    write("data/rooms.json", json)
}
