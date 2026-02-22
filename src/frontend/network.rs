use std::io::{self, BufRead, BufReader, Write};
use std::net::TcpStream;
use std::sync::mpsc::Sender;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::frontend::app::{AppMessage, ClientState, MY_STATE, CURRENT_USER, CURRENT_ROOM, MY_ROLE, ALLOWED_COMMANDS, MEMBERS};
use crate::shared::crypto::{generate_or_load_keys, decrypt};

pub fn handle_control_packets(stream: &mut TcpStream, msg: &str, tx: &Sender<AppMessage>) -> std::io::Result<()> {
    if let Some(frt_latency) = msg.strip_prefix("/PONG ") {
        if let Ok(sent_ms) = frt_latency.trim().parse::<u128>() {
            let now_ms = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
            let rtt_s = now_ms.saturating_sub(sent_ms) as f64 / 1000.0;
            let _ = tx.send(AppMessage::ControlResult(format!("◎ Pong! Round-trip: {rtt_s:.3}s")));
        } else {
            let _ = tx.send(AppMessage::ControlResult("⚠ Warning: received /PONG with invalid timestamp".into()));
        }
        return Ok(());
    }

    if let Some(username) = msg.strip_prefix("/LOGIN_OK ") {
        {
            let mut state = MY_STATE.lock().map_err(|e| io::Error::other(e.to_string()))?;
            *state = ClientState::LoggedIn;
        }
        {
            let mut u = CURRENT_USER.lock().map_err(|e| io::Error::other(e.to_string()))?;
            *u = username.trim().to_string();
        }
        if let Ok(pub_b64) = generate_or_load_keys(username) {
            stream.write_all(format!("/pubkey {pub_b64}\n").as_bytes())?;
        }
        return Ok(());
    }

    if msg == "/ROOM_STATE" {
        let mut state = MY_STATE.lock().map_err(|e| io::Error::other(e.to_string()))?;
        *state = ClientState::InRoom;
        stream.write_all(b"/members? full\n")?;
        return Ok(());
    }

    if let Some(role) = msg.strip_prefix("/ROLE ") {
        if let Ok(mut r) = MY_ROLE.lock() {
            *r = role.trim().to_string();
        }
        return Ok(());
    }

    if msg == "/LOBBY_STATE" {
        let mut state = MY_STATE.lock().map_err(|e| io::Error::other(e.to_string()))?;
        *state = ClientState::LoggedIn;
        if let Ok(mut r) = CURRENT_ROOM.lock() { r.clear(); }
        if let Ok(mut r) = MY_ROLE.lock() { r.clear(); }
        if let Ok(mut cmds) = ALLOWED_COMMANDS.lock() { cmds.clear(); }
        return Ok(());
    }

    if msg == "/GUEST_STATE" {
        let mut state = MY_STATE.lock().map_err(|e| io::Error::other(e.to_string()))?;
        *state = ClientState::Guest;
        if let Ok(mut u) = CURRENT_USER.lock() { u.clear(); }
        if let Ok(mut r) = CURRENT_ROOM.lock() { r.clear(); }
        if let Ok(mut r) = MY_ROLE.lock() { r.clear(); }
        if let Ok(mut cmds) = ALLOWED_COMMANDS.lock() { cmds.clear(); }
        return Ok(());
    }

    if let Some(cmds_str) = msg.strip_prefix("/CMDS ") {
        if let Ok(mut cmds) = ALLOWED_COMMANDS.lock() {
            *cmds = cmds_str.split_whitespace().map(String::from).collect();
        }
        return Ok(());
    }

    if msg == "/CMDS" {
        if let Ok(mut cmds) = ALLOWED_COMMANDS.lock() {
            cmds.clear();
        }
        return Ok(());
    }

    if let Some(room_name) = msg.strip_prefix("/ROOM_NAME ") {
        if let Ok(mut r) = CURRENT_ROOM.lock() { *r = room_name.trim().to_string(); }
        return Ok(());
    }

    if let Some(rest) = msg.strip_prefix("/members ") {
        let mut map = HashMap::new();
        for pair in rest.split_whitespace() {
            if let Some((user, pubkey)) = pair.split_once(':') {
                map.insert(user.to_string(), pubkey.to_string());
            }
        }
        let (lock, cvar) = &*MEMBERS;
        let mut members = lock.lock().map_err(|e| io::Error::other(e.to_string()))?;
        *members = map;
        cvar.notify_all();
        return Ok(());
    }

    if msg == "/members" {
        let (lock, cvar) = &*MEMBERS;
        if let Ok(mut members) = lock.lock() {
            members.clear();
            cvar.notify_all();
        }
        return Ok(());
    }

    Ok(())
}

pub fn handle_recv(stream: TcpStream, tx: Sender<AppMessage>) -> std::io::Result<()> {
    let mut stream_for_writing = stream.try_clone()?;
    let reader = BufReader::new(stream);

    for line in reader.lines() {
        match line {
            Ok(msg) => {
                if msg.starts_with("/enc ") {
                    if let Some(enc_line) = msg.strip_prefix("/enc ") {
                        if let Some((prefix, cipher_b64)) = enc_line.split_once(": ") {
                            match decrypt(cipher_b64) {
                                Ok(plaintext) => { let _ = tx.send(AppMessage::ServerMessage(format!("{prefix}: {plaintext}"))); }
                                Err(e) => { let _ = tx.send(AppMessage::NetworkError(format!("Decryption error: {e}"))); }
                            }
                        } else {
                            let _ = tx.send(AppMessage::NetworkError("Malformed /enc message".into()));
                        }
                    }
                    continue;
                }

                if msg.starts_with('/') {
                    if let Err(e) = handle_control_packets(&mut stream_for_writing, &msg, &tx) {
                        let _ = tx.send(AppMessage::NetworkError(format!("Control packet error: {e}")));
                    }
                    continue;
                }

                let _ = tx.send(AppMessage::ServerMessage(msg));
            }
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => {
                let _ = tx.send(AppMessage::NetworkError("Connection closed by server".into()));
                break;
            }
            Err(e) => {
                let _ = tx.send(AppMessage::NetworkError(format!("Read error: {e}")));
            }
        }
    }
    Ok(())
}
