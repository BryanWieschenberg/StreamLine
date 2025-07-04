use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;

type Clients = Arc<Mutex<Vec<TcpStream>>>;

fn handle_client(stream: TcpStream, clients: Clients) {
    let peer = stream.peer_addr().unwrap();
    let reader = BufReader::new(stream.try_clone().unwrap());

    for line in reader.lines() {
        let msg = match line {
            Ok(m) => m,
            Err(_) => break,
        };

        println!("{} says: {}", peer, msg);

        let mut locked = clients.lock().unwrap();
        locked.retain(|s| s.peer_addr().is_ok());
        for client in locked.iter_mut() {
            if client.peer_addr().unwrap() != peer {
                let _ = writeln!(client, "{}: {}", peer, msg);
            }
        }
    }
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

                clients_ref.lock().unwrap().push(stream.try_clone().unwrap());

                thread::spawn(move || handle_client(stream, clients_ref));
            }
            Err(e) => eprintln!("Error: {}", e),
        }
    }
}
