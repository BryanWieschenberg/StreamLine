use colored::*;
use crate::commands::command_utils::{duration_format_passes};

impl ToString for Command {
    fn to_string(&self) -> String {
        match self {
            // Not mapped to a permission since non-room commands are always available
            Command::Help |
            Command::Ping |
            Command::Quit |
            Command::Leave |
            Command::Status |
            Command::IgnoreList |
            Command::IgnoreAdd { .. } |
            Command::IgnoreRemove { .. } => "",
            
            Command::Account |
            Command::AccountRegister { .. } |
            Command::AccountLogin { .. } |
            Command::AccountLogout |
            Command::AccountEditUsername { .. } |
            Command::AccountEditPassword { .. } |
            Command::AccountImport { .. } |
            Command::AccountExport { .. } |
            Command::AccountDelete { .. } => "",
            
            Command::RoomList |
            Command::RoomCreate { .. } |
            Command::RoomJoin { .. } |
            Command::RoomImport { .. } |
            Command::RoomDelete { .. } => "",

            // Permission mappings, since these commands can be enabled/disabled for certain roles
            Command::AFK => "afk",
            Command::DM { .. } => "msg",
            Command::Me { .. } => "me",
            Command::Announce { .. } => "announce",
            Command::Seen { .. } => "seen",

            Command::SuperUsers => "super.users",
            Command::SuperRename { .. } => "super.rename",
            Command::SuperExport { .. } => "super.export",
            Command::SuperWhitelist => "super.whitelist",
            Command::SuperWhitelistToggle => "super.whitelist",
            Command::SuperWhitelistAdd { .. } => "super.whitelist",
            Command::SuperWhitelistRemove { .. } => "super.whitelist",
            Command::SuperLimit => "super.limit",
            Command::SuperLimitRate { .. } => "super.limit",
            Command::SuperLimitSession { .. } => "super.limit",
            Command::SuperRoles => "super.roles",
            Command::SuperRolesAdd { .. } => "super.roles",
            Command::SuperRolesRevoke { .. } => "super.roles",
            Command::SuperRolesAssign { .. } => "super.roles",
            Command::SuperRolesRecolor { .. } => "super.roles",
            
            Command::Users => "user.list",
            Command::UsersRename { .. } => "user.rename",
            Command::UsersRecolor { .. } => "user.color",
            Command::UsersHide => "user.hide",

            Command::ModInfo => "mod.info",
            Command::ModKick { .. } => "mod.kick",
            Command::ModMute { .. } => "mod.mute",
            Command::ModUnmute { .. } => "mod.unmute",
            Command::ModBan { .. } => "mod.ban",
            Command::ModUnban { .. } => "mod.unban",

            Command::InvalidSyntax { .. } | Command::Unavailable => ""
        }.to_string()
    }
}

// #[derive(Debug, Clone)]
pub enum Command {
    Help,
    Ping,
    Quit,
    Leave,
    Status,
    IgnoreList,
    IgnoreAdd { users: String },
    IgnoreRemove { users: String },

    AFK,
    DM { recipient: String, message: String },
    Me { action: String }, //TODO: <- check if ignore works for this
    Announce { message: String }, //TODO:
    Seen { username: String },

    Account,
    AccountRegister { username: String, password: String, confirm: String },
    AccountLogin { username: String, password: String },
    AccountLogout,
    AccountEditUsername { username: String },
    AccountEditPassword { current_password: String, new_password: String },
    AccountImport { filename: String },
    AccountExport { filename: String },
    AccountDelete { force: bool },

    RoomList,
    RoomCreate { name: String, whitelist: bool },
    RoomJoin { name: String },
    RoomImport { filename: String },
    RoomDelete { name: String, force: bool },

    SuperUsers,
    SuperRename { name: String },
    SuperExport { filename: String },
    SuperWhitelist,
    SuperWhitelistToggle,
    SuperWhitelistAdd { users: String },
    SuperWhitelistRemove { users: String },
    SuperLimit,
    SuperLimitRate { limit: u8 },
    SuperLimitSession { limit: u32 },
    SuperRoles,
    SuperRolesAdd { role: String, commands: String },
    SuperRolesRevoke { role: String, commands: String },
    SuperRolesAssign { role: String, users: String },
    SuperRolesRecolor { role: String, color: String },

    Users,
    UsersRename { name: String },
    UsersRecolor { color: String },
    UsersHide,

    ModInfo,
    ModKick { username: String, reason: String }, //TODO:
    ModMute { username: String, duration: String, reason: String }, //TODO:
    ModUnmute { username: String }, //TODO:
    ModBan { username: String, duration: String, reason: String }, //TODO:
    ModUnban { username: String }, //TODO:

    InvalidSyntax { err_msg: String },
    Unavailable
}

pub fn parse_command(input: &str) -> Command {
    let mut tokens: Vec<&str> = input.trim().split_whitespace().collect();
    tokens[0] = &tokens[0][1..];

    match tokens.as_slice() {
        ["help"] | ["h"] => Command::Help,
        ["ping"] => Command::Ping,
        ["quit"] | ["exit"] | ["q"] | ["e"] => Command::Quit,
        ["leave"] => Command::Leave,
        ["status"] => Command::Status,

        ["ignore", "list"] |
        ["ignore", "l"] |
        ["i", "list"] |
        ["i", "l"] => Command::IgnoreList,

        ["ignore", "list", ..] |
        ["ignore", "l", ..] |
        ["i", "list", ..] |
        ["i", "l", ..] => {
            let err_msg = format!("{}", "Usage: /ignore list".bright_blue());
            Command::InvalidSyntax { err_msg }
        },

        ["ignore", "add", users @ ..] |
        ["ignore", "a", users @ ..] |
        ["i", "add", users @ ..] |
        ["i", "a", users @ ..] if !users.is_empty() => Command::IgnoreAdd {
            users: users.join(" ")
        },

        ["ignore", "add", ..] |
        ["ignore", "a", ..] |
        ["i", "add", ..] |
        ["i", "a", ..] => {
            let err_msg = format!("{}", "Usage: /ignore add <user1> <user2> ...".bright_blue());
            Command::InvalidSyntax { err_msg }
        },

        ["ignore", "remove", users @ ..] |
        ["ignore", "r", users @ ..] |
        ["i", "remove", users @ ..] |
        ["i", "r", users @ ..] if !users.is_empty() => Command::IgnoreRemove {
            users: users.join(" ")
        },

        ["ignore", "remove", ..] |
        ["ignore", "r", ..] |
        ["i", "remove", ..] |
        ["i", "r", ..] => {
            let err_msg = format!("{}", "Usage: /ignore remove <user1> <user2> ...".bright_blue());
            Command::InvalidSyntax { err_msg }
        },

        ["ignore", ..] |
        ["i", ..] => {
            let err_msg = format!("{}", "Ignore commands:\n> /ignore list\n> /ignore add <user1> <user2> ...\n> /ignore remove <user1> <user2> ...".bright_blue());
            Command::InvalidSyntax { err_msg }
        },

        ["afk"] => Command::AFK,

        ["message", recipient, message @ ..] |
        ["msg", recipient, message @ ..] |
        ["dm", recipient, message @ ..] if !message.is_empty() => Command::DM {
            recipient: recipient.to_string(),
            message: message.join(" ")
        },

        ["message", ..] |
        ["msg", ..] |
        ["dm", ..] => {
            let err_msg = format!("{}", "Usage: /message <recipient> <message>".bright_blue());
            Command::InvalidSyntax { err_msg }
        },

        ["me", action @ ..] if !action.is_empty() => Command::Me {
            action: action.join(" ")
        },

        ["me", ..] => {
            let err_msg = format!("{}", "Usage: /me <action>".bright_blue());
            Command::InvalidSyntax { err_msg }
        },

        ["seen", username] => Command::Seen {
            username: username.to_string()
        },

        ["seen", ..] => {
            let err_msg = format!("{}", "Usage: /seen <username>".bright_blue());
            Command::InvalidSyntax { err_msg }
        },

        ["announce", message @ ..] if !message.is_empty() => Command::Announce {
            message: message.join(" ")
        },

        ["announce", ..] => {
            let err_msg = format!("{}", "Usage: /announce <message>".bright_blue());
            Command::InvalidSyntax { err_msg }
        },

        ["account", "register", username, password, confirm_password] |
        ["a", "register", username, password, confirm_password] |
        ["account", "r", username, password, confirm_password] |
        ["a", "r", username, password, confirm_password] => Command::AccountRegister {
            username: username.to_string(),
            password: password.to_string(),
            confirm: confirm_password.to_string()
        },

        ["account", "register", ..] |
        ["a", "register", ..] |
        ["account", "r", ..] |
        ["a", "r", ..] => {
            let err_msg = format!("{}", "Usage: /account register <username> <password> <password confirm>".bright_blue());
            Command::InvalidSyntax { err_msg }
        },

        ["account", "login", u, p] |
        ["a", "login", u, p] |
        ["account", "l", u, p] |
        ["a", "l", u, p] => Command::AccountLogin {
            username: u.to_string(),
            password: p.to_string(),
        },

        ["account", "login", ..] |
        ["a", "login", ..] |
        ["account", "l", ..] |
        ["a", "l", ..] => {
            let err_msg = format!("{}", "Usage: /account login <username> <password>".bright_blue());
            Command::InvalidSyntax { err_msg }
        },

        ["account", "logout"] |
        ["a", "logout"] => Command::AccountLogout {},

        ["account", "logout", ..] |
        ["a", "logout", ..] => {
            let err_msg = format!("{}", "Usage: /account logout".bright_blue());
            Command::InvalidSyntax { err_msg }
        },

        ["account", "edit", "username", username] |
        ["a", "edit", "username", username] |
        ["account", "e", "username", username] |
        ["account", "edit", "u", username] |
        ["a", "e", "username", username] |
        ["a", "edit", "u", username] |
        ["account", "e", "u", username] |
        ["a", "e", "u", username] => Command::AccountEditUsername {
            username: username.to_string()
        },

        ["account", "edit", "username", ..] |
        ["a", "edit", "username", ..] |
        ["account", "e", "username", ..] |
        ["a", "e", "username", ..] |
        ["account", "edit", "u", ..] |
        ["a", "edit", "u", ..] |
        ["account", "e", "u", ..] |
        ["a", "e", "u", ..] => {
            let err_msg = format!("{}", "Usage: /account edit username <new username>".bright_blue());
            Command::InvalidSyntax { err_msg }
        },

        ["account", "edit", "password", current_password, new_password] |
        ["a", "edit", "password", current_password, new_password] |
        ["account", "e", "password", current_password, new_password] |
        ["account", "edit", "p", current_password, new_password] |
        ["a", "e", "password", current_password, new_password] |
        ["a", "edit", "p", current_password, new_password] |
        ["account", "e", "p", current_password, new_password] |
        ["a", "e", "p", current_password, new_password] => Command::AccountEditPassword {
            current_password: current_password.to_string(),
            new_password: new_password.to_string()
        },

        ["account", "edit", "password", ..] |
        ["a", "edit", "password", ..] |
        ["account", "e", "password", ..] |
        ["a", "e", "password", ..] |
        ["account", "edit", "p", ..] |
        ["a", "edit", "p", ..] |
        ["account", "e", "p", ..] |
        ["a", "e", "p", ..] => {
            let err_msg = format!("{}", "Usage: /account edit password <current password> <new password>".bright_blue());
            Command::InvalidSyntax { err_msg }
        },

        ["account", "edit", ..] |
        ["a", "edit", ..] |
        ["account", "e", ..] |
        ["a", "e", ..] => {
            let err_msg = format!("{}", "Account edit commands:\n> /account edit username <new username>\n> /account edit password <current password> <new password>".bright_blue());
            Command::InvalidSyntax { err_msg }
        },

        ["account", "import", filename] |
        ["a", "import", filename] => Command::AccountImport {
            filename: filename.to_string()
        },

        ["account", "import", ..] |
        ["a", "import", ..] => {
            let err_msg = format!("{}", "Usage: /account import <filename>".bright_blue());
            Command::InvalidSyntax { err_msg }
        },

        ["account", "export"] |
        ["a", "export"] => Command::AccountExport {
            filename: "".to_string()
        },

        ["account", "export", filename] |
        ["a", "export", filename] => Command::AccountExport {
            filename: filename.to_string()
        },

        ["account", "export", ..] |
        ["a", "export", ..] => {
            let err_msg = format!("{}", "Usage: /account export <filename>?".bright_blue());
            Command::InvalidSyntax { err_msg }
        },

        ["account", "delete"] |
        ["a", "delete"] | 
        ["account", "d"] |
        ["a", "d"] => Command::AccountDelete { force: false },

        ["account", "delete", "force"] |
        ["a", "delete", "force"] |
        ["account", "d", "force"] |
        ["account", "delete", "f"] |
        ["a", "d", "force"] |
        ["a", "delete", "f"] |
        ["account", "d", "f"] |
        ["a", "d", "f"] => Command::AccountDelete { force: true },

        ["account", "delete", ..] |
        ["a", "delete", ..] |
        ["account", "d", ..] |
        ["a", "d", ..] => {
            let err_msg = format!("{}", "Usage: /account delete force?".bright_blue());
            Command::InvalidSyntax { err_msg }
        },

        ["account", "info"] |
        ["a", "info"] |
        ["account", "i"] |
        ["a", "i"] => Command::Account,

        ["account", "info", ..] |
        ["a", "info", ..] |
        ["account", "i", ..] |
        ["a", "i", ..] => {
            let err_msg = format!("{}", "Usage: /account info".bright_blue());
            Command::InvalidSyntax { err_msg }
        }

        ["account", ..] |
        ["a", ..] => {
            let err_msg = format!("{}", "Account commands:\n> /account info\n> /account register <username> <password> <password confirm>\n> /account login <username> <password>\n> /account logout\n> /account edit\n> /account import <filename>\n> /account export <filename>?\n> /account delete force?".bright_blue());
            Command::InvalidSyntax { err_msg }
        }

        ["room", "create", name] |
        ["r", "create", name] |
        ["room", "c", name] |
        ["r", "c", name] => Command::RoomCreate {
            name: name.to_string(),
            whitelist: false
        },

        ["room", "create", name, "whitelist"] |
        ["r", "create", name, "whitelist"] |
        ["room", "c", name, "whitelist"] |
        ["r", "c", name, "whitelist"] |
        ["room", "create", name, "w"] |
        ["r", "create", name, "w"] |
        ["room", "c", name, "w"] |
        ["r", "c", name, "w"] |
        ["room", "create", name, "private"] |
        ["r", "create", name, "private"] |
        ["room", "c", name, "private"] |
        ["r", "c", name, "private"] |
        ["room", "create", name, "p"] |
        ["r", "create", name, "p"] |
        ["room", "c", name, "p"] |
        ["r", "c", name, "p"] => Command::RoomCreate {
            name: name.to_string(),
            whitelist: true
        },

        ["room", "create", ..] |
        ["r", "create", ..] |
        ["room", "c", ..] |
        ["r", "c", ..] => {
            let err_msg = format!("{}", "Usage: /room create <room name> whitelist?".bright_blue());
            Command::InvalidSyntax { err_msg }
        },

        ["room", "join", name] |
        ["r", "join", name] |
        ["room", "j", name] |
        ["r", "j", name] => Command::RoomJoin {
            name: name.to_string(),
        },

        ["room", "join", ..] |
        ["r", "join", ..] |
        ["room", "j", ..] |
        ["r", "j", ..] => {
            let err_msg = format!("{}", "Usage: /room join <room name>".bright_blue());
            Command::InvalidSyntax { err_msg }
        },

        ["room", "import", filename] |
        ["r", "import", filename] => Command::RoomImport {
            filename: filename.to_string()
        },

        ["room", "import", ..] |
        ["r", "import", ..] => {
            let err_msg = format!("{}", "Usage: /room import <filename>".bright_blue());
            Command::InvalidSyntax { err_msg }
        },

        ["room", "delete", name] |
        ["r", "delete", name] | 
        ["room", "d", name] |
        ["r", "d", name] => Command::RoomDelete{
            name: name.to_string(),
            force: false
        },

        ["room", "delete", "force", name] |
        ["r", "delete", "force", name] |
        ["room", "d", "force", name] |
        ["room", "delete", "f", name] |
        ["r", "d", "force", name] |
        ["r", "delete", "f", name] |
        ["room", "d", "f", name] |
        ["r", "d", "f", name] => Command::RoomDelete {
            name: name.to_string(),
            force: true
        },

        ["room", "delete", ..] |
        ["r", "delete", ..] |
        ["room", "d", ..] |
        ["r", "d", ..] => {
            let err_msg = format!("{}", "Usage: /room delete <room name> force?".bright_blue());
            Command::InvalidSyntax { err_msg }
        },

        ["room", "list"] |
        ["r", "list"] |
        ["room", "l"] |
        ["r", "l"] => Command::RoomList,

        ["room", "list", ..] |
        ["r", "list", ..] |
        ["room", "l", ..] |
        ["r", "l", ..] => {
            let err_msg = format!("{}", "Usage: /room list".bright_blue());
            Command::InvalidSyntax { err_msg }
        },

        ["room", ..] |
        ["r", ..] => {
            let err_msg = format!("{}", "Room commands:\n> /room list\n> /room create <room name> whitelist?\n> /room join <room name>\n> /room import <filename>\n> /room delete <room name> force?".bright_blue());
            Command::InvalidSyntax { err_msg }
        },

        ["super", "users"] |
        ["s", "users"] |
        ["super", "u"] |
        ["s", "u"] => Command::SuperUsers,

        ["super", "users", ..] |
        ["s", "users", ..] |
        ["super", "u", ..] |
        ["s", "u", ..] => {
            let err_msg = format!("{}", "Usage: /super users".bright_blue());
            Command::InvalidSyntax { err_msg }
        },

        ["super", "rename", name] |
        ["s", "rename", name] |
        ["super", "rn", name] |
        ["s", "rn", name] => Command::SuperRename {
            name: name.to_string()
        },

        ["super", "rename", ..] |
        ["s", "rename", ..] |
        ["super", "rn", ..] |
        ["s", "rn", ..] => {
            let err_msg = format!("{}", "Usage: /super rename <new room name>".bright_blue());
            Command::InvalidSyntax { err_msg }
        },

        ["super", "export"] |
        ["s", "export"] => Command::SuperExport {
            filename: "".to_string()
        },

        ["super", "export", filename] |
        ["s", "export", filename] => Command::SuperExport {
            filename: filename.to_string()
        },

        ["super", "export", ..] |
        ["s", "export", ..] => {
            let err_msg = format!("{}", "Usage: /super export <filename>?".bright_blue());
            Command::InvalidSyntax { err_msg }
        },

        ["super", "whitelist", "info"] |
        ["super", "wl", "info"] |
        ["s", "whitelist", "info"] |
        ["s", "wl", "info"] |
        ["super", "whitelist", "i"] |
        ["super", "wl", "i"] |
        ["s", "whitelist", "i"] |
        ["s", "wl", "i"] => Command::SuperWhitelist,

        ["super", "whitelist", "toggle"] |
        ["super", "wl", "toggle"] |
        ["s", "whitelist", "toggle"] |
        ["s", "wl", "toggle"] |
        ["super", "whitelist", "t"] |
        ["super", "wl", "t"] |
        ["s", "whitelist", "t"] |
        ["s", "wl", "t"] => Command::SuperWhitelistToggle,

        ["super", "whitelist", "add", users @ ..] |
        ["super", "wl", "add", users @ ..] |
        ["s", "whitelist", "add", users @ ..] |
        ["s", "wl", "add", users @ ..] |
        ["super", "whitelist", "a", users @ ..] |
        ["super", "wl", "a", users @ ..] |
        ["s", "whitelist", "a", users @ ..] |
        ["s", "wl", "a", users @ ..] if !users.is_empty() => Command::SuperWhitelistAdd {
            users: users.join(" "),
        },

        ["super", "whitelist", "add", ..] |
        ["super", "wl", "add", ..] |
        ["s", "whitelist", "add", ..] |
        ["s", "wl", "add", ..] |
        ["super", "whitelist", "a", ..] |
        ["super", "wl", "a", ..] |
        ["s", "whitelist", "a", ..] |
        ["s", "wl", "a", ..] => {
            let err_msg = format!("{}", "Usage: /super whitelist add <user1> <user2> ...".bright_blue());
            Command::InvalidSyntax { err_msg }
        },

        ["super", "whitelist", "remove", users @ ..] |
        ["super", "wl", "remove", users @ ..] |
        ["s", "whitelist", "remove", users @ ..] |
        ["s", "wl", "remove", users @ ..] |
        ["super", "whitelist", "r", users @ ..] |
        ["super", "wl", "r", users @ ..] |
        ["s", "whitelist", "r", users @ ..] |
        ["s", "wl", "r", users @ ..] if !users.is_empty() => Command::SuperWhitelistRemove {
            users: users.join(" "),
        },

        ["super", "whitelist", "remove", ..] |
        ["super", "wl", "remove", ..] |
        ["s", "whitelist", "remove", ..] |
        ["s", "wl", "remove", ..] |
        ["super", "whitelist", "r", ..] |
        ["super", "wl", "r", ..] |
        ["s", "whitelist", "r", ..] |
        ["s", "wl", "r", ..] => {
            let err_msg = format!("{}", "Usage: /super whitelist remove <user1> <user2> ...".bright_blue());
            Command::InvalidSyntax { err_msg }
        },

        ["super", "whitelist", ..] |
        ["super", "wl", ..] |
        ["s", "whitelist", ..] |
        ["s", "wl", ..] => {
            let err_msg = format!("{}", "Super whitelist commands:\n> /super whitelist info\n> /super whitelist toggle\n> /super whitelist add <user1> <user2> ...\n> /super whitelist remove <user1> <user2> ...".bright_blue());
            Command::InvalidSyntax { err_msg }
        },

        ["super", "limit", "info"] |
        ["s", "limit", "info"] |
        ["super", "l", "info"] |
        ["s", "l", "info"] |
        ["super", "limit", "i"] |
        ["s", "limit", "i"] |
        ["super", "l", "i"] |
        ["s", "l", "i"] => Command::SuperLimit,

        ["super", "limit", "info", ..] |
        ["s", "limit", "info", ..] |
        ["super", "l", "info", ..] |
        ["s", "l", "info", ..] |
        ["super", "limit", "i", ..] |
        ["s", "limit", "i", ..] |
        ["super", "l", "i", ..] |
        ["s", "l", "i", ..] => {
            let err_msg = format!("{}", "Usage:\n> /super limit info".bright_blue());
            Command::InvalidSyntax { err_msg }
        },

        ["super", "limit", "rate", limit] |
        ["s", "limit", "rate", limit] |
        ["super", "l", "rate", limit] |
        ["s", "l", "rate", limit] |
        ["super", "limit", "r", limit] |
        ["s", "limit", "r", limit] |
        ["super", "l", "r", limit] |
        ["s", "l", "r", limit] => {
            if *limit == "*" { // * means unlimited, msg_rate is set to 0 as special case
                Command::SuperLimitRate { limit: 0 }
            }
            else {
                match limit.parse::<u8>() {
                    Ok(l) if l > 0 => Command::SuperLimitRate { limit: l },
                    _ => Command::InvalidSyntax {
                        err_msg: format!("{}", "Usage: /super limit rate <limit secs (1‑255) | *>".bright_blue())
                    }
                }
            }
        },

        ["super", "limit", "rate", ..] |
        ["s", "limit", "rate", ..] |
        ["super", "l", "rate", ..] |
        ["s", "l", "rate", ..] |
        ["super", "limit", "r", ..] |
        ["s", "limit", "r", ..] |
        ["super", "l", "r", ..] |
        ["s", "l", "r", ..] => {
            let err_msg = format!("{}", "Usage: /super limit rate <limit secs (1-255) | *>".bright_blue());
            Command::InvalidSyntax { err_msg }
        },

        ["super", "limit", "session", limit] |
        ["s", "limit", "session", limit] |
        ["super", "l", "session", limit] |
        ["s", "l", "session", limit] |
        ["super", "limit", "s", limit] |
        ["s", "limit", "s", limit] |
        ["super", "l", "s", limit] |
        ["s", "l", "s", limit] => {
            if *limit == "*" { // * means unlimited, msg_rate is set to 0 as special case
                Command::SuperLimitSession { limit: 0 }
            }
            else {
                match limit.parse::<u32>() {
                    Ok(l) if l > 0 => Command::SuperLimitSession { limit: l },
                    _ => {
                        let err_msg = format!("{}", "Usage: /super limit session <limit secs (1-4294967295) | *>".bright_blue());
                        Command::InvalidSyntax { err_msg }
                    }
                }
            }
        },

        ["super", "limit", "session", ..] |
        ["s", "limit", "session", ..] |
        ["super", "l", "session", ..] |
        ["s", "l", "session", ..] |
        ["super", "limit", "s", ..] |
        ["s", "limit", "s", ..] |
        ["super", "l", "s", ..] |
        ["s", "l", "s", ..] => {
            let err_msg = format!("{}", "Usage: /super limit session <limit secs (1-4294967295) | *>".bright_blue());
            Command::InvalidSyntax { err_msg }
        },

        ["super", "limit", ..] |
        ["s", "limit", ..] |
        ["super", "l", ..] |
        ["s", "l", ..] => {
            let err_msg = format!("{}", "Super limit commands:\n> /super limit info\n> /super limit rate <limit secs (1-255) | *>\n> /super limit session <limit secs (1-4294967295) | *>".bright_blue());
            Command::InvalidSyntax { err_msg }
        },

        ["super", "roles", "list"] |
        ["super", "r", "list"] |
        ["s", "roles", "list"] |
        ["s", "r", "list"] |
        ["super", "roles", "l"] |
        ["super", "r", "l"] |
        ["s", "roles", "l"] |
        ["s", "r", "l"] => Command::SuperRoles,

        ["super", "roles", "list", ..] |
        ["super", "r", "list", ..] |
        ["s", "roles", "list", ..] |
        ["s", "r", "list", ..] |
        ["super", "roles", "l", ..] |
        ["super", "r", "l", ..] |
        ["s", "roles", "l", ..] |
        ["s", "r", "l", ..] => {
            let err_msg = format!("{}", "Usage:\n> /super roles list".bright_blue());
            Command::InvalidSyntax { err_msg }
        },

        ["super", "roles", "add", role, commands @ ..] |
        ["super", "r", "add", role, commands @ ..] |
        ["s", "roles", "add", role, commands @ ..] |
        ["s", "r", "add", role, commands @ ..] |
        ["super", "roles", "a", role, commands @ ..] |
        ["super", "r", "a", role, commands @ ..] |
        ["s", "roles", "a", role, commands @ ..] |
        ["s", "r", "a", role, commands @ ..] if !commands.is_empty() => Command::SuperRolesAdd {
            role: role.to_string(),
            commands: commands.join(" ")
        },

        ["super", "roles", "add", ..] |
        ["super", "r", "add", ..] |
        ["s", "roles", "add", ..] |
        ["s", "r", "add", ..] |
        ["super", "roles", "a", ..] |
        ["super", "r", "a", ..] |
        ["s", "roles", "a", ..] |
        ["s", "r", "a", ..] => {
            let err_msg = format!("{}", "Usage: /super roles add <user|mod> <command1> <command2> ...".bright_blue());
            Command::InvalidSyntax { err_msg }
        },

        ["super", "roles", "revoke", role, commands @ ..] |
        ["super", "r", "revoke", role, commands @ ..] |
        ["s", "roles", "revoke", role, commands @ ..] |
        ["s", "r", "revoke", role, commands @ ..] |
        ["super", "roles", "r", role, commands @ ..] |
        ["super", "r", "r", role, commands @ ..] |
        ["s", "roles", "r", role, commands @ ..] |
        ["s", "r", "r", role, commands @ ..] if !commands.is_empty() => Command::SuperRolesRevoke {
            role: role.to_string(),
            commands: commands.join(" ")
        },

        ["super", "roles", "revoke", ..] |
        ["super", "r", "revoke", ..] |
        ["s", "roles", "revoke", ..] |
        ["s", "r", "revoke", ..] |
        ["super", "roles", "r", ..] |
        ["super", "r", "r", ..] |
        ["s", "roles", "r", ..] |
        ["s", "r", "r", ..] => {
            let err_msg = format!("{}", "Usage: /super roles revoke <user|mod> <command1> <command2> ...".bright_blue());
            Command::InvalidSyntax { err_msg }
        },

        ["super", "roles", "assign", role, users @ ..] |
        ["super", "r", "assign", role, users @ ..] |
        ["s", "roles", "assign", role, users @ ..] |
        ["s", "r", "assign", role, users @ ..] |
        ["super", "roles", "as", role, users @ ..] |
        ["super", "r", "as", role, users @ ..] |
        ["s", "roles", "as", role, users @ ..] |
        ["s", "r", "as", role, users @ ..] if !users.is_empty() => Command::SuperRolesAssign {
            role: role.to_string(),
            users: users.join(" ")
        },

        ["super", "roles", "assign", ..] |
        ["super", "r", "assign", ..] |
        ["s", "roles", "assign", ..] |
        ["s", "r", "assign", ..] |
        ["super", "roles", "as", ..] |
        ["super", "r", "as", ..] |
        ["s", "roles", "as", ..] |
        ["s", "r", "as", ..] => {
            let err_msg = format!("{}", "Usage: /super roles assign <user|mod|admin|owner> <user1> <user2> ...".bright_blue());
            Command::InvalidSyntax { err_msg }
        },

        ["super", "roles", "recolor", role, color] |
        ["super", "r", "recolor", role, color] |
        ["s", "roles", "recolor", role, color] |
        ["s", "r", "recolor", role, color] |
        ["super", "roles", "rc", role, color] |
        ["super", "r", "rc", role, color] |
        ["s", "roles", "rc", role, color] |
        ["s", "r", "rc", role, color] => Command::SuperRolesRecolor {
            role: role.to_string(),
            color: color.to_string()
        },

        ["super", "roles", "recolor", ..] |
        ["super", "r", "recolor", ..] |
        ["s", "roles", "recolor", ..] |
        ["s", "r", "recolor", ..] |
        ["super", "roles", "rc", ..] |
        ["super", "r", "rc", ..] |
        ["s", "roles", "rc", ..] |
        ["s", "r", "rc", ..] => {
            let err_msg = format!("{}", "Usage: /super roles recolor <role> <color hex>".bright_blue());
            Command::InvalidSyntax { err_msg }
        },

        ["super", "roles", ..] |
        ["super", "r", ..] |
        ["s", "roles", ..] |
        ["s", "r", ..] => {
            let err_msg = format!("{}", "Super roles commands:\n> /super roles list\n> /super roles add <user|mod> <command1> <command2> ...\n> /super roles revoke <user|mod> <command1> <command2> ...\n> /super roles assign <user|mod|admin|owner> <user1> <user2> ...\n> /super roles recolor <user|mod|admin|owner> <color>".bright_blue());
            Command::InvalidSyntax { err_msg }
        },

        ["super", ..] |
        ["s", ..] => {
            let err_msg = format!("{}", "Super commands:\n> /super users\n> /super rename <new room name>\n> /super export <filename>?\n> /super whitelist\n> /super limit\n> /super roles".bright_blue());
            Command::InvalidSyntax { err_msg }
        },

        ["user", "list"] |
        ["u", "list"] |
        ["user", "l"] |
        ["u", "l"] => Command::Users,

        ["user", "list", ..] |
        ["u", "list", ..] |
        ["user", "l", ..] |
        ["u", "l", ..] => {
            let err_msg = format!("{}", "Usage: /user list".bright_blue());
            Command::InvalidSyntax { err_msg }
        }

        ["user", "rename", name] |
        ["u", "rename", name] |
        ["user", "rn", name] |
        ["u", "rn", name] => Command::UsersRename {
            name: name.to_string()
        },

        ["user", "rename", ..] |
        ["u", "rename", ..] |
        ["user", "rn", ..] |
        ["u", "rn", ..] => {
            let err_msg = format!("{}", "Usage: /user rename <new name|*>".bright_blue());
            Command::InvalidSyntax { err_msg }
        },

        ["user", "recolor", color] |
        ["u", "recolor", color] |
        ["user", "rc", color] |
        ["u", "rc", color] => Command::UsersRecolor {
            color: color.to_string()
        },

        ["user", "recolor", ..] |
        ["u", "recolor", ..] |
        ["user", "rc", ..] |
        ["u", "rc", ..] => {
            let err_msg = format!("{}", "Usage: /user recolor <color hex|*>".bright_blue());
            Command::InvalidSyntax { err_msg }
        },

        ["user", "hide"] |
        ["u", "hide"] |
        ["user", "h"] |
        ["u", "h"] => Command::UsersHide,

        ["user", "hide", ..] |
        ["u", "hide", ..] => {
            let err_msg = format!("{}", "Usage: /user hide".bright_blue());
            Command::InvalidSyntax { err_msg }
        },

        ["user", ..] |
        ["u", ..] => {
            let err_msg = format!("{}", "User commands:\n> /user list\n> /user rename <new name|*>\n> /user recolor <color hex|*>\n> /user hide".bright_blue());
            Command::InvalidSyntax { err_msg }
        }

        ["mod", "info"] |
        ["m", "info"] |
        ["mod", "i"] |
        ["m", "i"] => Command::ModInfo,

        ["mod", "info", ..] |
        ["m", "info", ..] |
        ["mod", "i", ..] |
        ["m", "i", ..] => {
            let err_msg = format!("{}", "Usage: /mod info".bright_blue());
            Command::InvalidSyntax { err_msg }
        },

        ["mod", "kick", username] |
        ["m", "kick", username] |
        ["mod", "k", username] |
        ["m", "k", username] => Command::ModKick {
            username: username.to_string(),
            reason: "".to_string()
        },

        ["mod", "kick", username, reason @ ..] |
        ["m", "kick", username, reason @ ..] |
        ["mod", "k", username, reason @ ..] |
        ["m", "k", username, reason @ ..] if !reason.is_empty() => Command::ModKick {
            username: username.to_string(),
            reason: reason.join(" ")
        },

        ["mod", "kick", ..] |
        ["m", "kick", ..] |
        ["mod", "k", ..] |
        ["m", "k", ..] => {
            let err_msg = format!("{}", "Usage: /mod kick <username> <reason>?".bright_blue());
            Command::InvalidSyntax { err_msg }
        },

        ["mod", "ban", username] |
        ["m", "ban", username] |
        ["mod", "b", username] |
        ["m", "b", username] => Command::ModBan {
            username: username.to_string(),
            duration: "*".to_string(),
            reason: "".to_string()
        },

        ["mod", "ban", username, duration] |
        ["m", "ban", username, duration] |
        ["mod", "b", username, duration] |
        ["m", "b", username, duration] if duration_format_passes(duration) => Command::ModBan {
            username: username.to_string(),
            duration: duration.to_string(),
            reason: "".to_string()
        },

        ["mod", "ban", username, duration, reason @ ..] |
        ["m", "ban", username, duration, reason @ ..] |
        ["mod", "b", username, duration, reason @ ..] |
        ["m", "b", username, duration, reason @ ..] if duration_format_passes(duration) => Command::ModBan {
            username: username.to_string(),
            duration: duration.to_string(),
            reason: reason.join(" ")
        },

        ["mod", "ban", ..] |
        ["m", "ban", ..] |
        ["mod", "b", ..] |
        ["m", "b", ..] => {
            let err_msg = format!("{}", "Usage: /mod ban <username> <_d_h_m_s|*>? <reason>?".bright_blue());
            Command::InvalidSyntax { err_msg }
        },

        ["mod", "unban", username] |
        ["m", "unban", username] |
        ["mod", "ub", username] |
        ["m", "ub", username] => Command::ModUnban {
            username: username.to_string()
        },

        ["mod", "unban", ..] |
        ["m", "unban", ..] |
        ["mod", "ub", ..] |
        ["m", "ub", ..] => {
            let err_msg = format!("{}", "Usage: /mod unban <username>".bright_blue());
            Command::InvalidSyntax { err_msg }
        },

        ["mod", "mute", username] |
        ["m", "mute", username] |
        ["mod", "m", username] |
        ["m", "m", username] => Command::ModMute {
            username: username.to_string(),
            duration: "*".to_string(),
            reason: "".to_string()
        },

        ["mod", "mute", username, duration] |
        ["m", "mute", username, duration] |
        ["mod", "m", username, duration] |
        ["m", "m", username, duration] if duration_format_passes(duration) => Command::ModMute {
            username: username.to_string(),
            duration: duration.to_string(),
            reason: "".to_string()
        },

        ["mod", "mute", username, duration, reason @ ..] |
        ["m", "mute", username, duration, reason @ ..] |
        ["mod", "m", username, duration, reason @ ..] |
        ["m", "m", username, duration, reason @ ..] if duration_format_passes(duration) => Command::ModMute {
            username: username.to_string(),
            duration: duration.to_string(),
            reason: reason.join(" ")
        },

        ["mod", "mute", ..] |
        ["m", "mute", ..] |
        ["mod", "m", ..] |
        ["m", "m", ..] => {
            let err_msg = format!("{}", "Usage: /mod mute <username> <_d_h_m_s|*>? <reason>?".bright_blue());
            Command::InvalidSyntax { err_msg }
        },

        ["mod", "unmute", username] |
        ["m", "unmute", username] |
        ["mod", "um", username] |
        ["m", "um", username] => Command::ModUnmute {
            username: username.to_string()
        },

        ["mod", "unmute", ..] |
        ["m", "unmute", ..] |
        ["mod", "um", ..] |
        ["m", "um", ..] => {
            let err_msg = format!("{}", "Usage: /mod unmute <username>".bright_blue());
            Command::InvalidSyntax { err_msg }
        },

        ["mod", ..] |
        ["m", ..] => {
            let err_msg = format!("{}", "Mod commands:\n> /mod info\n> /mod kick <username> <reason>?\n> /mod ban <username> <_d_h_m_s|*>? <reason>?\n> /mod unban <username>\n> /mod mute <username> <_d_h_m_s|*>? <reason>?\n> /mod unmute <username>".bright_blue());
            Command::InvalidSyntax { err_msg }
        }

        _ => Command::Unavailable
    }
}
