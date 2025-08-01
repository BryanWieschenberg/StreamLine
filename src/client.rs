use std::io::{self, BufRead, BufReader, Write};
use std::net::TcpStream;
use std::{env, thread};
use colored::Colorize;
use std::time::{SystemTime, UNIX_EPOCH};
mod crypto;
use crate::crypto::{generate_or_load_keys};

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
        if let Ok(pub_b64) = generate_or_load_keys(username) {

            // Send pubkey to server
            let register = format!("/pubkey {pub_b64}");
            stream.write_all(register.as_bytes())?;
            stream.write_all(b"\n")?;
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
                if msg.starts_with("/") {
                    if let Err(e) = handle_control_packets(&mut stream_for_writing.try_clone()?, &msg) {
                        eprintln!("Error handling control packet: {e}");
                    }
                    continue;
                }

                println!("{msg}")
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

        if msg == "/clear" || msg == "/c" {
            print!("\x1B[2J\x1B[H");
            io::stdout().flush()?;
            continue;
        }

        if msg == "/ping" {
            let now_ms = match SystemTime::now().duration_since(UNIX_EPOCH) {
                Ok(dur) => dur.as_millis(),
                Err(_) => 0,
            };
            let ping_setup = format!("/ping {}", now_ms);
            stream.write_all(ping_setup.as_bytes())?;
            stream.write_all(b"\n")?;
        }

        stream.write_all(msg.as_bytes())?;
        stream.write_all(b"\n")?;
    }

    Ok(())
}
