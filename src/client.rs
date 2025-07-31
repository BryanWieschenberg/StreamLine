use std::io::{self, BufRead, BufReader, Write};
use std::net::TcpStream;
use std::{env, thread};
use colored::Colorize;

mod crypto;
use crate::crypto::{sayhi};

// Function to handle receiving messages from the server
fn handle_recv(stream: TcpStream) -> std::io::Result<()> {
    let reader = BufReader::new(stream);
    for line in reader.lines() {
        match line {
            Ok(msg) => {
                if let Some(username) = msg.strip_prefix("/LOGIN_OK ") {
                    sayhi(username);
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
        Make an account with /account register <username> <password> <confirm password> or
        login with /account login <username> <password> and
        make a room with /room create <room name>,
        join a room with /room join <room name>,
        or see a list of rooms with /room\n
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

        stream.write_all(msg.as_bytes())?;
        stream.write_all(b"\n")?;
    }

    Ok(())
}
