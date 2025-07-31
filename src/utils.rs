use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, MutexGuard};
use std::io;
use std::io::{Write};

use crate::types::{Clients, Client, ClientState, Rooms, Room, USERS_LOCK, ROOMS_LOCK};

pub fn broadcast_message(clients: &Clients, room_name: &str, sender: &str, msg: &str, include_sender: bool, bypass_ignores: bool) -> io::Result<()> {
    let client_arcs: Vec<Arc<Mutex<Client>>> =
        match lock_clients(clients) {
            Ok(map) => map.values().cloned().collect(),
            Err(_)  => return Ok(()),
        };

    for arc in client_arcs {
        let mut c = lock_client(&arc)?;
        
        if let ClientState::InRoom { username, room, .. } = &c.state {
            if room != room_name { continue; }

            if !include_sender && username == sender {
                continue;
            }

            if !bypass_ignores && c.ignore_list.contains(&sender.to_string()) {
                continue;
            }

            writeln!(c.stream, "{msg}")?;
        }
    }
    Ok(())
}

pub fn lock_clients(clients: &Clients) -> std::io::Result<std::sync::MutexGuard<'_, HashMap<SocketAddr, Arc<Mutex<Client>>>>> {
    clients.lock().map_err(|e| {
        eprintln!("Failed to lock clients: {e}");
        std::io::Error::new(std::io::ErrorKind::Other, "Lock poisoned")
    })
}

pub fn lock_client(client_arc: &Arc<Mutex<Client>>) -> std::io::Result<std::sync::MutexGuard<'_, Client>> {
    client_arc.lock().map_err(|e| {
        eprintln!("Failed to lock client: {e}");
        std::io::Error::new(std::io::ErrorKind::Other, "Lock poisoned")
    })
}

pub fn lock_rooms(rooms: &Rooms) -> std::io::Result<std::sync::MutexGuard<'_, HashMap<String, Arc<Mutex<Room>>>>> {
    rooms.lock().map_err(|e| {
        eprintln!("Failed to lock rooms: {e}");
        std::io::Error::new(std::io::ErrorKind::Other, "Lock poisoned")
    })
}

pub fn lock_room(room_arc: &Arc<Mutex<Room>>) -> std::io::Result<std::sync::MutexGuard<'_, Room>> {
    room_arc.lock().map_err(|e| {
        eprintln!("Failed to lock room: {e}");
        std::io::Error::new(std::io::ErrorKind::Other, "Lock poisoned")
    })
}

pub fn lock_users_storage<'a>() -> io::Result<MutexGuard<'a, ()>> {
    USERS_LOCK.lock().map_err(|e| {
        eprintln!("Failed to lock users: {e}");
        io::Error::new(io::ErrorKind::Other, "Error: Could not acquire user lock")
    })
}

pub fn lock_rooms_storage<'a>() -> io::Result<MutexGuard<'a, ()>> {
    ROOMS_LOCK.lock().map_err(|e| {
        eprintln!("Failed to lock rooms: {e}");
        io::Error::new(io::ErrorKind::Other, "Error: Could not acquire room lock")
    })
}
