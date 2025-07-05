use std::io::{self, BufRead, BufReader, Write};
use std::net::TcpStream;
use std::thread;

fn main() {
    let port = 9001;
    let stream = TcpStream::connect(format!("127.0.0.1:{}", port)).expect("Failed to connect");
    let mut write_stream = stream.try_clone().expect("Clone failed");
    
    println!("Welcome to Chatterbox! Please enter your username:");
    let mut username = String::new();
    let _ = io::stdout().flush();

    io::stdin().read_line(&mut username).unwrap();
    let _ = write_stream.write_all(username.trim().as_bytes());
    write_stream.write_all(b"\n").unwrap();

    // Read from server
    thread::spawn(move || {
        let reader = BufReader::new(stream);
        for line in reader.lines() {
            if let Ok(msg) = line {
                println!("{}", msg);
            }
        }
    });

    // Write to server
    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let msg = line.unwrap();
        if msg.trim() == "/exit" || msg.trim() == "\\q" {
            println!("Disconnecting...");
            break;
        }
        if write_stream.write_all(msg.as_bytes()).is_err() {
            break;
        }
        let _ = write_stream.write_all(b"\n");
    }
}
