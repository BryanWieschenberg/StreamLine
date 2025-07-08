use std::io::{self, BufRead, BufReader, Write};
use std::net::TcpStream;
use std::thread;

// Function to handle receiving messages from the server
fn handle_recv(stream: TcpStream) -> std::io::Result<()> {
    let reader = BufReader::new(stream);
    for line in reader.lines() {
        match line {
            Ok(msg) => println!("{msg}"),
            Err(e) => {
                eprintln!("Error reading line: {e}");
                break;
            }
        }
    }

    Ok(())
}

// Main function to connect to the server and read/receive user input
fn main() -> std::io::Result<()> {
    let port = 8080;
    let mut stream = TcpStream::connect(format!("127.0.0.1:{port}"))?;
    let stream_clone = stream.try_clone()?;

    // Prompt user for username and send it to the server
    println!("Enter your username:");
    let mut username = String::new();
    io::stdin().read_line(&mut username)?;
    stream.write_all(username.trim().as_bytes())?;
    stream.write_all(b"\n")?;
    
    // Spawn a thread to handle receiving messages from the server
    thread::spawn(move || handle_recv(stream_clone));

    // Handles sending messages to the server
    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let msg = line?.trim().to_string();
        
        // If the user types "/exit", safely close the connection and exit
        if msg == "/exit" {
            println!("[!] Exiting...");
            stream.shutdown(std::net::Shutdown::Both)?;
            break;
        }

        // Writes the message to the server with a newline
        stream.write_all(msg.as_bytes())?;
        stream.write_all(b"\n")?;
    }

    Ok(())
}
