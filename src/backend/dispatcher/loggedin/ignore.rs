use std::io::{self};
use std::sync::{Arc, Mutex};
use serde_json::{json, Value};
use crate::shared::types::Client;
use crate::shared::utils::{lock_client, lock_users_storage, load_json, save_json, send_error, send_success};
use crate::backend::dispatcher::CommandResult;

pub fn handle_ignore_list(client: Arc<Mutex<Client>>) -> io::Result<CommandResult> {
    let ignore_list = {
        let client_guard = lock_client(&client)?;
        client_guard.ignore_list.clone()
    };
    
    if ignore_list.is_empty() {
        send_success(&client, "You do not currently have anyone ignored")?;
    } else {
        send_success(&client, &format!("Currently ignoring: {}", ignore_list.join(", ")))?;
    }
    Ok(CommandResult::Handled)
}

pub fn handle_ignore_add(client: Arc<Mutex<Client>>, username: &String, users: &String) -> io::Result<CommandResult> {
    let to_add: Vec<String> = users
        .split_whitespace()
        .filter(|u| !u.is_empty() && u != username)
        .map(|u| u.to_string())
        .collect();

    let (added, already): (Vec<String>, Vec<String>) = {
        let mut client_guard = lock_client(&client)?;
        let mut added = Vec::new();
        let mut already = Vec::new();
        for u in &to_add {
            if client_guard.ignore_list.contains(u) {
                already.push(u.clone());
            } else {
                client_guard.ignore_list.push(u.clone());
                added.push(u.clone());
            }
        }
        (added, already)
    };

    if !added.is_empty() {
        let _ulock = lock_users_storage()?;
        let mut users_json = load_json("data/users.json")?;

        if let Some(ignore_arr) = users_json[username]
            .get_mut("ignore")
            .and_then(Value::as_array_mut)
        {
            for u in &added {
                ignore_arr.push(json!(u));
            }
        }

        save_json("data/users.json", &users_json)?;
    }

    if !added.is_empty() {
        send_success(&client, &format!("Added to ignore list: {}", added.join(", ")))?;
    }
    if !already.is_empty() {
        send_error(&client, &format!("Already ignored: {}", already.join(", ")))?;
    }
    Ok(CommandResult::Handled)
}

pub fn handle_ignore_remove(client: Arc<Mutex<Client>>, username: &String, users: &String) -> io::Result<CommandResult> {
    let to_remove: Vec<String> = users
        .split_whitespace()
        .filter(|u| !u.is_empty())
        .map(|u| u.to_string())
        .collect();

    let (removed, not_found): (Vec<String>, Vec<String>) = {
        let mut client_guard = lock_client(&client)?;
        let mut removed = Vec::new();
        let mut not_found = Vec::new();
        for u in &to_remove {
            if client_guard.ignore_list.contains(u) {
                removed.push(u.clone());
            } else {
                not_found.push(u.clone());
            }
        }
        client_guard.ignore_list.retain(|u| !removed.contains(u));
        (removed, not_found)
    };

    if !removed.is_empty() {
        let _ulock = lock_users_storage()?;
        let mut users_json = load_json("data/users.json")?;

        if let Some(ignore_arr) = users_json[username]
            .get_mut("ignore")
            .and_then(Value::as_array_mut)
        {
            ignore_arr.retain(|v| !removed.iter().any(|u| v == u));
        }

        save_json("data/users.json", &users_json)?;
    }

    if !removed.is_empty() {
        send_success(&client, &format!("Removed from ignore list: {}", removed.join(", ")))?;
    }
    if !not_found.is_empty() {
        send_error(&client, &format!("Not in ignore list: {}", not_found.join(", ")))?;
    }
    Ok(CommandResult::Handled)
}
