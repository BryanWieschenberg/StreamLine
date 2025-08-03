use std::collections::HashMap;
use std::io::{BufReader, BufRead, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::{env, thread};
use std::time::{SystemTime, Instant, Duration};
use colored::Colorize;
mod commands;
use crate::commands::parser::{Command, parse_command};
use crate::commands::dispatcher::{dispatch_command, CommandResult};
use crate::commands::command_utils::{unix_timestamp};
mod types;
use crate::types::{Client, ClientState, Clients, PublicKeys, Room, Rooms};
mod utils;
use crate::utils::{check_mute, format_broadcast, lock_client, lock_clients, lock_room, lock_rooms};

pub fn session_housekeeper(clients: Clients, rooms: Rooms) -> std::io::Result<()> {
    loop {
        // Check for inactive clients every 60 seconds
        thread::sleep(Duration::from_secs(60));
        let now = SystemTime::now();

        // Get each room's timeout
        let room_timeouts: HashMap<String, u32> = {
            let rooms_guard = match rooms.lock() {
                Ok(g)  => g,
                Err(_) => continue,
            };

            rooms_guard
                .iter()
                .filter_map(|(name, arc)| {
                    arc.lock().ok().map(|r| (name.clone(), r.session_timeout))
                })
                .collect()
        };

        let client_arcs: Vec<Arc<Mutex<_>>> = match clients.lock() {
            Ok(map) => map.values().cloned().collect(),
            Err(_)  => continue,
        };

        // Evaluate each client for inactivity
        for client_arc in client_arcs {
            if let Ok(mut client) = client_arc.lock() {
                if let ClientState::InRoom { username, room, inactive_time, .. } = &mut client.state {
                    let timeout = match room_timeouts.get(room) {
                        Some(t) => *t,
                        None => 0,
                    };
                    if timeout == 0 { continue; } // Unlimited session time for that room

                    let last_seen = match inactive_time {
                        Some(t) => *t,
                        None => now,
                    };

                    let idle_secs = match now.duration_since(last_seen) {
                        Ok(d)  => d.as_secs(),
                        Err(_) => 0,
                    };

                    if idle_secs >= timeout as u64
                    {
                        // Kick to lobby
                        let user = username.clone();
                        let room_name = room.clone();

                        client.state = ClientState::LoggedIn { username: user.clone() };
                        writeln!(client.stream, "{}", format!("/LOBBY_STATE"))?;
                        writeln!(client.stream, "{}", "Session timed out, returned to lobby".yellow())?;
                        drop(client);

                        {
                            let rooms_map = lock_rooms(&rooms)?;
                            if let Some(room_arc) = rooms_map.get(&room_name) {
                                if let Ok(mut r) = room_arc.lock() {
                                    r.online_users.retain(|u| u != &user);   // ← remove from list
                                }
                            }
                        }

                        // Get current unix time and update client's last_seen value for their current room
                        if let Err(e) = unix_timestamp(&rooms, &room_name, &user) {
                            eprintln!("Error updating last_seen for {user} in {room_name}: {e}");
                        }

                        println!("Auto-kicked {user} from {room_name} for inactivity");
                    }
                }
            }
        }
    }
}

pub fn check_rate_limit(client_arc: &Arc<Mutex<Client>>, rooms: &Rooms) -> std::io::Result<bool> {
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

// Handler for each client connection on a separate thread
fn handle_client(stream: TcpStream, peer: SocketAddr, clients: Clients, rooms: Rooms, pubkeys: PublicKeys) -> std::io::Result<()> {
    let reader = BufReader::new(stream.try_clone()?);

    let client_arc = Arc::new(Mutex::new(Client {
        stream: stream.try_clone()?,
        addr: peer,
        state: ClientState::Guest,
        ignore_list: Vec::new(),
        pubkey: String::new()
    }));

    // Insert the new client into the clients map
    {
        let mut locked = lock_clients(&clients)?;
        locked.insert(peer, Arc::clone(&client_arc));
    }

    println!("Address {peer} connected");

    // Read messages from the client and broadcast them to all other clients
    for line in reader.lines() {
        match line {
            Ok(msg) => {
                let msg = msg.trim().to_string();

                if msg.is_empty() { continue };

                // Optional: Have server print what it sees from E2EE perspective
                // println!("{}", msg);

                // Update last seen time for the client and reset AFK status
                {
                    let mut s = lock_client(&client_arc)?;
                    if let ClientState::InRoom { inactive_time, is_afk, .. } = &mut s.state {
                        *inactive_time = Some(SystemTime::now());
                        *is_afk = false;
                    }
                }

                if msg.starts_with("/") {
                    if let Some(rest) = msg.strip_prefix("/members? ") {
                        let (username, room_name) = {
                            let client = lock_client(&client_arc)?;
                            match &client.state {
                                ClientState::InRoom { username, room, .. } => (username.clone(), room.clone()),
                                _ => {
                                    let mut client = lock_client(&client_arc)?;
                                    writeln!(client.stream, "{}", "You are not in a room".yellow())?;
                                    continue;
                                }
                            }
                        };

                        let clients_map = lock_clients(&clients)?;
                        let pubkeys_map = match pubkeys.lock() {
                            Ok(map) => map,
                            Err(_) => {
                                let mut client = lock_client(&client_arc)?;
                                writeln!(client.stream, "{}", "Failed to lock pubkeys".red())?;
                                continue;
                            }
                        };

                        let mut pairs = Vec::new();

                        let tokens: Vec<&str> = rest.trim().split_whitespace().collect();

                        match tokens.as_slice() {
                            ["ind", target] => {
                                for (_, arc) in clients_map.iter() {
                                    let client = match arc.lock() {
                                        Ok(c) => c,
                                        Err(e) => {
                                            eprintln!("Failed to lock client: {e}");
                                            continue;
                                        }
                                    };
                                    let (u, r) = match &client.state {
                                        ClientState::InRoom { username: u, room: r, .. } => (u, r),
                                        _ => continue,
                                    };

                                    if u != target || r != &room_name {
                                        continue;
                                    }

                                    // Skip if recipient has sender ignored
                                    if client.ignore_list.contains(&username) {
                                        continue;
                                    }

                                    let key_b64 = match pubkeys_map.get(u) {
                                        Some(k) => k.clone(),
                                        None => {
                                            eprintln!("Public key for user '{}' not found", u);
                                            continue;
                                        }
                                    };
                                    let mut client = lock_client(&client_arc)?;
                                    writeln!(client.stream, "/members {u}:{key_b64}")?;
                                    break;
                                }
                            }

                            ["normal"] => {
                                for (_, arc) in clients_map.iter() {
                                    let client = match arc.lock() {
                                        Ok(c) => c,
                                        Err(e) => {
                                            eprintln!("Failed to lock client: {e}");
                                            continue;
                                        }
                                    };
                                    let (u, r) = match &client.state {
                                        ClientState::InRoom { username: u, room: r, .. } => (u, r),
                                        _ => continue,
                                    };

                                    if r != &room_name || u == &username {
                                        continue;
                                    }

                                    if client.ignore_list.contains(&username) {
                                        continue;
                                    }

                                    let Some(key_b64) = pubkeys_map.get(u).cloned() else {
                                        eprintln!("No public key found for user '{}'", u);
                                        continue;
                                    };
                                    pairs.push(format!("{u}:{key_b64}"));
                                }

                                let line = format!("/members {}", pairs.join(" "));
                                let mut client = lock_client(&client_arc)?;
                                writeln!(client.stream, "{line}")?;
                            }

                            ["full"] => {
                                for (_, arc) in clients_map.iter() {
                                    let client = match arc.lock() {
                                        Ok(c) => c,
                                        Err(e) => {
                                            eprintln!("Failed to lock client: {e}");
                                            continue;
                                        }
                                    };
                                    let (u, r) = match &client.state {
                                        ClientState::InRoom { username: u, room: r, .. } => (u, r),
                                        _ => continue,
                                    };

                                    if r != &room_name {
                                        continue;
                                    }

                                    if client.ignore_list.contains(&username) {
                                        continue;
                                    }

                                    let Some(key_b64) = pubkeys_map.get(u).cloned() else {
                                        eprintln!("No public key found for user '{}'", u);
                                        continue;
                                    };
                                    pairs.push(format!("{u}:{key_b64}"));
                                }

                                let line = format!("/members {}", pairs.join(" "));
                                let mut client = lock_client(&client_arc)?;
                                writeln!(client.stream, "{line}")?;
                            }

                            _ => {
                                let mut client = lock_client(&client_arc)?;
                                writeln!(client.stream, "{}", "Invalid /members? usage".red())?;
                            }
                        }

                        continue;
                    }

                    let command: Command = parse_command(&msg);
                    
                    match dispatch_command(command, Arc::clone(&client_arc), &clients, &rooms, &pubkeys)? {
                        CommandResult::Handled => continue,
                        CommandResult::Stop => break
                    }
                }

                let allowed = check_rate_limit(&client_arc, &rooms)?;
                if !allowed {
                    continue;
                }

                let mut sender = lock_client(&client_arc)?;
                
                match &sender.state {
                    ClientState::InRoom { username, room, .. } => {
                        let username = username.clone();
                        let room_name = room.clone();
                        drop(sender);

                        if let Some(msg) = check_mute(&rooms, &room_name, &username)? {
                            let mut client = lock_client(&client_arc)?;
                            writeln!(client.stream, "{}", msg.red())?;
                            continue;
                        }

                        let (role_prefix, display_name) = format_broadcast(&rooms, &room_name, &username)?;
                        let mut parts = msg.splitn(2, ' ');
                        let Some(recipient) = parts.next() else {
                            eprintln!("Missing recipient in encrypted message");
                            continue;
                        };

                        let Some(ciphertext) = parts.next() else {
                            eprintln!("Missing ciphertext in encrypted message");
                            continue;
                        };

                        if recipient.is_empty() || ciphertext.is_empty() {
                            continue;
                        }

                        let clients_map = lock_clients(&clients)?;
                        if let Some(rec_arc) = clients_map.values().find(|arc| {
                            let c = match arc.lock() {
                                Ok(c) => c,
                                Err(e) => {
                                    eprintln!("Failed to lock client: {e}");
                                    return false;
                                }
                            };
                            matches!(&c.state,
                                ClientState::InRoom { username: u, room: r, .. }
                                if u == recipient && r == &room_name)
                        }).cloned()
                        {
                            let mut rec = lock_client(&rec_arc)?;
                            writeln!(rec.stream, "/enc {} {}: {}", role_prefix, display_name, ciphertext)?;
                        }
                    }
                    ClientState::LoggedIn { .. } => {
                        writeln!(sender.stream, "{}", "You must join a room to chat".yellow())?;
                    }
                    ClientState::Guest => {
                        writeln!(sender.stream, "{}", "You must log in to chat".yellow())?;
                    }
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
    let args: Vec<String> = env::args().collect();
    if args.len() > 2 {
        eprintln!("Usage: ./cargo run --bin server -q <port>?");
        std::process::exit(1);
    }

    let port: u16 = if args.len() == 2 {
        match args[1].parse::<u16>() {
            Ok(p) => p,
            Err(_) => 8000,
        }
    } else {
        8000
    };
    
    let listener = TcpListener::bind(format!("0.0.0.0:{port}"))?;

    let clients: Clients = Arc::new(Mutex::new(HashMap::new()));
    let pubkeys: PublicKeys = Arc::new(Mutex::new(HashMap::new()));
    let room_file = std::fs::File::open("data/rooms.json")?;
    let room_reader = BufReader::new(room_file);
    let parsed_rooms: HashMap<String, Room> = serde_json::from_reader(room_reader)?;

    // Convert into Arc<Mutex<Room>>
    let mut rooms_map: HashMap<String, Arc<Mutex<Room>>> = HashMap::new();
    for (name, room) in parsed_rooms {
        rooms_map.insert(name, Arc::new(Mutex::new(room)));
    }

    let rooms: Rooms = Arc::new(Mutex::new(rooms_map));
    
    println!("Server listening on port {port}");

    {
        let clients = Arc::clone(&clients);
        let rooms = Arc::clone(&rooms);

        // Start the session housekeeper thread
        thread::Builder::new()
            .name("session-housekeeper".into())
            .spawn(move || {
                if let Err(e) = session_housekeeper(clients, rooms) {
                    eprintln!("Thread for session housekeeping exited with error: {e}");
                }
            })?;
    }

    // Main loop to accept incoming connections
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let peer = stream.peer_addr()?;

                // Clone clients Arc and use the clone in the thread
                let clients = Arc::clone(&clients);
                let rooms = Arc::clone(&rooms);
                let pubkeys = Arc::clone(&pubkeys);

                thread::Builder::new()
                    .name(format!("client-{peer}"))
                    .spawn(move || {
                        if let Err(e) = handle_client(stream, peer, clients, rooms, pubkeys) {
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
