use std::collections::HashMap;
use std::io::{BufReader, BufRead, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;

mod state;
// use crate::state::{types::*, manager::*, default::*};

mod commands;
// use crate::commands::parser::Command;
// use crate::commands::dispatcher::{dispatch_command, CommandResult};

use crate::state::types::{Client, Clients};

// Handler for each client connection on a separate thread
fn handle_client(stream: TcpStream, peer: SocketAddr, clients: Clients) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    
    // TODO: Right now, we assume the first line is /account register or /account login, but it isn't guarenteed
    let mut username = String::new();
    reader.read_line(&mut username)?;
    let username = username.trim().to_string();
    
    println!("Client connected: {username} ({peer})");

    {
        let mut locked = match clients.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                eprintln!("Failed to lock clients: {poisoned}");
                return Err(std::io::Error::new(std::io::ErrorKind::Other, "Lock poisoned"));
            }
        };
        locked.insert(username.clone(), Client {
            stream: stream.try_clone()?,
            username: username.clone(),
            current_room: String::new(),
        });
    }

    // Read messages from the client and broadcast them to all other clients
    for line in reader.lines() {
        let msg = match line {
            Ok(msg) => {
                let msg = msg.trim().to_string();
                if msg == "/exit" {
                    break;
                }
                println!("{username} ({peer}): {msg}");
                msg
            }
            Err(e) => {
                eprintln!("Error reading line from {peer}: {e}");
                break;
            }
        };

        // Remove any clients that have disconnected before broadcasting
        let mut locked = match clients.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                eprintln!("Failed to lock clients: {poisoned}");
                break;
            }
        };
        locked.retain(|_name, client| client.stream.peer_addr().is_ok());

        // Broadcast the message to all other clients
        for (_name, client) in locked.iter_mut() {
            if client.stream.peer_addr()? != peer {
                let _ = writeln!(client.stream, "{username}: {msg}");
            }
        }
    }

    println!("{username} ({peer}) disconnected");

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
