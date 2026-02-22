pub mod account;
pub mod rooms;
pub mod ignore;

use std::io;
use std::sync::{Arc, Mutex};
use colored::*;

use crate::backend::parser::Command;
use crate::backend::command_utils::help_msg_loggedin;
use crate::shared::types::{Client, Clients, PublicKeys, Rooms};
use crate::shared::utils::{lock_client, lock_clients, send_message, send_error, send_success};
use crate::backend::dispatcher::CommandResult;

pub fn loggedin_command(cmd: Command, client: Arc<Mutex<Client>>, clients: &Clients, rooms: &Rooms, username: &String, pubkeys: &PublicKeys) -> io::Result<CommandResult> {
    match cmd {
        Command::Help => {
            send_message(&client, &format!("{}{}", help_msg_loggedin().bright_blue(), "\x1b[0m"))?;
            Ok(CommandResult::Handled)
        }

        Command::Ping { start_time }=> {
            if let Some(start_ms) = start_time {
                send_message(&client, &format!("/PONG {start_ms}"))?;
            }
            Ok(CommandResult::Handled)
        }

        Command::PubKey { pubkey } => {
            let mut map = match pubkeys.lock() {
                Ok(m) => m,
                Err(_) => {
                    send_error(&client, "failed to lock pubkeys")?;
                    return Ok(CommandResult::Handled);
                }
            };
            if map.contains_key(username) {
                send_error(&client, "Public key already registered for this user")?;
                return Ok(CommandResult::Handled);
            }
            map.insert(username.clone(), pubkey.clone());
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
                let mut clients = lock_clients(clients)?;
                clients.remove(&addr);
            }
            let c_guard = lock_client(&client)?;
            send_success(&client, "Exiting...")?;
            use std::net::Shutdown;
            c_guard.stream.shutdown(Shutdown::Both)?;
            Ok(CommandResult::Stop)
        }

        Command::IgnoreList => ignore::handle_ignore_list(client),
        Command::IgnoreAdd { users } => ignore::handle_ignore_add(client, username, &users),
        Command::IgnoreRemove { users } => ignore::handle_ignore_remove(client, username, &users),

        Command::Leave | Command::Status | Command::AFK | Command::Announce { .. } | Command::Seen { .. } | Command::DM { .. } | Command::Me { .. } |
        Command::SuperUsers | Command::SuperRename { .. } | Command::SuperExport { .. } | Command::SuperWhitelist | Command::SuperWhitelistToggle | Command::SuperWhitelistAdd { .. } | Command::SuperWhitelistRemove { .. } | Command::SuperLimit | Command::SuperLimitRate { .. } | Command::SuperLimitSession { .. } | Command::SuperRoles | Command::SuperRolesAdd { .. } | Command::SuperRolesRevoke { .. } | Command::SuperRolesAssign { .. } | Command::SuperRolesRecolor { .. } |
        Command::Users | Command::UsersRename { .. } | Command::UsersRecolor { .. } | Command::UsersHide |
        Command::ModInfo | Command::ModKick { .. } | Command::ModMute { .. } | Command::ModUnmute { .. } | Command::ModBan { .. } | Command::ModUnban { .. } => {
            send_message(&client, &"Must be in a room to perform this command".yellow().to_string())?;
            Ok(CommandResult::Handled)
        }

        Command::AccountRegister { .. } | Command::AccountLogin { .. } => {
            send_error(&client, "You are already logged in")?;
            Ok(CommandResult::Handled)
        }

        Command::AccountLogout => account::handle_account_logout(client, username, pubkeys),
        Command::AccountEditUsername { username: new_username } => account::handle_account_edit_username(client, username, &new_username),
        Command::AccountEditPassword { current_password, new_password } => account::handle_account_edit_password(client, username, &current_password, &new_password),
        Command::AccountImport { filename } => account::handle_account_import(client, &filename),
        Command::AccountExport { filename } => account::handle_account_export(client, username, &filename),
        Command::AccountDelete { force } => account::handle_account_delete(client, username, pubkeys, force),
        Command::Account => account::handle_account(client, username),

        Command::RoomList => rooms::handle_room_list(client, rooms, username),
        Command::RoomCreate { name, whitelist } => rooms::handle_room_create(client, rooms, username, &name, whitelist),
        Command::RoomJoin { name } => rooms::handle_room_join(client, clients, rooms, pubkeys, username, &name),
        Command::RoomImport { filename } => rooms::handle_room_import(client, rooms, &filename),
        Command::RoomDelete { name, force } => rooms::handle_room_delete(client, rooms, username, &name, force),

        Command::InvalidSyntax {err_msg } => {
            send_message(&client, &err_msg)?;
            Ok(CommandResult::Handled)
        }

        Command::Unavailable => {
            send_error(&client, "Command not available, use /help to see available commands")?;
            Ok(CommandResult::Handled)
        }
    }
}
