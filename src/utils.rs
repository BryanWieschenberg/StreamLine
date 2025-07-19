use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, MutexGuard};
use std::io;

use crate::state::types::{Clients, Client, USERS_LOCK, ROOMS_LOCK};

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

pub fn lock_users<'a>() -> io::Result<MutexGuard<'a, ()>> {
    USERS_LOCK.lock().map_err(|e| {
        eprintln!("Failed to lock users: {e}");
        io::Error::new(io::ErrorKind::Other, "Error: Could not acquire user lock")
    })
}

#[allow(dead_code)]
pub fn lock_rooms<'a>() -> io::Result<MutexGuard<'a, ()>> {
    ROOMS_LOCK.lock().map_err(|e| {
        eprintln!("Failed to lock rooms: {e}");
        io::Error::new(io::ErrorKind::Other, "Error: Could not acquire room lock")
    })
}
