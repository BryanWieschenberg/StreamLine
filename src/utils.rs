use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, MutexGuard};
use std::io;

use crate::state::types::{Clients, Client, Rooms, Room, USERS_LOCK, ROOMS_LOCK};

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

#[allow(dead_code)]
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
