use std::collections::{HashSet, HashMap};
use once_cell::sync::Lazy;
use sha2::{Sha256, Digest};
use crate::state::types::{Clients, ClientState};
use colored::Colorize;

pub static DESCRIPTIONS: Lazy<HashMap<&'static str, &'static str>> = Lazy::new(|| {
    let mut map = HashMap::new();
    map.insert("afk",             "> /afk                Set yourself as away");
    map.insert("send",            "> /send <file> <user> Send a file to the room");
    map.insert("msg",             "> /msg <user> <msg>   Send a private message");
    map.insert("me",              "> /me <msg>           Send an emote message");
    map.insert("super",           "> /super              Administrator commands");
    map.insert("super.users",     "> /super users        Show all room user data");
    map.insert("super.reset",     "> /super reset        Reset room back to default");
    map.insert("super.rename",    "> /super rename       Changes room name");
    map.insert("super.export",    "> /super export       Saves room data");
    map.insert("super.whitelist", "> /super whitelist    Manage room whitelist");
    map.insert("super.limit",     "> /super limit        Manage room rate limits");
    map.insert("super.roles",     "> /super roles        Manage room roles and permissions");
    map.insert("user",            "> /user               Manage user settings");
    map.insert("user.list",       "> /user list          Show all visible room users");
    map.insert("user.rename",     "> /user rename        Changes your name in the room");
    map.insert("user.recolor",    "> /user recolor       Changes your name color in the room");
    map.insert("user.ignore",     "> /user ignore        Stops messages from certain users");
    map.insert("user.hide",       "> /user hide          Hides you from /user list");
    map.insert("log",             "> /log                Manage chat logs");
    map.insert("log.list",        "> /log list           Show all available chat logs");
    map.insert("log.save",        "> /log save           Saves chat log of current room session's messages");
    map.insert("log.view",        "> /log view           Updates room session with chat log messages");
    map.insert("mod",             "> /mod                Use chat moderation tools");
    map.insert("mod.kick",        "> /mod kick           Kick users from the chat");
    map.insert("mod.mute",        "> /mod mute           Disable certain users from speaking");
    map.insert("mod.ban",         "> /mod ban            Disable certain users from joining");
    map
});

pub fn command_order() -> Vec<&'static str> {
    vec![
        "help", "clear", "ping", "quit", "leave", "status",
        "afk", "send", "msg", "me",
        "super", "super.users", "super.reset", "super.rename", "super.export", "super.whitelist", "super.limit", "super.roles",
        "user", "user.list", "user.rename", "user.recolor", "user.ignore", "user.hide",
        "log", "log.list", "log.save", "log.view",
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
        "> /status             Show your current room info"
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
> /room               Manage chat rooms"#
}

pub fn help_msg_inroom(extra_cmds: Vec<&str>) -> String {
    let mut shown_cmds: HashSet<String> = HashSet::new();
    let descriptions = DESCRIPTIONS.clone();

    for cmd in extra_cmds {
        shown_cmds.insert(cmd.to_string());
    }

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
