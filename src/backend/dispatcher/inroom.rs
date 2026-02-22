pub mod moderation;
pub mod superuser;
pub mod superuser_roles;
pub mod user;
pub mod messaging;

use std::io::{self, Write};
use std::sync::{Arc, Mutex};
use colored::*;

use crate::backend::parser::Command;
use crate::backend::command_utils::{help_msg_inroom, has_permission, unix_timestamp, sync_room_members};
use crate::shared::types::{Client, ClientState, Clients, PublicKeys, Rooms};
use crate::shared::utils::{lock_client, lock_clients, lock_rooms, lock_room};
use super::CommandResult;

pub fn inroom_command(cmd: Command, client: Arc<Mutex<Client>>, clients: &Clients, rooms: &Rooms, username: &String, room: &String, pubkeys: &PublicKeys) -> io::Result<CommandResult> {
    if !has_permission(&cmd, client.clone(), rooms, username, room)? {
        return Ok(CommandResult::Handled);
    }

    match cmd {
        Command::Help => {
            let role_cmds: Vec<String> = {
                let _c = lock_client(&client)?;
                let rooms_map = lock_rooms(rooms)?;
                let room_arc = match rooms_map.get(room) {
                    Some(arc) => arc,
                    None => {
                        let mut c = lock_client(&client)?;
                        writeln!(c.stream, "{}", "Error: Room not found".red())?;
                        return Ok(CommandResult::Handled);
                    }
                };
                let room_guard = lock_room(room_arc)?;
                let role = match room_guard.users.get(username) {
                    Some(u) => u.role.as_str(),
                    None => "user",
                };
                match role {
                    "moderator" => room_guard.roles.moderator.clone(),
                    "user" => room_guard.roles.user.clone(),
                    "admin" | "owner" => vec![
                        "afk".to_string(), "announce".to_string(), "seen".to_string(), "msg".to_string(),
                        "me".to_string(), "super".to_string(), "user".to_string(), "mod".to_string()
                    ],
                    _ => Vec::new(),
                }
            };

            let mut c = lock_client(&client)?;
            let role_cmds_refs: Vec<&str> = role_cmds.iter().map(|s| s.as_str()).collect();
            writeln!(c.stream, "{}", help_msg_inroom(role_cmds_refs).bright_blue())?;
            Ok(CommandResult::Handled)
        }
        Command::Ping { start_time }=> {
            let mut c = lock_client(&client)?;
            if let Some(start_ms) = start_time {
                writeln!(c.stream, "/PONG {start_ms}")?;
            }
            Ok(CommandResult::Handled)
        }
        Command::PubKey { .. } => {
            let mut c = lock_client(&client)?;
            writeln!(c.stream, "{}", "Public keys are handled automatically when logging in".yellow())?;
            Ok(CommandResult::Handled)
        }
        Command::Quit => {
            {
                let mut pubkeys_map = match pubkeys.lock() {
                    Ok(g) => g,
                    Err(_) => return Ok(CommandResult::Handled),
                };
                pubkeys_map.remove(username);
            }
            let addr = {
                let c = lock_client(&client)?;
                c.addr
            };
            {
                let rooms_map = lock_rooms(rooms)?;
                if let Some(room_arc) = rooms_map.get(room) {
                    if let Ok(mut room_guard) = room_arc.lock() {
                        room_guard.online_users.retain(|u| u != username);
                    }
                }
            }
            let _ = sync_room_members(rooms, clients, pubkeys, room);
            if let Err(e) = unix_timestamp(rooms, room, username) {
                eprintln!("Error updating last_seen for {username} in {room}: {e}");
            }
            {
                let mut clients_guard = lock_clients(clients)?;
                clients_guard.remove(&addr);
            }
            
            let c_guard = lock_client(&client)?;
            crate::shared::utils::send_success(&client, "Exiting...")?;
            c_guard.stream.shutdown(std::net::Shutdown::Both)?;
            Ok(CommandResult::Stop)
        }
        Command::Leave => {
            {
                let rooms_map = lock_rooms(rooms)?;
                if let Some(room_arc) = rooms_map.get(room) {
                    if let Ok(mut r) = room_arc.lock() {
                        r.online_users.retain(|u| u != username);
                    }
                }
            }
            let _ = sync_room_members(rooms, clients, pubkeys, room);
            if let Err(e) = unix_timestamp(rooms, room, username) {
                eprintln!("Error updating last_seen for {username} in {room}: {e}");
            }
            let mut c = lock_client(&client)?;
            c.state = ClientState::LoggedIn {
                username: username.clone()
            };
            writeln!(c.stream, "{}", format!("/LOBBY_STATE"))?;
            writeln!(c.stream, "{}", format!("You have left {room}").green())?;
            Ok(CommandResult::Handled)
        }
        Command::Status => {
            let rooms_map = lock_rooms(rooms)?;
            let room_arc = match rooms_map.get(room) {
                Some(r) => Arc::clone(r),
                None => {
                    let mut c = lock_client(&client)?;
                    writeln!(c.stream, "{}", format!("Room {room} not found").yellow())?;
                    return Ok(CommandResult::Handled);
                }
            };
            let room_guard = lock_room(&room_arc)?;
            let role = room_guard.users.get(username).map(|u| u.role.clone()).unwrap_or_else(|| "user".to_string());
            let online: Vec<&String> = room_guard.online_users.iter().collect();
            let mut c = lock_client(&client)?;
            writeln!(c.stream, "{}", format!("Room: {} | Role: {} | Online: {}", room, role, online.len()).cyan())?;
            Ok(CommandResult::Handled)
        }
        Command::IgnoreList | Command::IgnoreAdd { .. } | Command::IgnoreRemove { .. } => {
            crate::backend::dispatcher::loggedin::loggedin_command(cmd, client, clients, rooms, username, pubkeys)
        }
        Command::AFK => messaging::handle_afk(client, rooms, username, room),
        Command::DM { recipient, message } => messaging::handle_dm(client, clients, rooms, username, room, &recipient, &message),
        Command::Me { action } => messaging::handle_me(client, clients, rooms, username, room, &action),
        Command::Seen { username: target } => messaging::handle_seen(client, rooms, room, &target),
        Command::Announce { message } => messaging::handle_announce(client, clients, rooms, username, room, &message),
        Command::AccountRegister { .. } | Command::AccountLogin { .. } | Command::Account | Command::AccountDelete { .. } | Command::AccountEditPassword { .. } | Command::AccountEditUsername { .. } | Command::AccountExport { .. } | Command::AccountImport { .. } | Command::RoomList | Command::AccountLogout => {
            let mut c = lock_client(&client)?;
            writeln!(c.stream, "{}", "Cannot use this command while in a room. Leave the room first.".yellow())?;
            Ok(CommandResult::Handled)
        }
        Command::SuperUsers => superuser::handle_super_users(client, clients, rooms, room),
        Command::SuperRename { name: new_name } => superuser::handle_super_rename(client, clients, rooms, room, &new_name),
        Command::SuperExport { filename } => superuser::handle_super_export(client, rooms, room, &filename),
        Command::SuperWhitelist => superuser::handle_super_whitelist(client, rooms, room),
        Command::SuperWhitelistToggle => superuser::handle_super_whitelist_toggle(client, rooms, room),
        Command::SuperWhitelistAdd { users } => superuser::handle_super_whitelist_add(client, rooms, room, &users),
        Command::SuperWhitelistRemove { users } => superuser::handle_super_whitelist_remove(client, rooms, room, &users),
        Command::SuperLimit => superuser::handle_super_limit(client, rooms, room),
        Command::SuperLimitRate { limit } => superuser::handle_super_limit_rate(client, rooms, room, limit),
        Command::SuperLimitSession { limit } => superuser::handle_super_limit_session(client, rooms, room, limit),
        Command::SuperRoles => superuser_roles::handle_super_roles(client, rooms, room),
        Command::SuperRolesAdd { role, commands } => superuser_roles::handle_super_roles_add(client, rooms, room, &role, &commands),
        Command::SuperRolesRevoke { role, commands } => superuser_roles::handle_super_roles_revoke(client, rooms, room, &role, &commands),
        Command::SuperRolesAssign { role, users } => superuser_roles::handle_super_roles_assign(client, rooms, room, &role, &users),
        Command::SuperRolesRecolor { role, color } => superuser_roles::handle_super_roles_recolor(client, rooms, room, &role, &color),
        Command::Users => user::handle_users(client, rooms, room),
        Command::UsersRename { name } => user::handle_users_rename(client, rooms, room, username, &name),
        Command::UsersRecolor { color } => user::handle_users_recolor(client, rooms, room, username, &color),
        Command::UsersHide => user::handle_users_hide(client, clients, rooms, pubkeys, username, room),
        Command::ModInfo => moderation::handle_mod_info(client, rooms, room),
        Command::ModKick { username: target, reason } => moderation::handle_mod_kick(client, clients, rooms, pubkeys, username, room, &target, reason),
        Command::ModBan { username: target, duration, reason } => moderation::handle_mod_ban(client, clients, rooms, pubkeys, username, room, &target, duration, reason),
        Command::ModUnban { username: target } => moderation::handle_mod_unban(client, rooms, username, room, &target),
        Command::ModMute { username: target, duration, reason } => moderation::handle_mod_mute(client, clients, rooms, username, room, &target, duration, reason),
        Command::ModUnmute { username: target } => moderation::handle_mod_unmute(client, clients, rooms, username, room, &target),
        Command::RoomJoin { .. } | Command::RoomCreate { .. } | Command::RoomDelete { .. } | Command::RoomImport { .. } => {
            let mut c = lock_client(&client)?;
            writeln!(c.stream, "{}", "You are already in a room. Use /leave first to switch rooms.".yellow())?;
            Ok(CommandResult::Handled)
        }
        Command::InvalidSyntax { err_msg } => {
            let mut c = lock_client(&client)?;
            writeln!(c.stream, "{}", err_msg)?;
            Ok(CommandResult::Handled)
        }
        Command::Unavailable => {
            let mut c = lock_client(&client)?;
            writeln!(c.stream, "{}", "Command not available, use /help to see available commands".red())?;
            Ok(CommandResult::Handled)
        }
    }
}
