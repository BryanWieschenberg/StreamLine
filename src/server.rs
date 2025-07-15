use std::collections::HashMap;
use std::io::{BufReader, BufRead, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use colored::Colorize;

mod commands;
use crate::commands::parser::{Command, parse_command};
use crate::commands::dispatcher::{dispatch_command, CommandResult};

mod state;
// use crate::state::{types::*, manager::*, default::*};

use crate::state::types::{Clients, Client, ClientState};

// Handler for each client connection on a separate thread
fn handle_client(stream: TcpStream, peer: SocketAddr, clients: Clients) -> std::io::Result<()> {
    let reader = BufReader::new(stream.try_clone()?);

    let client = Client {
        stream: stream.try_clone()?,
        addr: peer,
        state: ClientState::Guest
    };

    {
        let mut locked = match clients.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                eprintln!("Failed to lock clients: {poisoned}");
                return Err(std::io::Error::new(std::io::ErrorKind::Other, "Failed to lock clients"));
            }
        };
        locked.insert(peer, client);
    }

    // Read messages from the client and broadcast them to all other clients
    for line in reader.lines() {
        let msg = match line {
            Ok(msg) => {
                let msg = msg.trim().to_string();

                if msg.starts_with("/") {
                    let command: Command = parse_command(&msg);
                    
                    let mut locked = match clients.lock() {
                        Ok(guard) => guard,
                        Err(poisoned) => {
                            eprintln!("Failed to lock clients: {poisoned}");
                            return Err(std::io::Error::new(std::io::ErrorKind::Other, "Lock poisoned"));
                        }
                    };
                    let client = match locked.get_mut(&peer) {
                        Some(client) => client,
                        None => {
                            eprintln!("Client not found: {peer}");
                            return Err(std::io::Error::new(std::io::ErrorKind::Other, "Client not found"));
                        }
                    };

                    match dispatch_command(command, client)? {
                        CommandResult::Handled => continue,
                        CommandResult::Stop => break
                    }
                }

                let mut locked = match clients.lock() {
                    Ok(guard) => guard,
                    Err(poisoned) => {
                        eprintln!("Failed to lock clients: {poisoned}");
                        return Err(std::io::Error::new(std::io::ErrorKind::Other, "Lock poisoned"));
                    }
                };
                let sender = match locked.get_mut(&peer) {
                    Some(sender) => sender,
                    None => {
                        eprintln!("Sender not found: {peer}");
                        return Err(std::io::Error::new(std::io::ErrorKind::Other, "Sender not found"));
                    }
                };

                match &sender.state {
                    ClientState::InRoom { .. } => msg,
                    ClientState::LoggedIn { .. } => {
                        let _ = writeln!(sender.stream, "{}", "You must join a room to chat".yellow());
                        continue;
                    }
                    ClientState::Guest => {
                        let _ = writeln!(sender.stream, "{}", "You must log in to chat".yellow());
                        continue;
                    }
                }
            }
            Err(e) => {
                eprintln!("Error reading line from {peer}: {e}");
                break;
            }
        };

        let locked = match clients.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                eprintln!("Failed to lock clients: {poisoned}");
                return Err(std::io::Error::new(std::io::ErrorKind::Other, "Lock poisoned"));
            }
        };
        let sender = match locked.get(&peer) {
            Some(sender) => sender,
            None => {
                eprintln!("Sender not found: {peer}");
                return Err(std::io::Error::new(std::io::ErrorKind::Other, "Sender not found"));
            }
        };

        if let ClientState::InRoom { username, room } = &sender.state {
            let mut locked = match clients.lock() {
                Ok(guard) => guard,
                Err(poisoned) => {
                    eprintln!("Failed to lock clients: {poisoned}");
                    return Err(std::io::Error::new(std::io::ErrorKind::Other, "Lock poisoned"));
                }
            };
            for (addr, client) in locked.iter_mut() {
                if let ClientState::InRoom { room: r2, .. } = &client.state {
                    if r2 == room && addr != &peer {
                        let _ = writeln!(client.stream, "{username}: {msg}");
                    }
                }
            }
        }
    }

    // Disconnection cleanup
    let mut locked = match clients.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            eprintln!("Failed to lock clients: {poisoned}");
            return Err(std::io::Error::new(std::io::ErrorKind::Other, "Lock poisoned"));
        }
    };
    if let Some(client) = locked.remove(&peer) {
        match client.state {
            ClientState::Guest => println!("Guest ({peer}) disconnected"),
            ClientState::LoggedIn { username } => println!("{username} ({peer}) disconnected"),
            ClientState::InRoom { username, .. } => println!("{username} ({peer}) disconnected"),
        }
    }

    Ok(())
}

// Main function to set up the TCP server and handle incoming connections
fn main() -> std::io::Result<()> {
    let port = 8080;
    let listener = TcpListener::bind(format!("127.0.0.1:{port}"))?;
    let clients: Clients = Arc::new(Mutex::new(HashMap::new()));
    println!("Server listening on port {port}");

    // Main loop to accept incoming connections
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let peer = stream.peer_addr()?;

                // Clone clients Arc and use the clone in the thread
                let clients_ref = Arc::clone(&clients);
                thread::spawn(move || handle_client(stream, peer, clients_ref));
            }
            Err(e) => {
                eprintln!("Failed to accept connection: {e}");
                continue;
            }
        }
    }
    
    Ok(())
}
