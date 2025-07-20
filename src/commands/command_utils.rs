use sha2::{Sha256, Digest};
use crate::state::types::{Clients, ClientState};

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
