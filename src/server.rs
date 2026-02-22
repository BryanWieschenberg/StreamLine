use std::collections::{HashMap, VecDeque};
use std::io::{BufReader, BufRead, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::{env, thread};
use std::time::{SystemTime, Instant, Duration};
use colored::Colorize;
mod backend;
mod shared;

use crate::backend::parser::{Command, parse_command};
use crate::backend::dispatcher::{dispatch_command, CommandResult};
use crate::backend::command_utils::{sync_room_members, unix_timestamp};
use crate::shared::types::{Client, ClientState, Clients, PublicKeys, Room, Rooms};
use crate::shared::utils::{check_mute, format_broadcast, lock_client, lock_clients, lock_room, lock_rooms};

pub fn session_housekeeper(clients: Clients, rooms: Rooms, pubkeys: PublicKeys) -> std::io::Result<()> {
    loop {
        thread::sleep(Duration::from_secs(60));
        let now = SystemTime::now();

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

        for client_arc in client_arcs {
            if let Ok(mut client) = client_arc.lock() {
                if let ClientState::InRoom { username, room, inactive_time, .. } = &mut client.state {
                    let timeout = match room_timeouts.get(room) {
                        Some(t) => *t,
                        None => 0,
                    };
                    if timeout == 0 { continue; }

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
                                    r.online_users.retain(|u| u != &user);
                                }
                            }
                        }
                        let _ = sync_room_members(&rooms, &clients, &pubkeys, &room_name);

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

pub fn check_rate_limit(client_arc: &Arc<Mutex<Client>>, rooms: &Rooms, is_first: bool) -> std::io::Result<bool> {
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
            if is_first {
                writeln!(c.stream, "{}", "Rate limit exceeded, slow down your messages!".yellow())?;
            }
            return Ok(false);
        }

        if is_first {
            msg_timestamps.push_back(now);
        }
    }
    Ok(true)
}

fn handle_client(stream: TcpStream, peer: SocketAddr, clients: Clients, rooms: Rooms, pubkeys: PublicKeys) -> std::io::Result<()> {
    let reader = BufReader::new(stream.try_clone()?);

    let client_arc = Arc::new(Mutex::new(Client {
        stream: stream.try_clone()?,
        addr: peer,
        state: ClientState::Guest,
        ignore_list: Vec::new(),
        pubkey: String::new(),
        login_attempts: VecDeque::new(),
    }));

    {
        let mut locked = lock_clients(&clients)?;
        locked.insert(peer, Arc::clone(&client_arc));
    }

    println!("Address {peer} connected");

    for line in reader.lines() {
        match line {
            Ok(msg) => {
                let msg = msg.trim().to_string();

                if msg.is_empty() { continue };

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

                        let (can_see_hidden, online_in_room) = {
                            let rooms_map = match lock_rooms(&rooms) {
                                Ok(m) => m,
                                Err(_) => {
                                    let mut client = lock_client(&client_arc)?;
                                    writeln!(client.stream, "{}", "Failed to lock rooms".red())?;
                                    continue;
                                }
                            };
                            let room_arc = match rooms_map.get(&room_name) {
                                Some(r) => Arc::clone(r),
                                None => {
                                    let mut client = lock_client(&client_arc)?;
                                    writeln!(client.stream, "{}", format!("Room {room_name} not found").yellow())?;
                                    continue;
                                }
                            };
                            let room_guard = match lock_room(&room_arc) {
                                Ok(g) => g,
                                Err(_) => {
                                    let mut client = lock_client(&client_arc)?;
                                    writeln!(client.stream, "{}", "Failed to lock room".red())?;
                                    continue;
                                }
                            };
                            
                            let role = room_guard.users.get(&username).map(|u| u.role.as_str()).unwrap_or("user");
                            let can_see = role == "owner" || role == "admin";

                            let mut online_visibility = HashMap::new();
                            for uname in &room_guard.online_users {
                                let is_hidden = room_guard.users.get(uname).map(|u| u.hidden).unwrap_or(false);
                                online_visibility.insert(uname.clone(), is_hidden);
                            }
                            (can_see, online_visibility)
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
                                    if let Ok(client) = arc.try_lock() {
                                        if let ClientState::InRoom { username: u, room: r, .. } = &client.state {
                                            if u == target && r == &room_name {
                                                let is_hidden = online_in_room.get(u).cloned().unwrap_or(false);
                                                if is_hidden && !can_see_hidden {
                                                    continue;
                                                }
                                                if client.ignore_list.contains(&username) {
                                                    continue;
                                                }
                                                if let Some(key) = pubkeys_map.get(u) {
                                                    let mut requester = lock_client(&client_arc)?;
                                                    writeln!(requester.stream, "/members {u}:{key}")?;
                                                }
                                                break;
                                            }
                                        }
                                    }
                                }
                            }

                            ["normal"] => {
                                for (_, arc) in clients_map.iter() {
                                    if let Ok(client) = arc.try_lock() {
                                        if let ClientState::InRoom { username: u, room: r, .. } = &client.state {
                                            if r == &room_name && u != &username {
                                                let is_hidden = online_in_room.get(u).cloned().unwrap_or(false);
                                                if is_hidden && !can_see_hidden {
                                                    continue;
                                                }
                                                if client.ignore_list.contains(&username) {
                                                    continue;
                                                }
                                                if let Some(key) = pubkeys_map.get(u) {
                                                    pairs.push(format!("{u}:{key}"));
                                                }
                                            }
                                        }
                                    }
                                }
                                let line = format!("/members {}", pairs.join(" "));
                                let mut requester = lock_client(&client_arc)?;
                                writeln!(requester.stream, "{line}")?;
                            }

                            ["full"] => {
                                for (_, arc) in clients_map.iter() {
                                    if let Ok(client) = arc.try_lock() {
                                        if let ClientState::InRoom { username: u, room: r, .. } = &client.state {
                                            if r == &room_name {
                                                let is_hidden = online_in_room.get(u).cloned().unwrap_or(false);
                                                if is_hidden && !can_see_hidden {
                                                    continue;
                                                }
                                                if client.ignore_list.contains(&username) {
                                                    continue;
                                                }
                                                if let Some(key) = pubkeys_map.get(u) {
                                                    pairs.push(format!("{u}:{key}"));
                                                }
                                            }
                                        }
                                    }
                                }
                                let line = format!("/members {}", pairs.join(" "));
                                let mut requester = lock_client(&client_arc)?;
                                writeln!(requester.stream, "{line}")?;
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

                let mut sender = lock_client(&client_arc)?;
                
                match &sender.state {
                    ClientState::InRoom { username, room, .. } => {
                        let username = username.clone();
                        let room_name = room.clone();
                        drop(sender);

                        let (role_prefix, display_name) = format_broadcast(&rooms, &room_name, &username)?;
                        
                        let mut pieces: Vec<&str> = msg.split_whitespace().collect();
                        if pieces.len() < 2 { continue; }

                        let is_first = pieces.last() == Some(&"f");
                        if is_first { pieces.pop(); }

                        let recipient   = pieces[0];
                        let ciphertext  = pieces[1];

                        if let Some(msg) = check_mute(&rooms, &room_name, &username)? {
                            if is_first {
                                let mut client = lock_client(&client_arc)?;
                                writeln!(client.stream, "{}", msg.red())?;
                            }
                            continue;
                        }

                        if !check_rate_limit(&client_arc, &rooms, is_first)? {
                            continue;
                        }

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

    let removed = {
        let mut locked = lock_clients(&clients)?;
        locked.remove(&peer)
    };

    if let Some(client_arc) = removed {
        let client = lock_client(&client_arc)?;

        match &client.state {
            ClientState::Guest => println!("Guest ({peer}) disconnected"),
            ClientState::LoggedIn { username } => println!("{username} ({peer}) disconnected"),
            ClientState::InRoom { username, room, .. } => {
                println!("{username} ({peer}) disconnected from {room}");
                let uname = username.clone();
                let rname = room.clone();
                {
                    let rmap = lock_rooms(&rooms)?;
                    if let Some(rarc) = rmap.get(&rname) {
                        if let Ok(mut r) = rarc.lock() {
                            r.online_users.retain(|u| u != &uname);
                        }
                    }
                }
                let _ = sync_room_members(&rooms, &clients, &pubkeys, &rname);
                let _ = unix_timestamp(&rooms, &rname, &uname);
            }
        }
    }

    Ok(())
}

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

    let mut rooms_map: HashMap<String, Arc<Mutex<Room>>> = HashMap::new();
    for (name, room) in parsed_rooms {
        rooms_map.insert(name, Arc::new(Mutex::new(room)));
    }

    let rooms: Rooms = Arc::new(Mutex::new(rooms_map));
    
    println!("Server listening on port {port}");

    {
        let clients = Arc::clone(&clients);
        let rooms = Arc::clone(&rooms);
        let pubkeys = Arc::clone(&pubkeys);

        thread::Builder::new()
            .name("session-housekeeper".into())
            .spawn(move || {
                if let Err(e) = session_housekeeper(clients, rooms, pubkeys) {
                    eprintln!("Thread for session housekeeping exited with error: {e}");
                }
            })?;
    }

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let peer = stream.peer_addr()?;

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
