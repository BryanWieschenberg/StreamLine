use std::collections::HashMap;
use std::io::{BufReader, BufRead, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;

use colored::Colorize;

mod commands;
use crate::commands::parser::{Command, parse_command};
use crate::commands::dispatcher::{dispatch_command, CommandResult};

mod state;
use crate::state::types::{Clients, Client, ClientState, Rooms, Room};

mod utils;
use crate::utils::{lock_clients, lock_client, lock_rooms, lock_room};

pub fn check_rate_limit(client_arc: &std::sync::Arc<std::sync::Mutex<crate::state::types::Client>>, rooms: &Rooms) -> std::io::Result<bool> {
    let now = Instant::now();

    let mut c = lock_client(client_arc)?;
    if let ClientState::InRoom { room: rname, msg_timestamps, .. } = &mut c.state {
        let rooms_map = lock_rooms(rooms)?;
        let rate = match rooms_map.get(rname) {
            Some(room_arc) => match lock_room(room_arc) {
                Ok(room) => room.msg_rate,
                Err(_) => {
                    writeln!(c.stream, "{}", "Error: could not lock room".red())?;
                    return Ok(false);
                }
            },
            None => {
                writeln!(c.stream, "{}", "Error: room not found".yellow())?;
                return Ok(false);
            }
        };

        while let Some(ts) = msg_timestamps.front() {
            if now.duration_since(*ts).as_secs() >= 5 {
                msg_timestamps.pop_front();
            }
            else {
                break;
            }
        }

        if rate > 0 && msg_timestamps.len() as u8 >= rate {
            writeln!(c.stream, "{}", "Rate limit exceeded, slow down your messages!".yellow())?;
            return Ok(false);
        }

        msg_timestamps.push_back(now);
    }
    Ok(true)
}

fn broadcast_message(msg: &str, sender_arc: &Arc<Mutex<Client>>, clients: &Clients) -> std::io::Result<()> {
    let locked_clients = lock_clients(clients)?;
    
    let sender = lock_client(sender_arc)?;
    let (sender_username, sender_room) = match &sender.state {
        ClientState::InRoom { username, room, .. } => (username.clone(), room.clone()),
        _ => return Ok(()), // Sender not in room, don't broadcast
    };
    let sender_addr = sender.addr;
    drop(sender);
    
    // Iterate thru all clients
    for (addr, client_arc) in locked_clients.iter() {
        if addr == &sender_addr {
            continue; // Skip sender
        }
        
        if let Ok(mut client) = lock_client(client_arc) {
            if let ClientState::InRoom { room, .. } = &client.state {
                if room == &sender_room {
                    writeln!(client.stream, "{sender_username}: {msg}")?;
                }
            }
        }
    }
    
    Ok(())
}

// Handler for each client connection on a separate thread
fn handle_client(stream: TcpStream, peer: SocketAddr, clients: Clients, rooms: Rooms) -> std::io::Result<()> {
    let reader = BufReader::new(stream.try_clone()?);

    let client_arc = Arc::new(Mutex::new(Client {
        stream: stream.try_clone()?,
        addr: peer,
        state: ClientState::Guest,
        ignore_list: Vec::new(),
    }));

    // Insert the new client into the clients map
    {
        let mut locked = lock_clients(&clients)?;
        locked.insert(peer, Arc::clone(&client_arc));
    }

    println!("Guest ({peer}) connected");

    // Read messages from the client and broadcast them to all other clients
    for line in reader.lines() {
        match line {
            Ok(msg) => {
                let msg = msg.trim().to_string();

                if msg.is_empty() { continue };

                if msg.starts_with("/") {
                    let command: Command = parse_command(&msg);
                    
                    match dispatch_command(command, Arc::clone(&client_arc), &clients, &rooms)? {
                        CommandResult::Handled => continue,
                        CommandResult::Stop => break
                    }
                }

                let allowed = check_rate_limit(&client_arc, &rooms)?;
                if !allowed {
                    continue;
                }

                let mut sender = lock_client(&client_arc)?;
                
                if let ClientState::InRoom { .. } = &sender.state {
                    drop(sender);                                   // ← release the mutex
                    if let Err(e) = broadcast_message(&msg, &client_arc, &clients) {
                        eprintln!("Error broadcasting message from {peer}: {e}");
                        break;
                    }
                }
                else if let ClientState::LoggedIn { .. } = &sender.state {
                    writeln!(sender.stream, "{}", "You must join a room to chat".yellow())?;
                }
                else {
                    writeln!(sender.stream, "{}", "You must log in to chat".yellow())?;
                }
            }
            Err(e) => {
                eprintln!("Error reading line from {peer}: {e}");
                break;
            }
        };
    }

    // Disconnection cleanup
    let removed = {
        let mut locked = lock_clients(&clients)?;
        locked.remove(&peer)
    };

    if let Some(client_arc) = removed {
        let client = lock_client(&client_arc)?;

        match &client.state {
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

    let room_file: std::fs::File = std::fs::File::open("data/rooms.json")?;
    let room_reader: BufReader<std::fs::File> = BufReader::new(room_file);
    let parsed_rooms: HashMap<String, Room> = serde_json::from_reader(room_reader)?;

    // Convert into Arc<Mutex<Room>>
    let mut rooms_map: HashMap<String, Arc<Mutex<Room>>> = HashMap::new();
    for (name, room) in parsed_rooms {
        rooms_map.insert(name, Arc::new(Mutex::new(room)));
    }

    let rooms: Rooms = Arc::new(Mutex::new(rooms_map));
    
    println!("Server listening on port {port}");

    // Main loop to accept incoming connections
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let peer = stream.peer_addr()?;

                // Clone clients Arc and use the clone in the thread
                let clients_ref = Arc::clone(&clients);
                let rooms_ref = Arc::clone(&rooms);
                thread::Builder::new()
                    .name(format!("client-{peer}"))
                    .spawn(move || {
                        if let Err(e) = handle_client(stream, peer, clients_ref, rooms_ref) {
                            eprintln!("Thread for {peer} exited with error: {e}");
                        }
                    })?;
            }
            Err(e) => {
                eprintln!("Failed to accept connection: {e}");
                continue;
            }
        }
    }
    
    Ok(())
}
