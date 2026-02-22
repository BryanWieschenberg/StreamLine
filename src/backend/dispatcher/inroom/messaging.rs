use std::io::{self};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use colored::*;

use crate::shared::types::{Client, ClientState, Clients, Rooms};
use crate::shared::utils::{lock_client, lock_clients, lock_rooms, lock_room, check_mute, send_error, send_message, send_success, broadcast_message};
use crate::backend::dispatcher::CommandResult;

pub fn handle_afk(client: Arc<Mutex<Client>>, _rooms: &Rooms, _username: &String, _room: &String) -> io::Result<CommandResult> {
    let mut c = lock_client(&client)?;
    if let ClientState::InRoom { is_afk, .. } = &mut c.state {
        *is_afk = true;
    }
    use std::io::Write;
    writeln!(c.stream, "{}", "You are now set as AFK".yellow())?;
    Ok(CommandResult::Handled)
}

pub fn handle_dm(client: Arc<Mutex<Client>>, clients: &Clients, rooms: &Rooms, username: &String, room: &String, recipient: &String, message: &String) -> io::Result<CommandResult> {
    if let Some(msg) = check_mute(rooms, room, username)? {
        send_error(&client, &msg)?;
        return Ok(CommandResult::Handled);
    }

    let room_arc = {
        let rooms_map = lock_rooms(rooms)?;
        match rooms_map.get(room) {
            Some(r) => Arc::clone(r),
            None => {
                send_message(&client, &format!("Room {room} not found").yellow().to_string())?;
                return Ok(CommandResult::Handled);
            }
        }
    };

    let is_online = {
        let room_guard = lock_room(&room_arc)?;
        room_guard.online_users.contains(recipient)
    };

    if !is_online {
        send_message(&client, &format!("{recipient} is not currently online").yellow().to_string())?;
        return Ok(CommandResult::Handled);
    }

    {
        let root_client = lock_client(&client)?;
        if root_client.ignore_list.contains(recipient) {
            send_message(&client, &format!("Cannot send message to {recipient}, you have them ignored").yellow().to_string())?;
            return Ok(CommandResult::Handled);
        }
    }

    let clients_map = lock_clients(clients)?;
    let mut found = false;
    for client_arc in clients_map.values() {
        let mut c = match client_arc.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                eprintln!("Client lock poisoned: {poisoned}");
                continue;
            }
        };

        match &c.state {
            ClientState::InRoom { username: uname, room: rname, .. }
                if uname == recipient && rname == room => {
                    if c.ignore_list.contains(username) {
                        found = true;
                        break;
                    }

                    use std::io::Write;
                    writeln!(c.stream, "{}", format!("(Private) {username}: {message}").cyan().italic())?;
                    found = true;
                    break;
                }
            _ => continue,
        }
    }

    if found {
        send_success(&client, &format!("Message sent to {recipient}"))?;
    } else {
        send_error(&client, &format!("Failed to deliver message to {recipient}"))?;
    }

    Ok(CommandResult::Handled)
}

pub fn handle_me(client: Arc<Mutex<Client>>, clients: &Clients, rooms: &Rooms, username: &String, room: &String, action: &String) -> io::Result<CommandResult> {
    if let Some(msg) = check_mute(rooms, room, username)? {
        send_error(&client, &msg)?;
        return Ok(CommandResult::Handled);
    }
    let msg = format!("* {username} {action}").bright_green().to_string();
    broadcast_message(clients, room, username, &msg, true, false)?;
    Ok(CommandResult::Handled)
}

pub fn handle_seen(client: Arc<Mutex<Client>>, rooms: &Rooms, room: &String, username: &String) -> io::Result<CommandResult> {
    let rooms_map = lock_rooms(rooms)?;
    let room_arc = match rooms_map.get(room) {
        Some(r) => Arc::clone(r),
        None => {
            send_message(&client, &"Room not found".yellow().to_string())?;
            return Ok(CommandResult::Handled);
        }
    };
    let room_guard = lock_room(&room_arc)?;

    let is_online = room_guard.online_users.iter().any(|u| u == username);

    let response = if is_online {
        format!("{username} is online now").green().to_string()
    } else {
        match room_guard.users.get(username) {
            Some(info) => {
                let now_secs = match SystemTime::now().duration_since(UNIX_EPOCH) {
                    Ok(d)  => d.as_secs(),
                    Err(_) => 0,
                };
                let diff = now_secs.saturating_sub(info.last_seen);
                let days = diff / 86_400;
                let hrs  = (diff % 86_400) / 3_600;
                let mins = (diff % 3_600) / 60;
                let secs = diff % 60;
                format!("{username} was last seen {days}d {hrs}h {mins}m {secs}s ago").green().to_string()
            }
            None => format!("{username} has never joined this room").yellow().to_string(),
        }
    };

    send_success(&client, &response)?;
    Ok(CommandResult::Handled)
}

pub fn handle_announce(client: Arc<Mutex<Client>>, clients: &Clients, rooms: &Rooms, username: &String, room: &String, message: &String) -> io::Result<CommandResult> {
    if let Some(msg) = check_mute(rooms, room, username)? {
        send_error(&client, &msg)?;
        return Ok(CommandResult::Handled);
    }
    let msg = format!("Announcement: {message}").bright_yellow().to_string();
    broadcast_message(clients, room, username, &msg, true, true)?;
    Ok(CommandResult::Handled)
}
