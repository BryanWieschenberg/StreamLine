use std::io::{BufReader, BufRead, Write};
use std::sync::{Arc, Mutex};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::thread;
// use std::io::{BufRead, BufReader, Write};

// Lockable vector of connected TCP clients safely shared across threads
type Clients = Arc<Mutex<Vec<TcpStream>>>;

// Handler for each client connection on a separate thread
fn handle_client(stream: TcpStream, peer: SocketAddr, clients: Clients) -> std::io::Result<()> {
    let reader = BufReader::new(stream);

    // Read messages from the client and broadcast them to all other clients
    for line in reader.lines() {
        let msg = match line {
            Ok(msg) => {
                println!("{peer}: {msg}");
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
        locked.retain(|s| s.peer_addr().is_ok());

        // Broadcast the message to all other clients
        for client in locked.iter_mut() {
            if client.peer_addr()? != peer {
                let _ = writeln!(client, "{peer}: {msg}");
            }
        }
    }

    Ok(())
}

// Main function to set up the TCP server and handle incoming connections
fn main() -> std::io::Result<()> {
    let port = 8080;
    let listener = TcpListener::bind(format!("127.0.0.1:{port}"))?;
    let clients: Clients = Arc::new(Mutex::new(Vec::new()));
    println!("Server listening on port {port}");

    // Main loop to accept incoming connections
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let peer = stream.peer_addr()?;
                println!("Client connected: {peer}");

                // Push stream clone into the clients vector since the original will be moved into the thread
                {
                    let mut locked = match clients.lock() {
                        Ok(guard) => guard,
                        Err(poisoned) => {
                            eprintln!("Failed to lock clients: {poisoned}");
                            continue;
                        }
                    };
                    locked.push(stream.try_clone()?);
                }

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
