pub fn get_help_message() -> &'static str {
r#"Available commands:
/help      - Show this help menu
/ping      - Check connection to the server
/clear     - Clear the chat screen
/quit      - Exit the application
/room      - List or join chat rooms
/account   - Manage your account
"#
}

pub fn generate_hash(input: &str) -> String {
    use sha2::{Sha256, Digest};

    let mut hasher = Sha256::new();
    hasher.update(input);
    let result = hasher.finalize();

    hex::encode(result)
}
