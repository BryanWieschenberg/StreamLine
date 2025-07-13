pub enum Command {
    Help,
    Ping,
    Clear,
    Quit,

    // Room-related
    RoomList,
    RoomJoin(String),
    RoomCreate(String),
    RoomDelete(String),
    RoomImport(String),

    // Account-related
    AccountRegister { username: String, password: String, confirm: String },
    AccountEditUsername(String),
    AccountEditPassword { new: String, confirm: String },
    AccountLogin { username: String, password: String },
    AccountExport(Option<String>),
    AccountDelete,
    AccountImport(String),

    Unknown,
}

pub fn parse_command(input: &str) -> Command {
    let tokens: Vec<&str> = input.trim().split_whitespace().collect();

    match tokens.as_slice() {
        ["/help"] | ["/h"] => Command::Help,
        ["/ping"] => Command::Ping,
        ["/clear"] | ["/c"] => Command::Clear,
        ["/quit"] | ["/exit"] | ["q"] | ["e"] => Command::Quit,

        // Room commands
        ["/room"] => Command::RoomList,
        ["/room", "join", room] => Command::RoomJoin(room.to_string()),
        ["/room", "create", room] => Command::RoomCreate(room.to_string()),
        ["/room", "delete", room] => Command::RoomDelete(room.to_string()),
        ["/room", "import", file] => Command::RoomImport(file.to_string()),

        // Account commands
        ["/account", "register", u, p, c] => Command::AccountRegister {
            username: u.to_string(),
            password: p.to_string(),
            confirm: c.to_string(),
        },
        ["/account", "edit", "username", new] => Command::AccountEditUsername(new.to_string()),
        ["/account", "edit", "password", new, confirm] => Command::AccountEditPassword {
            new: new.to_string(),
            confirm: confirm.to_string(),
        },
        ["/account", "login", u, p] => Command::AccountLogin {
            username: u.to_string(),
            password: p.to_string(),
        },
        ["/account", "export"] => Command::AccountExport(None),
        ["/account", "export", filename] => Command::AccountExport(Some(filename.to_string())),
        ["/account", "delete"] => Command::AccountDelete,
        ["/account", "import", file] => Command::AccountImport(file.to_string()),

        _ => Command::Unknown,
    }
}
