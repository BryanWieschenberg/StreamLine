use std::io::{self, Write};
use std::sync::{Arc, Mutex};
use colored::*;

use crate::shared::types::{Client, ClientState, Rooms, Clients, PublicKeys};
use crate::shared::utils::{lock_client, lock_rooms, lock_room, save_rooms_to_disk, send_message_locked, send_error_locked, send_success_locked, ColorizeExt, broadcast_user_list};
use crate::backend::command_utils::sync_room_members;
use crate::backend::dispatcher::CommandResult;

pub fn handle_users(client: Arc<Mutex<Client>>, rooms: &Rooms, room: &String) -> io::Result<CommandResult> {
    let rooms_map = lock_rooms(rooms)?;
    let room_arc = match rooms_map.get(room) {
        Some(r) => Arc::clone(r),
        None => {
            let mut c = lock_client(&client)?;
            send_message_locked(&mut c, &format!("Room {room} not found").yellow().to_string())?;
            return Ok(CommandResult::Handled);
        }
    };
    let room_guard = lock_room(&room_arc)?;
    let mut c = lock_client(&client)?;

    writeln!(c.stream, "{}", format!("Users in {room}:").green())?;
    c.stream.flush()?;

    for (uname, udata) in &room_guard.users {
        if !room_guard.online_users.contains(uname) || udata.hidden {
            continue;
        }

        let role = {
            let mut ch = udata.role.chars();
            match ch.next() {
                Some(f) => f.to_uppercase().collect::<String>() + ch.as_str(),
                None => String::new(),
            }
        };

        let nickname = if udata.nick.is_empty() {
            "None".to_string()
        } else {
            udata.nick.italic().to_string()
        };

        let color_display = if udata.color.is_empty() {
            "Default".to_string()
        } else {
            udata.color.to_string().truecolor_from_hex(&udata.color).to_string()
        };

        writeln!(c.stream, "> {} - Role: {}, Nickname: {}, Color: {}",
            uname.green(),
            role,
            nickname,
            color_display
        )?;
        c.stream.flush()?;
    }

    Ok(CommandResult::Handled)
}

pub fn handle_users_rename(client: Arc<Mutex<Client>>, clients: &Clients, rooms: &Rooms, pubkeys: &PublicKeys, room: &String, old_name: &String, new_name: &String) -> io::Result<CommandResult> {
    let rooms_map = lock_rooms(rooms)?;
    let room_arc = match rooms_map.get(room) {
        Some(r) => Arc::clone(r),
        None => {
            let mut c = lock_client(&client)?;
            send_message_locked(&mut c, &format!("Room {room} not found").yellow().to_string())?;
            return Ok(CommandResult::Handled);
        }
    };

    let username;
    {
        let c = lock_client(&client)?;
        username = match &c.state {
            ClientState::InRoom { username, .. } => username.clone(),
            _ => return Ok(CommandResult::Handled),
        };
    }

    if old_name != &username {
        let room_guard = lock_room(&room_arc)?;
        let caller_role = room_guard.users.get(&username).map(|u| u.role.as_str()).unwrap_or("user");
        let rank = match caller_role {
            "owner" => 4,
            "admin" => 3,
            "moderator" => 2,
            _ => 1,
        };
        if rank < 3 {
            let mut c = lock_client(&client)?;
            send_message_locked(&mut c, &"Error: Only admins and owners can rename other users".yellow().to_string())?;
            return Ok(CommandResult::Handled);
        }
    }

    {
        let mut room_guard = lock_room(&room_arc)?;
        let mut c = lock_client(&client)?;
        match room_guard.users.get_mut(old_name) {
            Some(u) => {
                if new_name == "reset" || new_name == "*" {
                    u.nick.clear();
                } else {
                    u.nick = new_name.clone();
                }
            }
            None => {
                send_message_locked(&mut c, &"Error: user record missing".red().to_string())?;
                return Ok(CommandResult::Handled);
            }
        }
        drop(room_guard);
        drop(room_arc);
    }

    drop(rooms_map);

    {
        let fresh_map = lock_rooms(rooms)?;
        let mut c = lock_client(&client)?;
        if let Err(e) = save_rooms_to_disk(&fresh_map) {
            send_error_locked(&mut c, &format!("Failed to save rooms: {e}"))?;
            return Ok(CommandResult::Handled);
        }
        if new_name == "reset" || new_name == "*" {
            send_success_locked(&mut c, &format!("Nickname for {old_name} reset"))?;
        } else {
            send_success_locked(&mut c, &format!("Nickname for {old_name} set to {new_name}"))?;
        }
    }

    let _ = sync_room_members(rooms, clients, pubkeys, room);
    let _ = broadcast_user_list(clients, rooms, room);
    Ok(CommandResult::Handled)
}

pub fn handle_users_recolor(client: Arc<Mutex<Client>>, clients: &Clients, rooms: &Rooms, pubkeys: &PublicKeys, room: &String, target_user: &String, color: &String) -> io::Result<CommandResult> {
    let rooms_map = lock_rooms(rooms)?;
    let room_arc = match rooms_map.get(room) {
        Some(r) => Arc::clone(r),
        None => {
            let mut c = lock_client(&client)?;
            send_message_locked(&mut c, &format!("Room {room} not found").yellow().to_string())?;
            return Ok(CommandResult::Handled);
        }
    };

    let username;
    {
        let c = lock_client(&client)?;
        username = match &c.state {
            ClientState::InRoom { username, .. } => username.clone(),
            _ => return Ok(CommandResult::Handled),
        };
    }

    if target_user != &username {
        let room_guard = lock_room(&room_arc)?;
        let caller_role = room_guard.users.get(&username).map(|u| u.role.as_str()).unwrap_or("user");
        let target_role = room_guard.users.get(target_user).map(|u| u.role.as_str()).unwrap_or("user");
        let c_rank = match caller_role { "owner" => 4, "admin" => 3, "moderator" => 2, _ => 1 };
        let t_rank = match target_role { "owner" => 4, "admin" => 3, "moderator" => 2, _ => 1 };
        
        if c_rank < 3 {
            let mut c = lock_client(&client)?;
            send_message_locked(&mut c, &"Error: Only admins and owners can recolor other users".yellow().to_string())?;
            return Ok(CommandResult::Handled);
        }
        if c_rank <= t_rank {
            let mut c = lock_client(&client)?;
            send_message_locked(&mut c, &"Error: Cannot recolor user with equal or higher privilege".yellow().to_string())?;
            return Ok(CommandResult::Handled);
        }
    }

    let c_str = color.trim().trim_start_matches('#');
    let formatted_color = if c_str == "reset" || c_str == "*" {
        String::new()
    } else {
        if c_str.len() != 6 || !c_str.chars().all(|c| c.is_ascii_hexdigit()) {
            let mut c = lock_client(&client)?;
            send_message_locked(&mut c, &"Error: Bad color hex, must be exactly 6 characters (e.g. #FF0000)".yellow().to_string())?;
            return Ok(CommandResult::Handled);
        }
        format!("#{c_str}")
    };

    {
        let mut room_guard = lock_room(&room_arc)?;
        let mut c = lock_client(&client)?;
        match room_guard.users.get_mut(target_user) {
            Some(u) => {
                u.color = formatted_color.clone();
            }
            None => {
                send_message_locked(&mut c, &"Error: user record missing".red().to_string())?;
                return Ok(CommandResult::Handled);
            }
        }
        drop(room_guard);
        drop(room_arc);
    }

    drop(rooms_map);

    {
        let fresh_map = lock_rooms(rooms)?;
        let mut c = lock_client(&client)?;
        if let Err(e) = save_rooms_to_disk(&fresh_map) {
            send_error_locked(&mut c, &format!("Failed to save rooms: {e}"))?;
            return Ok(CommandResult::Handled);
        }
        if formatted_color.is_empty() {
            send_success_locked(&mut c, "Color cleared")?;
        } else {
            use std::io::Write;
            writeln!(c.stream, "{} {}", "Color set to".green(), formatted_color.clone().truecolor_from_hex(&formatted_color))?;
            c.stream.flush()?;
        }
    }

    let _ = sync_room_members(rooms, clients, pubkeys, room);
    let _ = broadcast_user_list(clients, rooms, room);
    Ok(CommandResult::Handled)
}

pub fn handle_users_hide(client: Arc<Mutex<Client>>, clients: &Clients, rooms: &Rooms, pubkeys: &PublicKeys, username: &String, room: &String) -> io::Result<CommandResult> {
    {
        let rooms_map = lock_rooms(rooms)?;
        let room_arc = match rooms_map.get(room) {
            Some(r) => Arc::clone(r),
            None => {
                let mut c = lock_client(&client)?;
                send_message_locked(&mut c, &format!("Room {room} not found").yellow().to_string())?;
                return Ok(CommandResult::Handled);
            }
        };

        let now_hidden;
        {
            let mut room_guard = lock_room(&room_arc)?;
            let mut c = lock_client(&client)?;
            match room_guard.users.get_mut(username) {
                Some(u) => {
                    u.hidden = !u.hidden;
                    now_hidden = u.hidden;
                }
                None => {
                    send_message_locked(&mut c, &"Error: user record missing".red().to_string())?;
                    return Ok(CommandResult::Handled);
                }
            }
            drop(room_guard);

            if let Err(e) = save_rooms_to_disk(&rooms_map) {
                send_error_locked(&mut c, &format!("Failed to save rooms: {e}"))?;
                return Ok(CommandResult::Handled);
            }

            if now_hidden {
                send_success_locked(&mut c, "You are now hidden")?;
            } else {
                send_success_locked(&mut c, "You are no longer hidden")?;
            }
        }
    }

    let _ = sync_room_members(rooms, clients, pubkeys, room);
    let _ = broadcast_user_list(clients, rooms, room);
    Ok(CommandResult::Handled)
}
