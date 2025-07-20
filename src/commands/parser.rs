use colored::*;

#[derive(Debug, Clone)]
pub enum Command {
    Help,
    Ping,
    Quit,

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
    
    InvalidSyntax { err_msg: String },
    Unavailable
}

pub fn parse_command(input: &str) -> Command {
    let tokens: Vec<&str> = input.trim().split_whitespace().collect();

    match tokens.as_slice() {
        ["/help"] | ["/h"] => Command::Help,
        ["/ping"] => Command::Ping,
        ["/quit"] | ["/exit"] | ["/q"] | ["/e"] => Command::Quit,

        ["/account", "register", username, password, confirm_password] |
        ["/a", "register", username, password, confirm_password] |
        ["/account", "r", username, password, confirm_password] |
        ["/a", "r", username, password, confirm_password] => Command::AccountRegister {
            username: username.to_string(),
            password: password.to_string(),
            confirm: confirm_password.to_string()
        },

        ["/account", "register", ..] |
        ["/a", "register", ..] |
        ["/account", "r", ..] |
        ["/a", "r", ..] => {
            let err_msg = format!("{}", "Usage: /account register <username> <password> <password confirm>".yellow());
            Command::InvalidSyntax { err_msg }
        },

        ["/account", "login", u, p] |
        ["/a", "login", u, p] |
        ["/account", "l", u, p] |
        ["/a", "l", u, p] => Command::AccountLogin {
            username: u.to_string(),
            password: p.to_string(),
        },

        ["/account", "login", ..] |
        ["/a", "login", ..] |
        ["/account", "l", ..] |
        ["/a", "l", ..] => {
            let err_msg = format!("{}", "Usage: /account login <username> <password>".yellow());
            Command::InvalidSyntax { err_msg }
        },

        ["/account", "logout"] |
        ["/a", "logout"] => Command::AccountLogout {},

        ["/account", "edit", "username", username] |
        ["/a", "edit", "username", username] |
        ["/account", "e", "username", username] |
        ["/account", "edit", "u", username] |
        ["/a", "e", "username", username] |
        ["/a", "edit", "u", username] |
        ["/account", "e", "u", username] |
        ["/a", "e", "u", username] => Command::AccountEditUsername {
            username: username.to_string()
        },

        ["/account", "edit", "password", current_password, new_password] |
        ["/a", "edit", "password", current_password, new_password] |
        ["/account", "e", "password", current_password, new_password] |
        ["/account", "edit", "p", current_password, new_password] |
        ["/a", "e", "password", current_password, new_password] |
        ["/a", "edit", "p", current_password, new_password] |
        ["/account", "e", "p", current_password, new_password] |
        ["/a", "e", "p", current_password, new_password] => Command::AccountEditPassword {
            current_password: current_password.to_string(),
            new_password: new_password.to_string()
        },

        ["/account", "edit", ..] |
        ["/a", "edit", ..] |
        ["/account", "e", ..] |
        ["/a", "e", ..] => {
            let err_msg = format!("{}", "Usage: /account edit username <new username> or /account edit password <current password> <new password>".yellow());
            Command::InvalidSyntax { err_msg }
        },

        ["/account", "import", filename] |
        ["/a", "import", filename] => Command::AccountImport {
            filename: filename.to_string()
        },

        ["/account", "import", ..] |
        ["/a", "import", ..] => {
            let err_msg = format!("{}", "Usage: /account import <filename>.json".yellow());
            Command::InvalidSyntax { err_msg }
        },

        ["/account", "export"] |
        ["/a", "export"] => Command::AccountExport {
            filename: "".to_string()
        },

        ["/account", "export", filename] |
        ["/a", "export", filename] => Command::AccountExport {
            filename: filename.to_string()
        },

        ["/account", "export", ..] |
        ["/a", "export", ..] => {
            let err_msg = format!("{}", "Usage: /account export or /account export <filename>.json".yellow());
            Command::InvalidSyntax { err_msg }
        },

        ["/account", "delete"] |
        ["/a", "delete"] | 
        ["/account", "d"] |
        ["/a", "d"] => Command::AccountDelete { force: false },

        ["/account", "delete", "force"] |
        ["/a", "delete", "force"] |
        ["/account", "d", "force"] |
        ["/account", "delete", "f"] |
        ["/a", "d", "force"] |
        ["/a", "delete", "f"] |
        ["/account", "d", "f"] |
        ["/a", "d", "f"] => Command::AccountDelete { force: true },

        ["/account", "delete", ..] |
        ["/a", "delete", ..] |
        ["/account", "d", ..] |
        ["/a", "d", ..] => {
            let err_msg = format!("{}", "Usage: /account delete or /account delete force".yellow());
            Command::InvalidSyntax { err_msg }
        },

        ["/account"] |
        ["/a"] => Command::Account,

        ["/account", ..] |
        ["/a", ..] => {
            let err_msg = format!("{}", "Usage: /account\n/account register <username> <password> <password confirm>\n/account login <username> <password>\n/account edit\n/account export\n/account delete\n/account import\n/account logout".yellow());
            Command::InvalidSyntax { err_msg }
        }

        ["/room"] |
        ["/r"] => Command::RoomList,

        _ => Command::Unavailable
    }
}
