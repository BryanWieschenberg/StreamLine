use std::io::{self, BufRead};
use std::sync::{Arc, Mutex};
use colored::*;

use crate::shared::types::{Client, ClientState, Clients, Rooms, RoomUser, PublicKeys};
use crate::shared::utils::{lock_client, lock_rooms, lock_room, save_rooms_to_disk, ColorizeExt, send_message_locked, send_error_locked, send_success_locked, broadcast_user_list};
use crate::backend::dispatcher::CommandResult;
use crate::backend::command_utils::{RESTRICTED_COMMANDS, command_order, sync_room_commands};

pub fn handle_super_roles(client: Arc<Mutex<Client>>, rooms: &Rooms, room: &String) -> io::Result<CommandResult> {
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

    let mod_cmds = &room_guard.roles.moderator;
    let user_cmds = &room_guard.roles.user;

    let all_cmds: Vec<&str> = command_order()
        .into_iter()
        .filter(|c| RESTRICTED_COMMANDS.contains(c))
        .collect();

    let mut lines = Vec::<String>::new();
    lines.push("Role info:".to_string());
    lines.push("(Owners and admins can access every command)".to_string());

    for cmd in all_cmds {
        let m_disp = if mod_cmds.contains(&cmd.to_string()) {
            "M".bright_yellow().bold().to_string()
        } else {
            " ".to_string()
        };

        let u_disp = if user_cmds.contains(&cmd.to_string()) {
            "U".white().bold().to_string()
        } else {
            " ".to_string()
        };

        let indent = if cmd.contains('.') { "   " } else { "" };
        lines.push(format!("  > {m_disp} {u_disp} {indent}{cmd}"));
    }

    send_success_locked(&mut c, &lines.join("\n"))?;
    Ok(CommandResult::Handled)
}
        
pub fn handle_super_roles_add(client: Arc<Mutex<Client>>, clients: &Clients, rooms: &Rooms, room: &String, role: &String, commands: &String) -> io::Result<CommandResult> {
    let mut added = Vec::<String>::new();

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
        let mut room_guard = lock_room(&room_arc)?;
        let mut c = lock_client(&client)?;

        let target_role = match role.to_lowercase().as_str() {
            "user" => "user",
            "mod" | "moderator" => "moderator",
            _ => {
                send_message_locked(&mut c, &"Error: Role must be user|mod".yellow().to_string())?;
                return Ok(CommandResult::Handled);
            }
        };

        let cmd_tokens: Vec<&str> = commands.split_whitespace().collect();
        let invalid: Vec<String> = cmd_tokens.iter().filter(|c_token| !RESTRICTED_COMMANDS.contains(**c_token)).map(|c_token| (*c_token).to_string()).collect();
        if !invalid.is_empty() {
            send_message_locked(&mut c, &format!("Error: Unknown commands: {}", invalid.join(", ")).yellow().to_string())?;
            return Ok(CommandResult::Handled);
        }

        let list = if target_role == "moderator" {
            &mut room_guard.roles.moderator
        } else {
            &mut room_guard.roles.user
        };

        for &c_token in &cmd_tokens {
            let c_str = c_token.to_string();
            if !list.contains(&c_str) {
                list.push(c_str.clone());
                added.push(c_str);
            }
        }
        drop(room_guard);

        if !added.is_empty() {
            if let Err(e) = save_rooms_to_disk(&rooms_map) {
                send_error_locked(&mut c, &format!("Failed to save rooms: {e}"))?;
                return Ok(CommandResult::Handled);
            }
        }

        if added.is_empty() {
            send_message_locked(&mut c, &"No changes made".yellow().to_string())?;
            return Ok(CommandResult::Handled);
        } else {
            send_success_locked(&mut c, &format!("Added for {target_role}: {}", added.join(", ")))?;
        }
    }

    let _ = sync_room_commands(rooms, clients, room);
    Ok(CommandResult::Handled)
}

pub fn handle_super_roles_revoke(client: Arc<Mutex<Client>>, clients: &Clients, rooms: &Rooms, room: &String, role: &String, commands: &String) -> io::Result<CommandResult> {
    let mut removed = Vec::<String>::new();

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
        let mut room_guard = lock_room(&room_arc)?;
        let mut c = lock_client(&client)?;

        let target_role = match role.to_lowercase().as_str() {
            "user" => "user",
            "mod" | "moderator" => "moderator",
            _ => {
                send_message_locked(&mut c, &"Error: Role must be user|mod".yellow().to_string())?;
                return Ok(CommandResult::Handled);
            }
        };

        let cmd_tokens: Vec<&str> = commands.split_whitespace().collect();
        let invalid: Vec<String> = cmd_tokens.iter().filter(|c_token| !RESTRICTED_COMMANDS.contains(**c_token)).map(|c_token| (*c_token).to_string()).collect();
        if !invalid.is_empty() {
            send_message_locked(&mut c, &format!("Error: Unknown commands: {}", invalid.join(", ")).yellow().to_string())?;
            return Ok(CommandResult::Handled);
        }

        let list = if target_role == "moderator" {
            &mut room_guard.roles.moderator
        } else {
            &mut room_guard.roles.user
        };

        list.retain(|existing| {
            let keep = !cmd_tokens.iter().any(|c_str_ref| c_str_ref == existing);
            if !keep {
                removed.push(existing.clone());
            }
            keep
        });
        drop(room_guard);

        if !removed.is_empty() {
            if let Err(e) = save_rooms_to_disk(&rooms_map) {
                send_error_locked(&mut c, &format!("Failed to save rooms: {e}"))?;
                return Ok(CommandResult::Handled);
            }
        }

        if removed.is_empty() {
            send_message_locked(&mut c, &"No changes made".yellow().to_string())?;
            return Ok(CommandResult::Handled);
        } else {
            send_success_locked(&mut c, &format!("Revoked for {target_role}: {}", removed.join(", ")))?;
        }
    }

    let _ = sync_room_commands(rooms, clients, room);
    Ok(CommandResult::Handled)
}

pub fn handle_super_roles_assign(client: Arc<Mutex<Client>>, clients: &Clients, rooms: &Rooms, pubkeys: &PublicKeys, room: &String, role: &String, users: &String) -> io::Result<CommandResult> {
    let target_role = match role.to_lowercase().as_str() {
        "usr" | "user" => "user",
        "mod" | "moderator" => "moderator",
        "admin" | "administrator" => "admin",
        "owner" | "creator" | "founder" => "owner",
        _ => {
            let mut c = lock_client(&client)?;
            send_message_locked(&mut c, &"Error: Role must be user|mod|admin|owner".yellow().to_string())?;
            return Ok(CommandResult::Handled);
        }
    };

    let users_vec: Vec<&str> = users.split_whitespace().collect();
    if users_vec.is_empty() {
        let mut c = lock_client(&client)?;
        send_message_locked(&mut c, &"Error: No users specified".yellow().to_string())?;
        return Ok(CommandResult::Handled);
    }

    if target_role == "owner" && users_vec.len() != 1 {
        let mut c = lock_client(&client)?;
        send_message_locked(&mut c, &"Error: Only 1 user may be assigned to owner".yellow().to_string())?;
        return Ok(CommandResult::Handled);
    }

    let username;
    {
        let c = lock_client(&client)?;
        username = match &c.state {
            ClientState::InRoom { username, .. } => username.clone(),
            _ => return Ok(CommandResult::Handled),
        };
    }

    let mut owner_transfer_approved = false;
    if target_role == "owner" {
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
            let room_guard = lock_room(&room_arc)?;
            match room_guard.users.get(&username) {
                Some(u) if u.role == "owner" => {},
                _ => {
                    let mut c = lock_client(&client)?;
                    send_message_locked(&mut c, &"Error: Only the room owner can transfer ownership".yellow().to_string())?;
                    return Ok(CommandResult::Handled);
                }
            }
        }

        let new_owner = users_vec[0];
        let mut c = lock_client(&client)?;
        use std::io::Write;
        writeln!(c.stream, "{}", format!("Assigning {new_owner} as owner will transfer room ownership to them. Are you sure you want to do this? (y/n): ").red())?;
        c.stream.flush()?;

        let mut reader = std::io::BufReader::new(c.stream.try_clone()?);
        drop(c);
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line)? == 0 { return Ok(CommandResult::Stop); }
            match line.trim().to_lowercase().as_str() {
                "y" => { owner_transfer_approved = true; break; },
                "n" => {
                    let mut c = lock_client(&client)?;
                    send_message_locked(&mut c, &"Owner transfer cancelled".yellow().to_string())?;
                    return Ok(CommandResult::Handled);
                }
                _ => {
                    let mut c = lock_client(&client)?;
                    writeln!(c.stream, "{}", "(y/n): ".red())?;
                    c.stream.flush()?;
                    drop(c);
                }
            }
        }
    }

    let mut assigned = Vec::<String>::new();
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
        let mut room_guard = lock_room(&room_arc)?;
        let mut c = lock_client(&client)?;

        if target_role == "owner" && owner_transfer_approved {
            let new_owner = users_vec[0];
            if new_owner != username {
                if let Some(cur_owner) = room_guard.users.get_mut(&username) {
                    if cur_owner.role == "owner" { cur_owner.role = "admin".to_string(); }
                }
            }
        }

        for &u in &users_vec {
            let entry = room_guard.users.entry(u.to_string()).or_insert(RoomUser {
                nick: "".to_string(), color: "".to_string(), role: "user".to_string(),
                hidden: false, last_seen: 0, banned: false, ban_stamp: 0, ban_length: 0, ban_reason: "".to_string(),
                muted: false, mute_stamp: 0, mute_length: 0, mute_reason: "".to_string()
            });
            if entry.role == "owner" && target_role != "owner" { continue; }
            if entry.role != target_role {
                entry.role = target_role.to_string();
                assigned.push(u.to_string());
            }
        }
        drop(room_guard);

        if !assigned.is_empty() {
            if let Err(e) = save_rooms_to_disk(&rooms_map) {
                send_error_locked(&mut c, &format!("Failed to save rooms: {e}"))?;
                return Ok(CommandResult::Handled);
            }
        }

        if assigned.is_empty() {
            send_message_locked(&mut c, &"No role changes made".yellow().to_string())?;
        } else {
            send_success_locked(&mut c, &format!("Assigned role '{target_role}' to: {}", assigned.join(", ")))?;
        }
    }

    if !assigned.is_empty() {
        let _ = sync_room_commands(rooms, clients, room);
        let _ = crate::backend::command_utils::sync_room_members(rooms, clients, pubkeys, room);
        let _ = broadcast_user_list(clients, rooms, room);
    }

    Ok(CommandResult::Handled)
}

pub fn handle_super_roles_recolor(client: Arc<Mutex<Client>>, clients: &Clients, rooms: &Rooms, pubkeys: &PublicKeys, room: &String, role: &String, color: &String) -> io::Result<CommandResult> {
    let hex = color.trim().trim_start_matches('#');
    if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        let mut c = lock_client(&client)?;
        send_message_locked(&mut c, &"Error: Color must be a 6‑digit hex value".yellow().to_string())?;
        return Ok(CommandResult::Handled);
    }
    let hex_with_hash = format!("#{hex}");

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
        let mut room_guard = lock_room(&room_arc)?;
        let mut c = lock_client(&client)?;

        let role_key = match role.to_lowercase().as_str() {
            "user" => "user",
            "mod" | "moderator" => "moderator",
            "admin" => "admin",
            "owner" => "owner",
            _ => {
                send_message_locked(&mut c, &"Error: Role must be user|mod|admin|owner".yellow().to_string())?;
                return Ok(CommandResult::Handled);
            }
        };

        room_guard.roles.colors.insert(role_key.to_string(), hex_with_hash.clone());
        drop(room_guard);

        if let Err(e) = save_rooms_to_disk(&rooms_map) {
            send_error_locked(&mut c, &format!("Failed to save rooms: {e}"))?;
            return Ok(CommandResult::Handled);
        }

        use std::io::Write;
        writeln!(c.stream, "{} {}", format!("Color for {role_key} role changed to").green(), hex_with_hash.clone().truecolor_from_hex(&hex_with_hash))?;
        c.stream.flush()?;
    }

    let _ = crate::backend::command_utils::sync_room_members(rooms, clients, pubkeys, room);
    let _ = broadcast_user_list(clients, rooms, room);
    Ok(CommandResult::Handled)
}
