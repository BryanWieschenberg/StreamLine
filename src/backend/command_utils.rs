use std::collections::{HashSet, HashMap};
use once_cell::sync::Lazy;
use sha2::{Sha256, Digest};
use colored::Colorize;
use std::io::{self, Write};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use crate::shared::types::{Clients, Client, ClientState, Rooms, Roles, PublicKeys};
use crate::backend::parser::Command;
use crate::shared::utils::{lock_client, lock_clients, lock_room, lock_rooms, save_rooms_to_disk};

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
        ("mod.unmute",      "> /mod unmute       Allow certain users to speak again"),
        ("mod.ban",         "> /mod ban          Disable certain users from joining"),
        ("mod.unban",       "> /mod unban        Allow certain users to join again")
    ])
});

pub static RESTRICTED_COMMANDS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    HashSet::from([
        "afk", "msg", "me", "seen", "announce",
        "super", "super.users", "super.rename", "super.export", 
        "super.whitelist", "super.whitelist.info", "super.whitelist.add", "super.whitelist.remove",
        "super.limit", "super.limit.info", "super.limit.rate", "super.limit.session",
        "super.roles", "super.roles.list", "super.roles.add", "super.roles.revoke", "super.roles.assign", "super.roles.recolor",
        "user", "user.list", "user.rename", "user.recolor", "user.hide",
        "mod", "mod.info", "mod.kick", "mod.ban", "mod.unban", "mod.mute", "mod.unmute",
    ])
});

pub fn command_order() -> Vec<&'static str> {
    vec![
        "help", "clear", "ping", "quit", "leave", "status", "ignore",
        "afk", "msg", "me", "seen", "announce",
        "super", "super.users", "super.rename", "super.export", 
        "super.whitelist", "super.whitelist.info", "super.whitelist.add", "super.whitelist.remove",
        "super.limit", "super.limit.info", "super.limit.rate", "super.limit.session",
        "super.roles", "super.roles.list", "super.roles.add", "super.roles.revoke", "super.roles.assign", "super.roles.recolor",
        "user", "user.list", "user.rename", "user.recolor", "user.hide",
        "mod", "mod.info", "mod.kick", "mod.ban", "mod.unban", "mod.mute", "mod.unmute"
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
        
        let parts: Vec<&str> = command.split('.').collect();
        for i in 1..parts.len() {
            let prefix = parts[0..i].join(".");
            if cmds.iter().any(|c| *c == prefix) {
                return true;
            }
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

pub fn has_permission(cmd: &Command, client_arc: Arc<Mutex<Client>>, rooms: &Rooms, username: &String, room: &String) -> io::Result<bool> {
    let cmd_str = cmd.to_string();

    if cmd_str.is_empty() || !RESTRICTED_COMMANDS.contains(cmd_str.as_str()) {
        return Ok(true)
    }

    let role = {
        let rooms_map = lock_rooms(rooms)?;
        let room_arc = match rooms_map.get(room) {
            Some(r) => Arc::clone(r),
            None => {
                let client = lock_client(&client_arc)?;
                writeln!(&client.stream, "{}", format!("Room {room} not found").yellow())?;
                return Ok(false)
            }
        };

        let room_guard = lock_room(&room_arc)?;
        let client = lock_client(&client_arc)?;
        match room_guard.users.get(username) {
            Some(u) => u.role.clone(),
            None => {
                writeln!(&client.stream, "{}", "Error: You are not registered in this room".red())?;
                return Ok(false);
            }
        }
    };

    let rooms_map = lock_rooms(rooms)?;
    let room_arc = rooms_map.get(room).ok_or_else(|| io::Error::other("Room disappeared"))?;
    let room_guard = lock_room(room_arc)?;

    if !check_role_permissions(&role, cmd_str.as_str(), &room_guard.roles) {
        let client = lock_client(&client_arc)?;
        writeln!(&client.stream, "{}", "You don't have permission to run this command".red())?;
        return Ok(false)
    }

    Ok(true)
}



pub fn sync_user_commands(client_arc: &Arc<Mutex<Client>>, rooms: &Rooms, username: &str, room_name: &str) -> io::Result<()> {
    let extra_cmds = {
        let rooms_map = lock_rooms(rooms)?;
        let room_arc = match rooms_map.get(room_name) {
            Some(arc) => arc,
            None => return Ok(()),
        };
        let room_guard = lock_room(room_arc)?;
        let role = match room_guard.users.get(username) {
            Some(u) => u.role.as_str(),
            None => "user",
        };
        let base_allowed = match role {
            "moderator" => room_guard.roles.moderator.clone(),
            "user" => room_guard.roles.user.clone(),
            "admin" | "owner" => {
                RESTRICTED_COMMANDS.iter().map(|s| s.to_string()).collect()
            },
            _ => Vec::new(),
        };

        if role == "admin" || role == "owner" {
            base_allowed
        } else {
            let mut expanded = HashSet::new();
            for cmd in base_allowed {
                expanded.insert(cmd.clone());
                let prefix = format!("{}.", cmd);
                for restricted in RESTRICTED_COMMANDS.iter() {
                    if restricted.starts_with(&prefix) {
                        expanded.insert(restricted.to_string());
                    }
                }
            }
            expanded.into_iter().collect()
        }
    };

    let mut extra_cmds = extra_cmds;
    extra_cmds.sort();

    let mut c = lock_client(client_arc)?;
    if !extra_cmds.is_empty() {
        writeln!(c.stream, "/CMDS {}", extra_cmds.join(" "))?;
        let _ = c.stream.flush();
    } else {
        writeln!(c.stream, "/CMDS")?;
        let _ = c.stream.flush();
    }

    Ok(())
}

pub fn sync_room_commands(rooms: &Rooms, clients: &Clients, room_name: &str) -> io::Result<()> {
    let affected_clients = {
        let clients_guard = lock_clients(clients)?;
        let mut list = Vec::new();
        for client_arc in clients_guard.values() {
            if let Ok(target_c) = client_arc.try_lock() {
                if let ClientState::InRoom { username, room, .. } = &target_c.state {
                    if room == room_name {
                        list.push((Arc::clone(client_arc), username.clone(), room.clone()));
                    }
                }
            }
        }
        list
    };

    for (arc, u, r) in affected_clients {
        sync_user_commands(&arc, rooms, &u, &r)?;
    }
    Ok(())
}

pub fn sync_room_members(rooms: &Rooms, clients: &Clients, pubkeys: &PublicKeys, room_name: &str) -> io::Result<()> {
    let (online_users, visibility, user_roles) = {
        let rooms_map = lock_rooms(rooms)?;
        let room_arc = match rooms_map.get(room_name) {
            Some(arc) => Arc::clone(arc),
            None => return Ok(()),
        };
        let room_guard = lock_room(&room_arc)?;
        let mut vis = HashMap::new();
        let mut roles = HashMap::new();
        for uname in &room_guard.online_users {
            if let Some(udata) = room_guard.users.get(uname) {
                vis.insert(uname.clone(), udata.hidden);
                roles.insert(uname.clone(), udata.role.clone());
            }
        }
        (room_guard.online_users.clone(), vis, roles)
    };

    let pkeys = {
        let pk_guard = pubkeys.lock().map_err(|_| io::Error::other("Pubkeys lock poisoned"))?;
        let mut map = HashMap::new();
        for uname in &online_users {
            if let Some(key) = pk_guard.get(uname) {
                map.insert(uname.clone(), key.clone());
            }
        }
        map
    };

    let client_arcs: Vec<Arc<Mutex<Client>>> = {
        let clients_guard = lock_clients(clients)?;
        clients_guard.values().cloned().collect()
    };

    for arc in client_arcs {
        let mut c = lock_client(&arc)?;
        if let ClientState::InRoom { username: recipient, room: rname, .. } = &c.state {
            if rname != room_name {
                continue;
            }

            let role = user_roles.get(recipient).map(|s| s.as_str()).unwrap_or("user");
            let can_see_hidden = role == "owner" || role == "admin";

            let mut pairs = Vec::new();
            for uname in &online_users {
                let is_hidden = visibility.get(uname).cloned().unwrap_or(false);
                if is_hidden && !can_see_hidden {
                    continue;
                }
                
                if let Some(key) = pkeys.get(uname) {
                    pairs.push(format!("{uname}:{key}"));
                }
            }

            if pairs.is_empty() {
                writeln!(c.stream, "/members")?;
            } else {
                writeln!(c.stream, "/members {}", pairs.join(" "))?;
            }
            let _ = c.stream.flush();
        }
    }

    Ok(())
}

pub fn unix_timestamp(rooms: &Rooms, room_name: &str, username: &str) -> io::Result<()> {
    let ts: u64 = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(dur) => dur.as_secs(),
        Err(_)  => 0,
    };

    let rooms_map = lock_rooms(rooms)?;
    if let Some(room_arc) = rooms_map.get(room_name) {
        if let Ok(mut room_guard) = room_arc.lock() {
            if let Some(entry) = room_guard.users.get_mut(username) {
                entry.last_seen = ts;
            }
        }
    }

    save_rooms_to_disk(&rooms_map)?;
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
