use std::io::{self, BufRead, BufReader};
use std::fs::{File, OpenOptions};
use serde::Serialize;
use serde_json::{json, Serializer, Value};
use serde_json::ser::PrettyFormatter;
use std::sync::{Arc, Mutex};
use colored::*;

use crate::shared::types::{Client, ClientState, PublicKeys};
use crate::shared::utils::{lock_client, lock_users_storage, load_json, save_json, send_error, send_success, send_error_locked, send_message_locked, send_success_locked};
use crate::backend::dispatcher::CommandResult;
use crate::backend::command_utils::generate_hash;

pub fn handle_account_logout(client: Arc<Mutex<Client>>, username: &String, pubkeys: &PublicKeys) -> io::Result<CommandResult> {
    {
        let mut pubkeys_map = match pubkeys.lock() {
            Ok(g) => g,
            Err(_) => return Ok(CommandResult::Handled),
        };

        pubkeys_map.remove(username);
    }
    
    let mut c = lock_client(&client)?;
    c.state = ClientState::Guest;
    let _ = crate::shared::utils::send_message_locked(&mut c, "/GUEST_STATE");
    let _ = crate::shared::utils::send_success_locked(&mut c, &format!("Logged out: {username}"));
    
    Ok(CommandResult::Handled)
}

pub fn handle_account_edit_username(client: Arc<Mutex<Client>>, username: &String, new_username: &String) -> io::Result<CommandResult> {
    {
        let mut c = lock_client(&client)?;
        if new_username.is_empty() {
             send_error_locked(&mut c, "Username cannot be empty")?;
             return Ok(CommandResult::Handled);
        }
    }
    
    let _lock = lock_users_storage()?;

    let mut users = load_json("data/users.json")?;

    if users.get(new_username).is_some() {
        send_error(&client, "Username is already taken")?;
        return Ok(CommandResult::Handled);
    }

    if let Some(old_data) = users.get(username).cloned() {
        users[new_username] = old_data;
        if let Some(map) = users.as_object_mut() {
            map.remove(username);
        }
    } else {
        send_error(&client, "Original username not found")?;
        return Ok(CommandResult::Handled);
    }

    save_json("data/users.json", &users)?;

    let old_username = username.clone();
    let mut c = lock_client(&client)?;
    c.state = ClientState::LoggedIn { username: new_username.clone() };

    send_message_locked(&mut c, "/GUEST_STATE")?;
    send_success_locked(&mut c, &format!("Username changed from {old_username} to: {new_username}"))?;
    Ok(CommandResult::Handled)
}

pub fn handle_account_edit_password(client: Arc<Mutex<Client>>, username: &String, current_password: &String, new_password: &String) -> io::Result<CommandResult> {
    let _lock = lock_users_storage()?;

    let mut users = load_json("data/users.json")?;

    let user_obj = if let Some(obj) = users.get_mut(username) {
        obj
    }
    else {
        send_error(&client, "Username not found")?;
        return Ok(CommandResult::Handled);
    };

    let stored_hash = match user_obj.get("password").and_then(|v| v.as_str()) {
        Some(hash) => hash,
        None => {
            send_error(&client, "Password field missing")?;
            return Ok(CommandResult::Handled);
        }
    };

    if generate_hash(&current_password) != stored_hash {
        send_error(&client, "Incorrect current password")?;
        return Ok(CommandResult::Handled);
    }

    let new_hash = generate_hash(&new_password);
    user_obj["password"] = Value::String(new_hash);

    save_json("data/users.json", &users)?;

    send_success(&client, "Password updated successfully")?;
    Ok(CommandResult::Handled)
}

pub fn handle_account_import(client: Arc<Mutex<Client>>, filename: &String) -> io::Result<CommandResult> {
    let safe_filename = if !filename.ends_with(".json") {
        format!("{filename}.json")
    }
    else {
        filename.clone()
    };

    let import_path = format!("data/vault/users/{safe_filename}");
    let import_file = match File::open(&import_path) {
        Ok(file) => file,
        Err(_) => {
            send_error(&client, &format!("Could not open {import_path}"))?;
            return Ok(CommandResult::Handled);
        }
    };

    let import_reader = BufReader::new(import_file);
    let import_user: Value = match serde_json::from_reader(import_reader) {
        Ok(data) => data,
        Err(_) => {
            send_error(&client, "Invalid JSON format in import file")?;
            return Ok(CommandResult::Handled);
        }
    };

    let (imported_username, user_data) = match import_user.as_object().and_then(|obj| obj.iter().next()) {
        Some((u, data)) => (u.clone(), data.clone()),
        None => {
            send_error(&client, "Import file is empty or malformed")?;
            return Ok(CommandResult::Handled);
        }
    };

    let _lock = lock_users_storage()?;

    let mut users = load_json("data/users.json")?;

    if users.get(&imported_username).is_some() {
        send_error(&client, &format!("User {imported_username} already exists"))?;
        return Ok(CommandResult::Handled);
    }

    users[&imported_username] = user_data;

    save_json("data/users.json", &users)?;

    send_success(&client, &format!("Imported user: {imported_username}"))?;
    Ok(CommandResult::Handled)
}

pub fn handle_account_export(client: Arc<Mutex<Client>>, username: &String, filename: &String) -> io::Result<CommandResult> {
    let _lock = lock_users_storage()?;

    let users = load_json("data/users.json")?;

    let user_data = match users.get(username) {
        Some(data) => data.clone(),
        None => {
            send_error(&client, "Your account data could not be found")?;
            return Ok(CommandResult::Handled);
        }
    };

    let final_filename = {
        if filename.is_empty() {
            let timestamp = chrono::Local::now().format("%y%m%d%H%M%S").to_string();
            format!("{username}_{timestamp}.json")
        } else if !filename.ends_with(".json") {
            format!("{filename}.json")
        }
        else {
            filename.clone()
        }
    };

    let export_path = format!("data/vault/users/{final_filename}");
    let export_file = OpenOptions::new().create(true).write(true).truncate(true).open(&export_path)?;

    let mut writer = std::io::BufWriter::new(export_file);
    let formatter = PrettyFormatter::with_indent(b"    ");
    let mut ser = Serializer::with_formatter(&mut writer, formatter);

    json!({ username: user_data }).serialize(&mut ser)?;

    send_success(&client, &format!("Exported account data to: {final_filename}"))?;
    Ok(CommandResult::Handled)
}

pub fn handle_account_delete(client: Arc<Mutex<Client>>, username: &String, pubkeys: &PublicKeys, force: bool) -> io::Result<CommandResult> {
    if !force {
        let mut c = lock_client(&client)?;
        use std::io::Write;
        writeln!(c.stream, "{}", "Are you sure you want to delete your account? (y/n): ".red())?;

        let mut reader: BufReader<std::net::TcpStream> = BufReader::new(c.stream.try_clone()?);
        drop(c);
        loop {
            let mut line = String::new();
            let bytes_read = reader.read_line(&mut line)?;
            if bytes_read == 0 {
                send_error(&client, "Connection closed")?;
                return Ok(CommandResult::Stop);
            }

            let input = line.trim().to_lowercase();
            match input.as_str() {
                "y" => break,
                "n" => {
                    send_error(&client, "Account deletion cancelled")?;
                    return Ok(CommandResult::Handled);
                },
                _ => {
                    send_error(&client, "(y/n): ")?;
                }
            }
        }
    }

    let _lock = lock_users_storage()?;
    let mut users = load_json("data/users.json")?;

    if users.get(username).is_none() {
        send_error(&client, "User not found in records")?;
        return Ok(CommandResult::Handled);
    }

    if let Some(obj) = users.as_object_mut() {
        obj.remove(username);
    }
    else {
        send_error(&client, "Malformed users.json")?;
        return Ok(CommandResult::Handled);
    }

    save_json("data/users.json", &users)?;

    {
        let mut pubkeys_map = match pubkeys.lock() {
            Ok(g) => g,
            Err(_) => return Ok(CommandResult::Handled),
        };
        pubkeys_map.remove(username);
    }

    let mut c = lock_client(&client)?;
    c.state = ClientState::Guest;
    send_message_locked(&mut c, "/GUEST_STATE")?;
    send_success_locked(&mut c, &format!("Account {username} deleted successfully, you are now a guest"))?;

    Ok(CommandResult::Handled)
}

pub fn handle_account(client: Arc<Mutex<Client>>, username: &String) -> io::Result<CommandResult> {
    send_success(&client, &format!("Currently logged in as: {username} (not in a room)"))?;
    Ok(CommandResult::Handled)
}
