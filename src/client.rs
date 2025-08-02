use std::collections::HashMap;
use std::io::{self, BufRead, BufReader, Write};
use std::net::TcpStream;
use std::sync::{Condvar, Mutex};
use std::{env, thread};
use colored::Colorize;
use once_cell::sync::Lazy;
use std::time::{SystemTime, UNIX_EPOCH};
mod crypto;
use crate::crypto::{generate_or_load_keys, decrypt, broadcast_message};

enum ClientState {
    Guest,
    LoggedIn,
    InRoom
}

// Clientside room members list for sending encrypted messages to the server
// Uses Mutex for send/recv threaed safely and Condvar for main thread blocking until list fully populated
static MEMBERS: Lazy<(Mutex<HashMap<String, String>>, Condvar)> = Lazy::new(|| (Mutex::new(HashMap::new()), Condvar::new()));

// Clientside state tracker
static MY_STATE: Lazy<Mutex<ClientState>> = Lazy::new(|| (Mutex::new(ClientState::Guest)));

fn get_room_members() -> HashMap<String, String> {
    let (lock, cvar) = &*MEMBERS;
    let mut members = lock.lock().unwrap();

    // Wait until members is not empty
    members = cvar.wait(members).unwrap();

    members.clone() // Return a copy
}

fn handle_control_packets(stream: &mut TcpStream, msg: &str) -> std::io::Result<()> {
    // Ping FRT latency calculation
    if let Some(frt_latency) = msg.strip_prefix("/PONG ") {
        if let Ok(sent_ms) = frt_latency.trim().parse::<u128>() {
            let now_ms = match SystemTime::now().duration_since(UNIX_EPOCH) {
                Ok(dur) => dur.as_millis(),
                Err(_) => 0,
            };

            let rtt_s = (now_ms.saturating_sub(sent_ms)) as f64 / 1000.0;
            println!("{}", format!("Pong! Full round-trip latency: {rtt_s:.3} seconds").green());
        } else {
            eprintln!("Warning: Received /ping but timestamp was invalid");
        }
        return Ok(());
    }
    
    // Login confirmation
    else if let Some(username) = msg.strip_prefix("/LOGIN_OK ") {
        {
            let mut state = MY_STATE.lock().unwrap();
            *state = ClientState::LoggedIn;
        }
        
        if let Ok(pub_b64) = generate_or_load_keys(username) {
            // Send pubkey to server
            let register = format!("/pubkey {pub_b64}");
            stream.write_all(register.as_bytes())?;
            stream.write_all(b"\n")?;
        }
        return Ok(());
    }

    else if msg == "/ROOM_STATE" {
        {
            let mut state = match MY_STATE.lock() {
                Ok(state) => state,
                Err(e) => {
                    eprintln!("Failed to lock MY_STATE: {e}");
                    return Err(io::Error::new(io::ErrorKind::Other, "Failed to lock MY_STATE"));
                }
            };
            *state = ClientState::InRoom;
        }
        return Ok(());
    }

    else if msg == "/LOBBY_STATE" {
        {
            let mut state = match MY_STATE.lock() {
                Ok(state) => state,
                Err(e) => {
                    eprintln!("Failed to lock MY_STATE: {e}");
                    return Err(io::Error::new(io::ErrorKind::Other, "Failed to lock MY_STATE"));
                }
            };
            *state = ClientState::LoggedIn;
        }
        return Ok(());
    }

    else if msg == "/GUEST_STATE" {
        {
            let mut state = match MY_STATE.lock() {
                Ok(state) => state,
                Err(e) => {
                    eprintln!("Failed to lock MY_STATE: {e}");
                    return Err(io::Error::new(io::ErrorKind::Other, "Failed to lock MY_STATE"));
                }
            };
            *state = ClientState::Guest {};
        }
        return Ok(());
    }

    else if let Some(rest) = msg.strip_prefix("/members ") {
        let mut map = HashMap::new();
        for pair in rest.split_whitespace() {
            if let Some((user, pubkey)) = pair.split_once(':') {
                map.insert(user.to_string(), pubkey.to_string());
            }
        }

        let (lock, cvar) = &*MEMBERS;
        let mut members = lock.lock().unwrap();
        *members = map;
        cvar.notify_all(); // Let main send thread continue
        return Ok(());
    }

    // For if only sender is in room
    else if msg == "/members" {
        println!("hi");
        let (lock, cvar) = &*MEMBERS;
        if let Ok(mut members) = lock.lock() {
            members.clear(); // no members, just unblock
            cvar.notify_all();
        }
        return Ok(());
    }

    Ok(())
}

// Function to handle receiving messages from the server
fn handle_recv(stream: TcpStream) -> std::io::Result<()> {
    let stream_for_writing = stream.try_clone()?;
    
    let reader = BufReader::new(stream);
    for line in reader.lines() {
        match line {
            Ok(msg) => {
                if msg.starts_with("/enc ") {
                    if let Some(enc_line) = msg.strip_prefix("/enc ") {
                        if let Some((prefix, cipher_b64)) = enc_line.split_once(": ") {
                            match decrypt(cipher_b64) {
                                Ok(plaintext) => println!("{}: {}", prefix, plaintext),
                                Err(e) => {
                                    eprintln!("Decryption error: {e}");
                                    continue;
                                }
                            }
                        } else {
                            eprintln!("Malformed /enc message, missing prefix or ciphertext");
                        }
                    }
                    continue;
                }

                if msg.starts_with("/") {
                    if let Err(e) = handle_control_packets(&mut stream_for_writing.try_clone()?, &msg) {
                        eprintln!("Error handling control packet: {e}");
                    }
                    continue;
                }

                println!("{msg}");
            }
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => break,
            Err(e) => {
                eprintln!("Error reading line: {e}");
                continue;
            }
        }
    }

    std::process::exit(0);
}

// Main function to connect to the server and read/receive user input
fn main() -> std::io::Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() > 2 {
        eprintln!("Usage: ./cargo run --bin client -q <ip:port>?");
        std::process::exit(1);
    }

    let address = if args.len() == 2 {
        let arg = &args[1];
        match arg.parse::<u16>() {
            Ok(port) => format!("127.0.0.1:{}", port),
            Err(_) => {
                if arg.contains(':') {
                    arg.clone()
                } else {
                    eprintln!("Invalid format. Use <ip:port> or <port>.");
                    std::process::exit(1);
                }
            }
        }
    } else {
        "127.0.0.1:8000".to_string()
    };

    let mut stream = TcpStream::connect(&address)?;
    let stream_clone = stream.try_clone()?;

    println!("{}", "
        Welcome to StreamLine!\n
        Make an account with /account register <username> <password> <confirm password>
        Login with /account login <username> <password>
        Make a room with /room create <room name>
        Join a room with /room join <room name>
        See a list of rooms with /room\n
        For a list of all available commands, type /help
    ".bright_blue());
        
    // Spawn a thread to handle receiving messages from the server
    thread::spawn(move || handle_recv(stream_clone));

    // Handles sending messages to the server
    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let msg = line?.trim().to_string();

        if msg.is_empty() {
            continue;
        }

        if msg.starts_with('/') {
            // Local commands
            if msg == "/clear" || msg == "/c" {
                print!("\x1B[2J\x1B[H");
                io::stdout().flush()?;
                continue;
            }

            if msg == "/ping" {
                let now_ms = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis();
                let ping_setup = format!("/ping {}", now_ms);
                stream.write_all(ping_setup.as_bytes())?;
                stream.write_all(b"\n")?;
                continue;
            }

            stream.write_all(msg.as_bytes())?;
            stream.write_all(b"\n")?;
            continue;
        }

        {
            let (lock, _) = &*MEMBERS;
            if let Ok(mut map) = lock.lock() {
                map.clear();
            }
        }

        if let Ok(state) = MY_STATE.lock() {
            if let ClientState::InRoom = &*state {
                // Ask server for current room members
                let mut stream_clone = stream.try_clone()?;
                stream.write_all(b"/members? normal\n")?;

                let members = get_room_members();
                if !members.is_empty() {
                    broadcast_message(&mut stream_clone, &members, &msg)?;
                }
            } else {
                // Not in a room, just send plaintext
                stream.write_all(msg.as_bytes())?;
                stream.write_all(b"\n")?;
            }
        }
    }

    Ok(())
}
