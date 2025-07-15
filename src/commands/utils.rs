use std::fs::File;
use std::io::BufReader;
use serde_json::Value;
use sha2::{Sha256, Digest};

pub fn get_help_message() -> &'static str {
r#"Available commands:
/help      - Show this help menu
/ping      - Check connection to the server
/clear     - Clear the chat screen
/quit      - Exit the application
/room      - List or join chat rooms
/account   - Manage your account"#
}

pub fn generate_hash(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input);
    let result = hasher.finalize();

    hex::encode(result)
}

pub fn is_unique_username(username: String) -> bool {
    let file = match File::open("data/users.json") {
        Ok(f) => f,
        Err(_) => return true, // File doesn't exist yet, allow any usernames
    };

    // Read into a JSON object
    let reader = BufReader::new(file);
    let users: Value = match serde_json::from_reader(reader) {
        Ok(v) => v,
        Err(_) => return true, // Misformatted file, assume no users
    };

    // Check if it's an object and contains the username key
    match users.as_object() {
        Some(map) => !map.contains_key(&username),
        None => true,
    }
}
