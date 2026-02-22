use std::io::{self, Write};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use colored::*;

use crate::backend::command_utils::{unix_timestamp, parse_duration, sync_room_members};
use crate::shared::types::{Client, ClientState, Clients, RoomUser, Rooms, PublicKeys};
use crate::shared::utils::{lock_client, lock_clients, lock_room, lock_rooms, save_rooms_to_disk, send_message_locked, send_error_locked, send_success_locked};
use crate::backend::dispatcher::CommandResult;

pub fn role_rank(role: &str) -> u8 {
    match role {
        "owner" => 4,
        "admin" => 3,
        "moderator" => 2,
        _ => 1,
    }
}

pub fn handle_mod_info(client: Arc<Mutex<Client>>, rooms: &Rooms, room: &String) -> io::Result<CommandResult> {
    let mut c = lock_client(&client)?;
    let room_arc = {
        let rooms_map = lock_rooms(rooms)?;
        match rooms_map.get(room) {
            Some(r) => Arc::clone(r),
            None => {
                send_message_locked(&mut c, &"Room not found".yellow().to_string())?;
                return Ok(CommandResult::Handled);
            }
        }
    };

    let mut changed = false;
    let (banned, muted) = {
        let mut room_guard = lock_room(&room_arc)?;
        let now = match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(d) => d.as_secs(),
            Err(_) => 0,
        };

        let mut banned_vec = Vec::<String>::new();
        let mut muted_vec  = Vec::<String>::new();

        for (uname, rec) in room_guard.users.iter_mut() {
            if rec.banned {
                let still_banned = if rec.ban_length == 0 {
                    true
                } else {
                    let expire = rec.ban_stamp.saturating_add(rec.ban_length);
                    if now >= expire {
                        rec.banned = false;
                        rec.ban_stamp = 0;
                        rec.ban_length = 0;
                        rec.ban_reason.clear();
                        changed = true;
                        false
                    } else {
                        true
                    }
                };

                if still_banned {
                    let remaining = if rec.ban_length == 0 {
                        "Permanent".to_string()
                    } else {
                        let rem = rec.ban_stamp.saturating_add(rec.ban_length) - now;
                        let d = rem / 86_400;
                        let h = (rem % 86_400) / 3_600;
                        let m = (rem % 3_600) / 60;
                        let s = rem % 60;
                        format!("{d}d {h}h {m}m {s}s left")
                    };
                    let reason = if rec.ban_reason.is_empty() { "" } else { " - " };
                    banned_vec.push(format!("{uname} ({remaining}){reason}{}", rec.ban_reason));
                }
            }

            if rec.muted {
                let still_muted = if rec.mute_length == 0 {
                    true
                } else {
                    let expire = rec.mute_stamp.saturating_add(rec.mute_length);
                    if now >= expire {
                        rec.muted = false;
                        rec.mute_stamp = 0;
                        rec.mute_length = 0;
                        rec.mute_reason.clear();
                        changed = true;
                        false
                    } else {
                        true
                    }
                };

                if still_muted {
                    let remaining = if rec.mute_length == 0 {
                        "Permanent".to_string()
                    } else {
                        let rem = rec.mute_stamp.saturating_add(rec.mute_length) - now;
                        let d = rem / 86_400;
                        let h = (rem % 86_400) / 3_600;
                        let m = (rem % 3_600) / 60;
                        let s = rem % 60;
                        format!("{d}d {h}h {m}m {s}s left")
                    };
                    let reason = if rec.mute_reason.is_empty() { "" } else { " - " };
                    muted_vec.push(format!("{uname} ({remaining}){reason}{}", rec.mute_reason));
                }
            }
        }
        (banned_vec, muted_vec)
    };

    if changed {
        let res = {
            let rooms_map = lock_rooms(rooms)?;
            save_rooms_to_disk(&rooms_map)
        };
        if let Err(e) = res {
            send_error_locked(&mut c, &format!("Failed to save rooms: {e}"))?;
            return Ok(CommandResult::Handled);
        }
    }

    if banned.is_empty() && muted.is_empty() {
        send_success_locked(&mut c, "No users are currently banned or muted")?;
    } else {
        if !banned.is_empty() {
            send_success_locked(&mut c, "- Banned users -")?;
            for line in &banned {
                send_message_locked(&mut c, &format!("  > {line}"))?;
            }
        }
        if !muted.is_empty() {
            send_success_locked(&mut c, "- Muted users -")?;
            for line in &muted {
                send_message_locked(&mut c, &format!("  > {line}"))?;
            }
        }
    }

    Ok(CommandResult::Handled)
}

pub fn handle_mod_kick(client: Arc<Mutex<Client>>, clients: &Clients, rooms: &Rooms, pubkeys: &PublicKeys, username: &String, room: &String, target: &String, reason: String) -> io::Result<CommandResult> {
    let mut c = lock_client(&client)?;
    let room_arc = {
        let rooms_map = lock_rooms(rooms)?;
        match rooms_map.get(room) {
            Some(r) => Arc::clone(r),
            None => {
                send_message_locked(&mut c, &"Room not found".yellow().to_string())?;
                return Ok(CommandResult::Handled);
            }
        }
    };

    {
        let rg = lock_room(&room_arc)?;
        let caller_role = rg.users.get(username).map(|u| u.role.as_str()).unwrap_or("user");
        let target_role = rg.users.get(target).map(|u| u.role.as_str()).unwrap_or("user");
        if role_rank(caller_role) <= role_rank(target_role) {
            send_message_locked(&mut c, &"Error: Cannot kick a user with equal or higher privilege".yellow().to_string())?;
            return Ok(CommandResult::Handled);
        }
    }

    {
        let mut rg = lock_room(&room_arc)?;
        if !rg.online_users.contains(target) {
            send_message_locked(&mut c, &format!("{target} is not currently online").yellow().to_string())?;
            return Ok(CommandResult::Handled);
        }
        rg.online_users.retain(|u| u != target);
    }

    if let Err(e) = unix_timestamp(rooms, room, target) {
        eprintln!("Failed to update last_seen for {target} in {room}: {e}");
    }

    drop(c);
    let clients_map = lock_clients(clients)?;
    let mut kicked = false;
    for c_arc in clients_map.values() {
        let mut target_c = match c_arc.lock() {
            Ok(g) => g,
            Err(p) => {
                eprintln!("Poisoned client lock: {p}");
                continue;
            }
        };

        if let ClientState::InRoom { username: u, room: rnm, .. } = &target_c.state {
            if u == target && rnm == room {
                let msg = if reason.trim().is_empty() {
                    format!("You have been kicked from {room}")
                } else {
                    format!("You have been kicked from {room}: {reason}")
                };
                writeln!(target_c.stream, "{}", format!("/LOBBY_STATE"))?;
                writeln!(target_c.stream, "{}", msg.red())?;
                target_c.state = ClientState::LoggedIn { username: target.clone() };
                kicked = true;
                break;
            }
        }
    }
    drop(clients_map);

    let mut c = match lock_client(&client) {
        Ok(g) => g,
        Err(_) => return Ok(CommandResult::Handled),
    };

    {
        let rooms_map = lock_rooms(rooms)?;
        if let Err(e) = save_rooms_to_disk(&rooms_map) {
            send_error_locked(&mut c, &format!("Failed to save rooms: {e}"))?;
            return Ok(CommandResult::Handled);
        }
    }

    if kicked {
        if reason.trim().is_empty() {
            send_success_locked(&mut c, &format!("Kicked {target}"))?;
        } else {
            send_success_locked(&mut c, &format!("Kicked {target}: {reason}"))?;
        }
    } else {
        send_message_locked(&mut c, &format!("Failed to kick {target}").yellow().to_string())?;
    }
    
    drop(c);
    let _ = sync_room_members(rooms, clients, pubkeys, room);

    Ok(CommandResult::Handled)
}

pub fn handle_mod_ban(client: Arc<Mutex<Client>>, clients: &Clients, rooms: &Rooms, pubkeys: &PublicKeys, username: &String, room: &String, target: &String, duration: String, reason: String) -> io::Result<CommandResult> {
    let mut c = lock_client(&client)?;
    let room_arc = {
        let rooms_map = lock_rooms(rooms)?;
        match rooms_map.get(room) {
            Some(r) => Arc::clone(r),
            None => {
                send_message_locked(&mut c, &"Room not found".yellow().to_string())?;
                return Ok(CommandResult::Handled);
            }
        }
    };

    {
        let rg = lock_room(&room_arc)?;
        let caller_role = rg.users.get(username).map(|u| u.role.as_str()).unwrap_or("user");
        let target_role = rg.users.get(target).map(|u| u.role.as_str()).unwrap_or("user");
        if role_rank(caller_role) <= role_rank(target_role) {
            send_message_locked(&mut c, &"Error: Cannot ban a user with equal or higher privilege".yellow().to_string())?;
            return Ok(CommandResult::Handled);
        }
    }

    let ban_secs = match parse_duration(&duration) {
        Ok(v) => v,
        Err(e) => {
            send_message_locked(&mut c, &format!("Bad duration: {e}").yellow().to_string())?;
            return Ok(CommandResult::Handled);
        }
    };

    let now = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_secs(),
        Err(_) => 0,
    };

    {
        let mut rg = lock_room(&room_arc)?;

        let user_rec = rg.users.entry(target.clone()).or_insert(RoomUser {
            nick: "".to_string(),
            color: "".to_string(),
            role: "user".to_string(),
            hidden: false,
            last_seen: now,
            banned: false,
            ban_stamp: 0,
            ban_length: 0,
            ban_reason: "".to_string(),
            muted: false,
            mute_stamp: 0,
            mute_length: 0,
            mute_reason: "".to_string(),
        });

        user_rec.banned      = true;
        user_rec.ban_stamp   = now;
        user_rec.ban_length  = ban_secs;
        user_rec.ban_reason  = reason.clone();
        user_rec.last_seen   = now;

        rg.online_users.retain(|u| u != target);
    }

    let _ = unix_timestamp(rooms, room, target);

    drop(c);
    let clients_map = lock_clients(clients)?;
    let human_len = if ban_secs == 0 {
        "PERMANENT".to_string()
    } else {
        let mut rem = ban_secs;
        let d = rem / 86_400;
        rem %= 86_400;
        let h = rem / 3_600;
        rem %= 3_600;
        let m = rem / 60;
        let s = rem % 60;
        format!("{d}d {h}h {m}m {s}s")
    };

    for c_arc in clients_map.values() {
        let mut target_c = match c_arc.lock() {
            Ok(g) => g,
            Err(_) => continue,
        };
        if let ClientState::InRoom { username: u, room: rnm, .. } = &target_c.state {
            if u == target && rnm == room {
                let msg = if reason.trim().is_empty() {
                    format!("You have been banned from {room} ({human_len})")
                } else {
                    format!("You have been banned from {room} ({reason})\n> {human_len}")
                };
                writeln!(target_c.stream, "{}", format!("/LOBBY_STATE"))?;
                writeln!(target_c.stream, "{}", msg.red())?;
                target_c.state = ClientState::LoggedIn { username: target.clone() };
                break;
            }
        }
    }
    drop(clients_map);

    let mut c = match lock_client(&client) {
        Ok(g) => g,
        Err(_) => return Ok(CommandResult::Handled),
    };

    {
        let rooms_map = lock_rooms(rooms)?;
        if let Err(e) = save_rooms_to_disk(&rooms_map) {
            send_error_locked(&mut c, &format!("Failed to save rooms: {e}"))?;
            return Ok(CommandResult::Handled);
        }
    }

    let len_disp = if ban_secs == 0 {
        "PERMANENT".to_string()
    } else {
        human_len.to_string()
    };

    if reason.trim().is_empty() {
        send_success_locked(&mut c, &format!("Banned {target} ({len_disp})"))?;
    } else {
        send_success_locked(&mut c, &format!("Banned {target} ({len_disp}): {reason}"))?;
    }
    
    drop(c);
    let _ = sync_room_members(rooms, clients, pubkeys, room);

    Ok(CommandResult::Handled)
}

pub fn handle_mod_unban(client: Arc<Mutex<Client>>, rooms: &Rooms, _username: &String, room: &String, target: &String) -> io::Result<CommandResult> {
    let mut c = lock_client(&client)?;
    let room_arc = {
        let rooms_map = lock_rooms(rooms)?;
        match rooms_map.get(room) {
            Some(r) => Arc::clone(r),
            None => {
                send_message_locked(&mut c, &"Room not found".yellow().to_string())?;
                return Ok(CommandResult::Handled);
            }
        }
    };

    let mut actually_unbanned = false;
    {
        let mut rg = lock_room(&room_arc)?;

        if let Some(rec) = rg.users.get_mut(target) {
            if rec.banned {
                rec.banned = false;
                rec.ban_stamp = 0;
                rec.ban_length = 0;
                rec.ban_reason.clear();
                actually_unbanned = true;
            }
        }
    }

    if !actually_unbanned {
        send_message_locked(&mut c, &format!("{target} is not currently banned").yellow().to_string())?;
        return Ok(CommandResult::Handled);
    }

    {
        let rooms_map = lock_rooms(rooms)?;
        if let Err(e) = save_rooms_to_disk(&rooms_map) {
            send_error_locked(&mut c, &format!("Failed to save rooms: {e}"))?;
            return Ok(CommandResult::Handled);
        }
    }

    send_success_locked(&mut c, &format!("Unbanned {target}"))?;
    Ok(CommandResult::Handled)
}

pub fn handle_mod_mute(client: Arc<Mutex<Client>>, clients: &Clients, rooms: &Rooms, username: &String, room: &String, target: &String, duration: String, reason: String) -> io::Result<CommandResult> {
    let mut c = lock_client(&client)?;
    let room_arc = {
        let rooms_map = lock_rooms(rooms)?;
        match rooms_map.get(room) {
            Some(r) => Arc::clone(r),
            None => {
                send_message_locked(&mut c, &"Room not found".yellow().to_string())?;
                return Ok(CommandResult::Handled);
            }
        }
    };

    {
        let rg = lock_room(&room_arc)?;
        let caller_role = rg.users.get(username).map(|u| u.role.as_str()).unwrap_or("user");
        let target_role = rg.users.get(target).map(|u| u.role.as_str()).unwrap_or("user");
        if role_rank(caller_role) <= role_rank(target_role) {
            send_message_locked(&mut c, &"Error: Cannot mute a user with equal or higher privilege".yellow().to_string())?;
            return Ok(CommandResult::Handled);
        }
    }

    let mute_secs = match parse_duration(&duration) {
        Ok(v) => v,
        Err(e) => {
            send_message_locked(&mut c, &format!("Bad duration: {e}").yellow().to_string())?;
            return Ok(CommandResult::Handled);
        }
    };

    let now = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_secs(),
        Err(_) => 0,
    };

    {
        let mut rg = lock_room(&room_arc)?;
        let rec = rg.users.entry(target.clone()).or_insert(RoomUser {
            nick: "".into(),
            color: "".into(),
            role: "user".into(),
            hidden: false,
            last_seen: now,
            banned: false,
            ban_stamp: 0,
            ban_length: 0,
            ban_reason: "".into(),
            muted: false,
            mute_stamp: 0,
            mute_length: 0,
            mute_reason: "".into(),
        });

        rec.muted       = true;
        rec.mute_stamp  = now;
        rec.mute_length = mute_secs;
        rec.mute_reason = reason.clone();
        rec.last_seen   = now;
    }

    let _ = unix_timestamp(rooms, room, target);

    drop(c);
    let clients_map = lock_clients(clients)?;
    let human_len = if mute_secs == 0 {
        "PERMANENT".to_string()
    } else {
        let mut rem = mute_secs;
        let d = rem / 86_400;
        rem %= 86_400;
        let h = rem / 3_600;
        rem %= 3_600;
        let m = rem / 60;
        let s = rem % 60;
        format!("{d}d {h}h {m}m {s}s")
    };

    for c_arc in clients_map.values() {
        let mut target_c = match c_arc.lock() {
            Ok(g) => g,
            Err(_) => continue,
        };
        if let ClientState::InRoom { username: u, room: rnm, .. } = &target_c.state {
            if u == target && rnm == room {
                let msg = if reason.trim().is_empty() {
                    format!("You have been muted in {room} ({human_len})")
                } else {
                    format!("You have been muted in {room}: {reason}\n> {human_len}")
                };
                writeln!(target_c.stream, "{}", msg.red())?;
                break;
            }
        }
    }
    drop(clients_map);

    let mut c = match lock_client(&client) {
        Ok(g) => g,
        Err(_) => return Ok(CommandResult::Handled),
    };

    {
        let rooms_map = lock_rooms(rooms)?;
        if let Err(e) = save_rooms_to_disk(&rooms_map) {
            send_error_locked(&mut c, &format!("Failed to save rooms: {e}"))?;
            return Ok(CommandResult::Handled);
        }
    }

    let len_disp = if mute_secs == 0 {
        "PERMANENT".into()
    } else {
        human_len.to_string()
    };

    if reason.trim().is_empty() {
        send_success_locked(&mut c, &format!("Muted {target} ({len_disp})"))?;
    } else {
        send_success_locked(&mut c, &format!("Muted {target} ({len_disp}): {reason}"))?;
    }

    Ok(CommandResult::Handled)
}

pub fn handle_mod_unmute(client: Arc<Mutex<Client>>, clients: &Clients, rooms: &Rooms, _username: &String, room: &String, target: &String) -> io::Result<CommandResult> {
    let mut c = lock_client(&client)?;
    let room_arc = {
        let rooms_map = lock_rooms(rooms)?;
        match rooms_map.get(room) {
            Some(r) => Arc::clone(r),
            None => {
                send_message_locked(&mut c, &"Room not found".yellow().to_string())?;
                return Ok(CommandResult::Handled);
            }
        }
    };

    let mut unmuted_success = false;
    {
        let mut rg = lock_room(&room_arc)?;
        if let Some(rec) = rg.users.get_mut(target) {
            if rec.muted {
                rec.muted = false;
                rec.mute_stamp = 0;
                rec.mute_length = 0;
                rec.mute_reason.clear();
                unmuted_success = true;
            }
        }
    }

    if !unmuted_success {
        send_message_locked(&mut c, &format!("{target} is not currently muted").yellow().to_string())?;
        return Ok(CommandResult::Handled);
    }

    {
        let rooms_map = lock_rooms(rooms)?;
        if let Err(e) = save_rooms_to_disk(&rooms_map) {
            send_error_locked(&mut c, &format!("Failed to save rooms: {e}"))?;
            return Ok(CommandResult::Handled);
        }
    }

    drop(c);
    let clients_map = lock_clients(clients)?;
    for c_arc in clients_map.values() {
        let mut target_c = match c_arc.lock() {
            Ok(g) => g,
            Err(_) => continue,
        };
        if let ClientState::InRoom { username: u, room: rnm, .. } = &target_c.state {
            if u == target && rnm == room {
                writeln!(target_c.stream, "{}", "You have been unmuted".green())?;
                target_c.stream.flush()?;
                break;
            }
        }
    }
    drop(clients_map);

    let mut c = match lock_client(&client) {
        Ok(g) => g,
        Err(_) => return Ok(CommandResult::Handled),
    };

    send_success_locked(&mut c, &format!("Unmuted {target}"))?;
    Ok(CommandResult::Handled)
}
