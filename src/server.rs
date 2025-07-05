use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use colored::*;

struct Client {
    stream: TcpStream,
    username: String,
}

type Clients = Arc<Mutex<Vec<Client>>>;

fn handle_client(stream: TcpStream, clients: Clients) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());

    let mut username = String::new();
    if reader.read_line(&mut username).is_err() {
        println!("Error reading username");
        return;
    }
    username = username.trim().to_string();

    let client = Client {
        stream: stream.try_clone().unwrap(),
        username: username.clone(),
    };

    {
        let mut locked = clients.lock().unwrap();
        locked.push(client);
    }

    // Main message loop
    for line in reader.lines() {
        let msg = match line {
            Ok(m) => m,
            Err(_) => break,
        };

        println!("{} says: {}", username.green(), msg);

        let mut locked = clients.lock().unwrap();
        locked.retain(|c| c.stream.peer_addr().is_ok());

        for client in locked.iter_mut() {
            if client.username != username {
                let _ = writeln!(client.stream, "{}: {}", username, msg);
            }
        }
    }

    println!("{} disconnected", username);
}

fn main() {
    let port = 9001;
    let listener = TcpListener::bind(format!("127.0.0.1:{}", port)).expect("Couldn't bind");
    println!("Server listening on port {}", port);

    let clients: Clients = Arc::new(Mutex::new(Vec::new()));

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let peer = stream.peer_addr().unwrap();
                println!("New connection: {}", peer);

                let clients_ref = Arc::clone(&clients);

                thread::spawn(move || handle_client(stream, clients_ref));
            }
            Err(e) => eprintln!("Error: {}", e),
        }
    }
}
