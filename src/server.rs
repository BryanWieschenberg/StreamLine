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
use crate::state::types::{Clients, Client, ClientState};

mod utils;
use crate::utils::{lock_clients, lock_client};

// Handler for each client connection on a separate thread
fn handle_client(stream: TcpStream, peer: SocketAddr, clients: Clients) -> std::io::Result<()> {
    let reader = BufReader::new(stream.try_clone()?);

    let client_arc = Arc::new(Mutex::new(Client {
        stream: stream.try_clone()?,
        addr: peer,
        state: ClientState::Guest
    }));

    // Insert the new client into the clients map
    {
        let mut locked = lock_clients(&clients)?;
        locked.insert(peer, Arc::clone(&client_arc));
    }

    // Read messages from the client and broadcast them to all other clients
    for line in reader.lines() {
        let msg = match line {
            Ok(msg) => {
                let msg = msg.trim().to_string();

                if msg.starts_with("/") {
                    let command: Command = parse_command(&msg);
                    
                    let mut client = lock_client(&client_arc)?;
                    match dispatch_command(command, &mut *client)? {
                        CommandResult::Handled => continue,
                        CommandResult::Stop => break
                    }
                }

                let mut sender = lock_client(&client_arc)?;

                match &sender.state {
                    ClientState::InRoom { .. } => msg,
                    ClientState::LoggedIn { .. } => {
                        writeln!(sender.stream, "{}", "You must join a room to chat".yellow())?;
                        continue;
                    }
                    ClientState::Guest => {
                        writeln!(sender.stream, "{}", "You must log in to chat".yellow())?;
                        continue;
                    }
                }
            }
            Err(e) => {
                eprintln!("Error reading line from {peer}: {e}");
                break;
            }
        };

        let (username, room) = {
            let sender = lock_client(&client_arc)?;

            match &sender.state {
                ClientState::InRoom { username, room } => (username.clone(), room.clone()),
                _ => continue,
            }
        };

        let locked = lock_clients(&clients)?;

        for (addr, other_arc) in locked.iter() {
            if addr == &peer {
                continue;
            }

            let mut client = lock_client(&other_arc)?;

            if let ClientState::InRoom { room: room_recv, .. } = &client.state {
                if room_recv == &room {
                    writeln!(client.stream, "{username}: {msg}")?;
                }
            }
        }
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
    println!("Server listening on port {port}");

    // Main loop to accept incoming connections
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let peer = stream.peer_addr()?;

                // Clone clients Arc and use the clone in the thread
                let clients_ref = Arc::clone(&clients);
                thread::Builder::new()
                    .name(format!("client-{peer}"))
                    .spawn(move || {
                        if let Err(e) = handle_client(stream, peer, clients_ref) {
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
