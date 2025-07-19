use colored::*;

#[allow(dead_code)]
pub enum Command {
    Help,
    Ping,
    Quit,

    AccountRegister { username: String, password: String, confirm: String },
    AccountLogin { username: String, password: String },
    AccountLogout,
    Account,

    InvalidSyntax { err_msg: String },
    Unavailable
}

#[allow(dead_code)]
pub fn parse_command(input: &str) -> Command {
    let tokens: Vec<&str> = input.trim().split_whitespace().collect();

    match tokens.as_slice() {
        ["/help"] | ["/h"] => Command::Help,
        ["/ping"] => Command::Ping,
        ["/quit"] | ["/exit"] | ["q"] | ["e"] => Command::Quit,

        ["/account", "register", username, password, confirm_password] => Command::AccountRegister {
            username: username.to_string(),
            password: password.to_string(),
            confirm: confirm_password.to_string()
        },

        ["/account", "register", ..] => {
            let err_msg = format!("{}", "Usage: /account register <username> <password> <password confirm>".yellow());
            Command::InvalidSyntax { err_msg }
        }

        ["/account", "login", u, p] => Command::AccountLogin {
            username: u.to_string(),
            password: p.to_string(),
        },

        ["/account", "login", ..] => {
            let err_msg = format!("{}", "Usage: /account login <username> <password>".yellow());
            Command::InvalidSyntax { err_msg }
        }

        ["/account", "logout"] => Command::AccountLogout {},

        ["/account"] => Command::Account,

        // ["/account", "edit", "username", new] => Command::AccountEditUsername(new.to_string()),
        // ["/account", "edit", "password", new, confirm] => Command::AccountEditPassword {
        //     new: new.to_string(),
        //     confirm: confirm.to_string(),
        // },
        
        // ["/account", "export"] => Command::AccountExport(None),
        // ["/account", "export", filename] => Command::AccountExport(Some(filename.to_string())),
        // ["/account", "delete"] => Command::AccountDelete,
        // ["/account", "import", file] => Command::AccountImport(file.to_string()),

        // Room commands
        // ["/room"] => Command::RoomList,
        // ["/room", "join", room] => Command::RoomJoin(room.to_string()),
        // ["/room", "create", room] => Command::RoomCreate(room.to_string()),
        // ["/room", "delete", room] => Command::RoomDelete(room.to_string()),
        // ["/room", "import", file] => Command::RoomImport(file.to_string()),

        _ => Command::Unavailable
    }
}
