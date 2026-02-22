use std::collections::{HashSet, HashMap};
use once_cell::sync::Lazy;
use sha2::{Sha256, Digest};
use colored::Colorize;
use std::io::{self, Write};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use serde::Serialize;
use serde_json::{Serializer};
use serde_json::ser::PrettyFormatter;
use crate::types::{Clients, Client, ClientState, Rooms, Room, Roles};
use crate::commands::parser::Command;
use crate::utils::{lock_client, lock_room, lock_rooms, lock_rooms_storage};

pub static DESCRIPTIONS: Lazy<HashMap<&'static str, &'static str>> = Lazy::new(|| {
    HashMap::from([
        ("afk",             "> /afk              Set yourself as away"),
        ("msg",             "> /msg <user> <msg> Send a private message"),
        ("me",              "> /me <msg>         Send an emote message"),
        ("seen",            "> /seen <user>      See when a user was last online"),
        ("announce",        "> /announce <msg>   Announce a room message, bypass ignores"),
        ("super",           "> /super            Administrator commands"),
        ("super.users",     "> /super users      Show all room user data"),
        ("super.rename",    "> /super rename     Changes room name"),
        ("super.export",    "> /super export     Saves room data"),
        ("super.whitelist", "> /super whitelist  Manage room whitelist"),
        ("super.limit",     "> /super limit      Manage room rate limits"),
        ("super.roles",     "> /super roles      Manage room roles and permissions"),
        ("user",            "> /user             Manage user settings"),
        ("user.list",       "> /user list        Show all visible room users"),
        ("user.rename",     "> /user rename      Changes your name in the room"),
        ("user.recolor",    "> /user recolor     Changes your name color in the room"),
        ("user.hide",       "> /user hide        Hides you from /user list"),
        ("mod",             "> /mod              Use chat moderation tools"),
        ("mod.info",        "> /mod info         Show who is muted and banned"),
        ("mod.kick",        "> /mod kick         Kick users from the chat"),
        ("mod.mute",        "> /mod mute         Disable certain users from speaking"),
        ("mod.ban",         "> /mod ban          Disable certain users from joining")
    ])
});

pub static RESTRICTED_COMMANDS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    HashSet::from([
        "afk", "msg", "me", "seen", "announce",
        "super", "super.users", "super.rename", "super.export", "super.whitelist", "super.limit", "super.roles",
        "user", "user.list", "user.rename", "user.recolor", "user.hide",
        "mod", "mod.info", "mod.kick", "mod.ban", "mod.mute",
    ])
});

pub fn command_order() -> Vec<&'static str> {
    vec![
        "help", "clear", "ping", "quit", "leave", "status", "ignore",
        "afk", "msg", "me", "seen", "announce",
        "super", "super.users", "super.rename", "super.export", "super.whitelist", "super.limit", "super.roles",
        "user", "user.list", "user.rename", "user.recolor", "user.hide",
        "mod", "mod.info", "mod.kick", "mod.ban", "mod.mute"
    ]
}

pub fn always_visible() -> Vec<&'static str> {
    vec![
        "Available commands:",
        "> /help             Show this help menu",
        "> /clear            Clear the chat screen",
        "> /ping             Check connection to the server",
        "> /quit             Exit the application",
        "> /leave            Leave your current room",
        "> /status           Show your current room info",
        "> /ignore           Manage ignore list"
    ]
}

pub fn help_msg_guest() -> &'static str {
r#"Available commands:
> /help             Show this help menu
> /clear            Clear the chat screen
> /ping             Check connection to the server
> /quit             Exit the application
> /account          Manage your account"#
}

pub fn help_msg_loggedin() -> &'static str {
r#"Available commands:
> /help             Show this help menu
> /clear            Clear the chat screen
> /ping             Check connection to the server
> /quit             Exit the application
> /account          Manage your account
> /room             Manage chat rooms
> /ignore           Manage ignore list"#
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

impl ColorizeExt for &str {
    fn truecolor_from_hex(self, hex: &str) -> colored::ColoredString {
        self.to_string().truecolor_from_hex(hex)
    }
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
    fn granted(cmds: &[String], command: &str) -> bool {
        if cmds.iter().any(|c| c == command) {
            return true;
        }
        if let Some(parent) = command.split('.').next() {
            return cmds.iter().any(|c| c == parent);
        }
        false
    }

    match role {
        "owner" | "admin" => true,
        "moderator" => granted(&roles.moderator, command),
        "user"      => granted(&roles.user, command),
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
    let _lock = lock_rooms_storage()?;

    let mut snapshot = HashMap::new();
    for (name, arc) in map.iter() {
        if let Ok(room) = arc.lock() {
            snapshot.insert(name.clone(), room.clone());
        } else {
            eprintln!("Failed to lock room '{}'", name);
        }
    }
    let file = std::fs::OpenOptions::new().create(true).write(true).truncate(true).open("data/rooms.json")?;
    let mut writer = std::io::BufWriter::new(file);
    let formatter = PrettyFormatter::with_indent(b"    ");
    let mut ser = Serializer::with_formatter(&mut writer, formatter);
    snapshot.serialize(&mut ser).map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

    Ok(())
}

pub fn unix_timestamp(rooms: &Rooms, room_name: &str, username: &str) -> io::Result<()> {
    let ts: u64 = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(dur) => dur.as_secs(),
        Err(_)  => 0,
    };

    {
        let rooms_map = lock_rooms(rooms)?;
        if let Some(room_arc) = rooms_map.get(room_name) {
            if let Ok(mut room_guard) = lock_room(room_arc) {
                if let Some(entry) = room_guard.users.get_mut(username) {
                    entry.last_seen = ts;
                } else {
                    return Ok(());
                }
            } else {
                return Ok(());
            }
        } else {
            return Ok(());
        }
    }

    {
        let rooms_map = lock_rooms(rooms)?;
        save_rooms_to_disk(&rooms_map)?;
    }

    Ok(())
}

pub fn duration_format_passes(duration: &str) -> bool{
    if duration == "*" {
        true
    } else {
        let re = match regex::Regex::new(r"^(\d+d)?(\d+h)?(\d+m)?(\d+s)?$") {
            Ok(r) => r,
            Err(_) => return false,
        };
        re.is_match(duration)
    }
}

pub fn parse_duration(spec: &str) -> io::Result<u64> {
    if spec == "*" { return Ok(0); }
    let mut secs: u64 = 0;
    let mut num = String::new();
    for ch in spec.chars() {
        if ch.is_ascii_digit() {
            num.push(ch);
        } else {
            let val: u64 = num.parse().map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidInput, "Invalid duration number")
            })?;
            num.clear();
            match ch {
                'd' | 'D' => secs += val * 86_400,
                'h' | 'H' => secs += val * 3_600,
                'm' | 'M' => secs += val * 60,
                's' | 'S' => secs += val,
                _ => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "Invalid duration specifier",
                    ))
                }
            }
        }
    }
    if !num.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Duration spec ended unexpectedly",
        ));
    }
    Ok(secs)
}
