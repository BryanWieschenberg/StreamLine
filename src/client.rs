use std::collections::HashMap;
use std::io::{self, BufRead, BufReader, Write};
use std::net::TcpStream;
use std::sync::{Condvar, Mutex};
use std::{env, thread};
use once_cell::sync::Lazy;
use std::time::{SystemTime, UNIX_EPOCH};
mod crypto;
use crate::crypto::{generate_or_load_keys, decrypt, broadcast_message};
use std::sync::mpsc::{self, Receiver, Sender};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, Paragraph},
    Frame, Terminal,
};

enum ClientState {
    Guest,
    LoggedIn,
    InRoom,
}

static MEMBERS: Lazy<(Mutex<HashMap<String, String>>, Condvar)> =
    Lazy::new(|| (Mutex::new(HashMap::new()), Condvar::new()));

static MY_STATE: Lazy<Mutex<ClientState>> = Lazy::new(|| Mutex::new(ClientState::Guest));

static CURRENT_USER: Lazy<Mutex<String>> = Lazy::new(|| Mutex::new(String::new()));
static CURRENT_ROOM: Lazy<Mutex<String>> = Lazy::new(|| Mutex::new(String::new()));
static MY_ROLE: Lazy<Mutex<String>> = Lazy::new(|| Mutex::new(String::new()));

fn get_room_members() -> HashMap<String, String> {
    let (lock, cvar) = &*MEMBERS;
    let mut members = match lock.lock() {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Failed to lock MEMBERS: {e}");
            return HashMap::new();
        }
    };
    members = match cvar.wait(members) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Failed to wait on MEMBERS: {e}");
            return HashMap::new();
        }
    };
    members.clone()
}

const C_BG: Color = Color::Rgb(18, 18, 18);
const C_SURFACE: Color = Color::Rgb(28, 28, 28);
const C_BORDER: Color = Color::Rgb(60, 60, 60);
const C_BORDER_ACTIVE: Color = Color::Rgb(160, 160, 160);
const C_TEXT: Color = Color::Rgb(240, 240, 240);
const C_DIM: Color = Color::DarkGray;
const C_ACCENT: Color = Color::Rgb(200, 200, 200);
const C_ACCENT2: Color = Color::Rgb(150, 150, 150);
const C_YELLOW: Color = Color::Rgb(220, 220, 220);
const C_RED: Color = Color::Red;
const C_GREEN: Color = Color::Rgb(120, 160, 120);
const C_SYSTEM: Color = Color::Gray;

const COMMANDS_ALWAYS: &[&str] = &[
    "/help",
    "/clear",
    "/quit",
    "/ping",
];

const COMMANDS_GUEST: &[&str] = &[
    "/account register",
    "/account login",
    "/account import",
];

const COMMANDS_LOGGEDIN: &[&str] = &[
    "/account logout",
    "/account edit username",
    "/account edit password",
    "/account export",
    "/account delete",
    "/account delete force",
    "/room",
    "/room list",
    "/room join",
    "/room create",
    "/room import",
    "/room delete",
    "/room delete force",
    "/ignore",
    "/ignore list",
    "/ignore add",
    "/ignore remove",
];

const COMMANDS_INROOM: &[&str] = &[
    "/room leave",
    "/leave",
    "/status",
    "/afk",
    "/msg",
    "/me",
    "/seen",
    "/announce",
    "/user",
    "/user list",
    "/user rename",
    "/user recolor",
    "/user hide",
];

const COMMANDS_MOD: &[&str] = &[
    "/mod",
    "/mod kick",
    "/mod ban",
    "/mod unban",
    "/mod mute",
    "/mod unmute",
    "/mod info",
];

const COMMANDS_SUPER: &[&str] = &[
    "/super",
    "/super users",
    "/super rename",
    "/super export",
    "/super whitelist",
    "/super whitelist info",
    "/super whitelist add",
    "/super whitelist remove",
    "/super limit",
    "/super limit info",
    "/super limit rate",
    "/super limit session",
    "/super roles",
    "/super roles list",
    "/super roles add",
    "/super roles revoke",
    "/super roles assign",
    "/super roles recolor",
];

enum AppMessage {
    ServerMessage(String),
    NetworkError(String),
    ControlResult(String),
}

enum LineKind {
    System,
    Error,
    Success,
    SelfMsg,
    UserMsg(String),
    Plain,
}

fn classify_line(s: &str) -> LineKind {
    if s.contains("Error")
        || s.contains("error")
        || s.contains("Failed")
        || s.contains("failed")
        || s.contains("Warning")
    {
        return LineKind::Error;
    }

    if s.starts_with("Pong!")
        || s.contains("success")
        || s.contains("Success")
        || s.starts_with("Logged in")
        || s.starts_with("Logged out")
        || s.starts_with("User Registered")
        || s.starts_with("Imported user")
        || s.starts_with("Username changed")
        || s.starts_with("Added to ignore")
        || s.starts_with("Removed from ignore")
        || s.starts_with("Currently ignoring")
        || s.starts_with("Exiting")
    {
        return LineKind::Success;
    }

    if s.starts_with("You: ") {
        return LineKind::SelfMsg;
    }

    if let Some(colon_pos) = s.find(": ") {
        let candidate = &s[..colon_pos];
        let name = if candidate.starts_with('[') {
            if let Some(bracket_end) = candidate.find("] ") {
                candidate[bracket_end + 2..].trim()
            } else {
                candidate
            }
        } else {
            candidate
        };
        if !name.is_empty() && !name.contains(' ') {
            return LineKind::UserMsg(candidate.to_string());
        }
    }

    if s.starts_with("Welcome")
        || s.starts_with("Make ")
        || s.starts_with("Login ")
        || s.starts_with("Join ")
        || s.starts_with("See ")
        || s.starts_with("For ")
        || s.starts_with("  /")
        || s.starts_with("  [")
        || s.starts_with("You must")
        || s.starts_with("Must ")
        || s.starts_with("Currently ")
        || s.starts_with("Session ")
        || s.starts_with("Rate limit")
        || s.starts_with("You are ")
        || s.starts_with("You do")
        || s.starts_with("Room ")
        || s.starts_with("Public key")
        || s.starts_with("Already ")
        || s.starts_with("Not in ")
        || s.starts_with("Command not")
        || s.starts_with("◎")
    {
        return LineKind::System;
    }

    LineKind::Plain
}

fn styled_line(s: &str) -> Line<'static> {
    if s.contains('\x1b') {
        return parse_ansi(s);
    }

    match classify_line(s) {
        LineKind::System  => Line::from(Span::styled(s.to_owned(), Style::default().fg(C_SYSTEM))),
        LineKind::Error   => Line::from(Span::styled(s.to_owned(), Style::default().fg(C_RED))),
        LineKind::Success => Line::from(Span::styled(s.to_owned(), Style::default().fg(C_GREEN))),
        LineKind::SelfMsg => {
            let split_at = 4.min(s.len());
            let (label, rest) = s.split_at(split_at);
            Line::from(vec![
                Span::styled(label.to_owned(), Style::default().fg(C_ACCENT).add_modifier(Modifier::BOLD)),
                Span::styled(rest.to_owned(),  Style::default().fg(C_TEXT)),
            ])
        }
        LineKind::UserMsg(full_prefix) => {
            let display_prefix = format!("{}: ", full_prefix);
            let rest = s.strip_prefix(&display_prefix).unwrap_or(s).to_owned();
            let bare_name = if full_prefix.starts_with('[') {
                if let Some(pos) = full_prefix.find("] ") {
                    &full_prefix[pos + 2..]
                } else {
                    &full_prefix
                }
            } else {
                &full_prefix
            };
            let hue = bare_name.bytes().fold(0u32, |a, b| a.wrapping_add(b as u32)) % 6;
            let name_color = [
                Color::Cyan,
                Color::Yellow,
                Color::Red,
                Color::Magenta,
                Color::Green,
                Color::Blue,
            ][hue as usize];

            if full_prefix.starts_with('[') {
                if let Some(bracket_end) = full_prefix.find("] ") {
                    let role_tag = &full_prefix[..bracket_end + 1];
                    let name_part = &full_prefix[bracket_end + 2..];
                    return Line::from(vec![
                        Span::styled(role_tag.to_owned(), Style::default().fg(C_DIM)),
                        Span::styled(" ".to_owned(),      Style::default()),
                        Span::styled(name_part.to_owned(), Style::default().fg(name_color).add_modifier(Modifier::BOLD)),
                        Span::styled(": ".to_owned(),      Style::default().fg(C_DIM)),
                        Span::styled(rest,                 Style::default().fg(C_TEXT)),
                    ]);
                }
            }

            Line::from(vec![
                Span::styled(full_prefix, Style::default().fg(name_color).add_modifier(Modifier::BOLD)),
                Span::styled(": ".to_owned(),  Style::default().fg(C_DIM)),
                Span::styled(rest,             Style::default().fg(C_TEXT)),
            ])
        }
        LineKind::Plain => Line::from(Span::styled(s.to_owned(), Style::default().fg(C_TEXT))),
    }
}

struct Autocomplete {
    candidates: Vec<String>,
    index: Option<usize>,
    prefix: String,
}

impl Autocomplete {
    fn new() -> Self {
        Self { candidates: Vec::new(), index: None, prefix: String::new() }
    }

    fn populate(&mut self, input: &str, members: &[String]) {
        self.prefix = input.to_string();
        self.candidates.clear();
        self.index = None;

        if input.starts_with('/') {
            let state = MY_STATE.lock().ok();
            let role = MY_ROLE.lock().ok();
            let role_str = role.as_deref().map(|s| s.as_str()).unwrap_or("");

            let mut available: Vec<&str> = COMMANDS_ALWAYS.iter().copied().collect();

            match state.as_deref() {
                Some(ClientState::Guest) | None => {
                    available.extend(COMMANDS_GUEST.iter().copied());
                }
                Some(ClientState::LoggedIn) => {
                    available.extend(COMMANDS_LOGGEDIN.iter().copied());
                }
                Some(ClientState::InRoom) => {
                    available.extend(COMMANDS_LOGGEDIN.iter().copied());
                    available.extend(COMMANDS_INROOM.iter().copied());
                    match role_str {
                        "owner" | "admin" => {
                            available.extend(COMMANDS_MOD.iter().copied());
                            available.extend(COMMANDS_SUPER.iter().copied());
                        }
                        "moderator" => {
                            available.extend(COMMANDS_MOD.iter().copied());
                        }
                        _ => {}
                    }
                }
            }
            for cmd in available {
                if cmd.starts_with(input) {
                    self.candidates.push(cmd.to_string());
                }
            }
        } else if let Some(at_pos) = input.rfind('@') {
            let name_prefix = &input[at_pos + 1..];
            let base = &input[..at_pos + 1];
            for m in members {
                if m.to_lowercase().starts_with(&name_prefix.to_lowercase()) {
                    self.candidates.push(format!("{}{}", base, m));
                }
            }
        }
    }

    fn reset(&mut self) {
        self.candidates.clear();
        self.index = None;
        self.prefix.clear();
    }
}

struct App {
    messages: Vec<String>,
    input: String,
    should_quit: bool,
    autocomplete: Autocomplete,
    member_names: Vec<String>,

    status: String,
    scroll_offset: usize,
    input_history: Vec<String>,
    history_pos: Option<usize>,
    input_draft: String,
    popup_visible: bool,
    popup_selected: usize,
    popup_candidates: Vec<String>,
}

impl App {
    fn new() -> App {
        App {
            messages: Vec::new(),
            input: String::new(),
            should_quit: false,
            autocomplete: Autocomplete::new(),
            member_names: Vec::new(),

            status: String::from("Not logged in"),
            scroll_offset: 0,
            input_history: Vec::new(),
            history_pos: None,
            input_draft: String::new(),
            popup_visible: false,
            popup_selected: 0,
            popup_candidates: Vec::new(),
        }
    }

    fn push(&mut self, msg: String) {
        self.messages.push(msg);
    }

    fn refresh_member_names(&mut self) {
        if let Ok(m) = MEMBERS.0.lock() {
            self.member_names = m.keys().cloned().collect();
        }
    }

    fn update_status(&mut self) {
        let user = CURRENT_USER.lock().map(|u| u.clone()).unwrap_or_default();
        let room = CURRENT_ROOM.lock().map(|r| r.clone()).unwrap_or_default();
        self.status = match (user.is_empty(), room.is_empty()) {
            (true, _) => "Not logged in  ·  Press /help for commands".into(),
            (false, true) => format!("  {}  ·  In lobby", user),
            (false, false) => format!("  {}  ·  #{}", user, room),
        };
    }
}

fn handle_control_packets(stream: &mut TcpStream, msg: &str, tx: &Sender<AppMessage>) -> std::io::Result<()> {
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
            let mut state = MY_STATE.lock().map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
            *state = ClientState::LoggedIn;
        }
        {
            let mut u = CURRENT_USER.lock().map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
            *u = username.trim().to_string();
        }
        if let Ok(pub_b64) = generate_or_load_keys(username) {
            stream.write_all(format!("/pubkey {pub_b64}\n").as_bytes())?;
        }
        return Ok(());
    }

    if msg == "/ROOM_STATE" {
        let mut state = MY_STATE.lock().map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
        *state = ClientState::InRoom;
        return Ok(());
    }

    if let Some(role) = msg.strip_prefix("/ROLE ") {
        if let Ok(mut r) = MY_ROLE.lock() {
            *r = role.trim().to_string();
        }
        return Ok(());
    }

    if msg == "/LOBBY_STATE" {
        let mut state = MY_STATE.lock().map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
        *state = ClientState::LoggedIn;
        if let Ok(mut r) = CURRENT_ROOM.lock() { r.clear(); }
        if let Ok(mut r) = MY_ROLE.lock() { r.clear(); }
        return Ok(());
    }

    if msg == "/GUEST_STATE" {
        let mut state = MY_STATE.lock().map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
        *state = ClientState::Guest;
        if let Ok(mut u) = CURRENT_USER.lock() { u.clear(); }
        if let Ok(mut r) = CURRENT_ROOM.lock() { r.clear(); }
        if let Ok(mut r) = MY_ROLE.lock() { r.clear(); }
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
        let mut members = lock.lock().map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
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

fn parse_ansi(s: &str) -> Line<'static> {
    let mut spans = Vec::new();
    let mut current_text = String::new();
    let mut current_style = Style::default();

    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            if chars.peek() == Some(&'[') {
                chars.next();
                let mut sequence = String::new();
                for ch in chars.by_ref() {
                    sequence.push(ch);
                    if ch.is_ascii_alphabetic() {
                        break;
                    }
                }
                
                if sequence.ends_with('m') {
                    if !current_text.is_empty() {
                        spans.push(Span::styled(current_text.clone(), current_style));
                        current_text.clear();
                    }
                    
                    let code_str = &sequence[..sequence.len()-1];
                    let codes: Vec<&str> = code_str.split(';').collect();
                    let mut i = 0;
                    while i < codes.len() {
                        if let Ok(c) = codes[i].parse::<u32>() {
                            match c {
                                0 => current_style = Style::default(),
                                1 => current_style = current_style.add_modifier(Modifier::BOLD),
                                3 => current_style = current_style.add_modifier(Modifier::ITALIC),
                                4 => current_style = current_style.add_modifier(Modifier::UNDERLINED),
                                30..=37 => {
                                    let colors = [Color::Black, Color::Red, Color::Green, Color::Yellow, Color::Blue, Color::Magenta, Color::Cyan, Color::DarkGray];
                                    current_style = current_style.fg(colors[(c - 30) as usize]);
                                }
                                90..=97 => {
                                    let colors = [Color::DarkGray, Color::LightRed, Color::LightGreen, Color::LightYellow, Color::LightBlue, Color::LightMagenta, Color::LightCyan, Color::White];
                                    current_style = current_style.fg(colors[(c - 90) as usize]);
                                }
                                38 => {
                                    if i + 2 < codes.len() && codes[i+1] == "5" {
                                        if let Ok(n) = codes[i+2].parse::<u8>() {
                                            current_style = current_style.fg(Color::Indexed(n));
                                        }
                                        i += 2;
                                    } else if i + 4 < codes.len() && codes[i+1] == "2" {
                                        if let (Ok(r), Ok(g), Ok(b)) = (codes[i+2].parse::<u8>(), codes[i+3].parse::<u8>(), codes[i+4].parse::<u8>()) {
                                            current_style = current_style.fg(Color::Rgb(r, g, b));
                                        }
                                        i += 4;
                                    }
                                }
                                48 => {
                                    if i + 2 < codes.len() && codes[i+1] == "5" {
                                        if let Ok(n) = codes[i+2].parse::<u8>() {
                                            current_style = current_style.bg(Color::Indexed(n));
                                        }
                                        i += 2;
                                    } else if i + 4 < codes.len() && codes[i+1] == "2" {
                                        if let (Ok(r), Ok(g), Ok(b)) = (codes[i+2].parse::<u8>(), codes[i+3].parse::<u8>(), codes[i+4].parse::<u8>()) {
                                            current_style = current_style.bg(Color::Rgb(r, g, b));
                                        }
                                        i += 4;
                                    }
                                }
                                _ => {}
                            }
                        }
                        i += 1;
                    }
                }
            } else {
                chars.next();
            }
        } else {
            current_text.push(c);
        }
    }
    
    if !current_text.is_empty() {
        spans.push(Span::styled(current_text, current_style));
    }
    
    Line::from(spans)
}

fn handle_recv(stream: TcpStream, tx: Sender<AppMessage>) -> std::io::Result<()> {
    let stream_for_writing = stream.try_clone()?;
    let reader = BufReader::new(stream);

    for line in reader.lines() {
        match line {
            Ok(msg) => {
                if msg.starts_with("/enc ") {
                    if let Some(enc_line) = msg.strip_prefix("/enc ") {
                        if let Some((prefix, cipher_b64)) = enc_line.split_once(": ") {
                            match decrypt(cipher_b64) {
                                Ok(plaintext) => { let _ = tx.send(AppMessage::ServerMessage(format!("{}: {}", prefix, plaintext))); }
                                Err(e) => { let _ = tx.send(AppMessage::NetworkError(format!("Decryption error: {e}"))); }
                            }
                        } else {
                            let _ = tx.send(AppMessage::NetworkError("Malformed /enc message".into()));
                        }
                    }
                    continue;
                }

                if msg.starts_with('/') {
                    if let Err(e) = handle_control_packets(&mut stream_for_writing.try_clone()?, &msg, &tx) {
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

fn ui(f: &mut Frame, app: &App) {
    let area = f.area();

    f.render_widget(
        Block::default().style(Style::default().bg(C_BG)),
        area,
    );

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(3),
            Constraint::Length(3),
            Constraint::Length(1),
        ])
        .split(area);

    let title = Paragraph::new(
        Line::from(vec![
            Span::styled("  ◈ ", Style::default().fg(C_ACCENT).add_modifier(Modifier::BOLD)),
            Span::styled("StreamLine", Style::default().fg(C_ACCENT).add_modifier(Modifier::BOLD))
        ])
    )
    .style(Style::default().bg(C_SURFACE))
    .alignment(Alignment::Left);
    f.render_widget(title, chunks[0]);

    let msg_area = chunks[1];
    let inner_height = msg_area.height.saturating_sub(2) as usize;

    let total = app.messages.len();
    let max_scroll = total.saturating_sub(inner_height);
    let offset = app.scroll_offset.min(max_scroll);
    let first = max_scroll.saturating_sub(offset);

    let items: Vec<ListItem> = app.messages
        .iter()
        .skip(first)
        .take(inner_height)
        .map(|m| ListItem::new(styled_line(m)))
        .collect();

    let scroll_indicator = if offset > 0 {
        format!(" Messages  ↑{} lines up — ↓ to return ", offset)
    } else {
        " Messages ".to_string()
    };

    let messages_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(C_BORDER))
        .title(Span::styled(scroll_indicator, Style::default().fg(C_DIM)))
        .style(Style::default().bg(C_BG));

    let msg_list = List::new(items)
        .block(messages_block);

    f.render_widget(msg_list, msg_area);

    let input_area = chunks[2];

    let mut spans = vec![Span::styled(app.input.clone(), Style::default().fg(C_YELLOW))];
    spans.push(Span::styled("█".to_owned(), Style::default().fg(C_ACCENT).add_modifier(Modifier::SLOW_BLINK)));

    let input_title = if app.popup_visible {
        " Input  [↑↓] navigate · [Tab/Enter] accept · [Esc] close "
    } else {
        " Input  [Tab] autocomplete · [Esc] quit "
    };

    let input_widget = Paragraph::new(Line::from(spans))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(C_BORDER_ACTIVE))
                .title(Span::styled(input_title, Style::default().fg(C_DIM)))
                .style(Style::default().bg(C_SURFACE)),
        );
    f.render_widget(input_widget, input_area);

    if app.popup_visible && !app.popup_candidates.is_empty() {
        let max_visible: usize = 10;
        let total = app.popup_candidates.len();

        let (win_start, win_end) = if total <= max_visible {
            (0, total)
        } else {
            let half = max_visible / 2;
            let start = if app.popup_selected < half {
                0
            } else if app.popup_selected + half >= total {
                total - max_visible
            } else {
                app.popup_selected - half
            };
            (start, start + max_visible)
        };

        let popup_items: Vec<ListItem> = app.popup_candidates[win_start..win_end]
            .iter()
            .enumerate()
            .map(|(i, cmd)| {
                let actual_idx = win_start + i;
                let style = if actual_idx == app.popup_selected {
                    Style::default().fg(C_BG).bg(C_ACCENT).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(C_TEXT).bg(C_SURFACE)
                };
                ListItem::new(Span::styled(format!(" {} ", cmd), style))
            }).collect();

        let visible_count = win_end - win_start;
        let popup_height = visible_count as u16 + 2;
        let popup_width = app.popup_candidates.iter().map(|c| c.len()).max().unwrap_or(10) as u16 + 4;
        let popup_width = popup_width.min(input_area.width);

        let popup_title = if total > max_visible {
            format!(" Completions ({}/{}) ", app.popup_selected + 1, total)
        } else {
            " Completions ".to_string()
        };

        let popup_area = ratatui::layout::Rect {
            x: input_area.x,
            y: input_area.y.saturating_sub(popup_height),
            width: popup_width,
            height: popup_height,
        };

        let popup_block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(C_BORDER_ACTIVE))
            .title(Span::styled(popup_title, Style::default().fg(C_ACCENT2)))
            .style(Style::default().bg(C_SURFACE));

        let popup_list = List::new(popup_items).block(popup_block);
        f.render_widget(ratatui::widgets::Clear, popup_area);
        f.render_widget(popup_list, popup_area);
    }

    let status_line = Paragraph::new(Line::from(vec![
        Span::styled(app.status.clone(), Style::default().fg(C_ACCENT2)),
    ]))
    .style(Style::default().bg(C_SURFACE))
    .alignment(Alignment::Left);
    f.render_widget(status_line, chunks[3]);
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() > 2 {
        eprintln!("Usage: cargo run --bin client -- <ip:port|port>");
        std::process::exit(1);
    }

    let address = if args.len() == 2 {
        let arg = &args[1];
        match arg.parse::<u16>() {
            Ok(port) => format!("127.0.0.1:{}", port),
            Err(_) => {
                if arg.contains(':') { arg.clone() }
                else { eprintln!("Invalid format. Use <ip:port> or <port>."); std::process::exit(1); }
            }
        }
    } else {
        "127.0.0.1:8000".to_string()
    };

    let mut stream = TcpStream::connect(&address)?;
    let stream_clone = stream.try_clone()?;

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();
    app.push("Welcome to StreamLine!".into());
    app.push("  /account register <user> <pass> <confirm>  — create account".into());
    app.push("  /account login <user> <pass>              — sign in".into());
    app.push("  /room create <name>  ·  /room join <name> — rooms".into());
    app.push("  /help                                     — all commands".into());
    app.push("  [Tab]  autocomplete commands & @usernames".into());

    let (tx, rx) = mpsc::channel::<AppMessage>();
    let tx_clone = tx.clone();

    thread::spawn(move || { let _ = handle_recv(stream_clone, tx_clone); });

    let res = run_app(&mut terminal, &mut app, &mut stream, rx, tx);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;

    if let Err(err) = res { println!("{:?}", err); }
    Ok(())
}

fn run_app<B: Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
    stream: &mut TcpStream,
    rx: Receiver<AppMessage>,
    _tx: Sender<AppMessage>,
) -> io::Result<()>
where
    io::Error: From<<B as Backend>::Error>,
{
    loop {
        while let Ok(msg) = rx.try_recv() {
            let text = match msg {
                AppMessage::ServerMessage(s) => s,
                AppMessage::NetworkError(s) => format!("⚠ {}", s),
                AppMessage::ControlResult(s) => s,
            };
            let was_at_bottom = app.scroll_offset == 0;
            app.push(text);
            app.refresh_member_names();
            app.update_status();
            if !was_at_bottom {
                app.scroll_offset = app.scroll_offset.saturating_add(1);
            }
        }

        terminal.draw(|f| ui(f, app))?;

        if !event::poll(std::time::Duration::from_millis(50))? {
            continue;
        }

        let ev = event::read()?;

        if let Event::Mouse(me) = ev {
            use crossterm::event::MouseEventKind;
            match me.kind {
                MouseEventKind::ScrollUp => {
                    app.scroll_offset = app.scroll_offset.saturating_add(1);
                }
                MouseEventKind::ScrollDown => {
                    app.scroll_offset = app.scroll_offset.saturating_sub(1);
                }
                _ => {}
            }
            continue;
        }

        if let Event::Key(key) = ev {
            use crossterm::event::KeyEventKind;
            if key.kind == KeyEventKind::Release || key.kind == KeyEventKind::Repeat {
                continue;
            }

            match key.code {
                KeyCode::Enter => {
                    if app.popup_visible && !app.popup_candidates.is_empty() {
                        app.input = app.popup_candidates[app.popup_selected].clone();
                        app.popup_visible = false;
                        app.popup_candidates.clear();
                        app.popup_selected = 0;
                        app.autocomplete.reset();

                        continue;
                    }

                    app.autocomplete.reset();


                    let msg = app.input.trim().to_string();
                    app.input.clear();
                    app.history_pos = None;
                    app.input_draft.clear();

                    if msg.is_empty() { continue; }

                    if app.input_history.last().map(|s| s.as_str()) != Some(&msg) {
                        app.input_history.push(msg.clone());
                    }

                    if msg == "/quit" { return Ok(()); }

                    if msg.starts_with('/') {
                        if msg == "/clear" || msg == "/c" {
                            app.messages.clear();
                            continue;
                        }
                        if msg == "/ping" {
                            let now_ms = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
                            stream.write_all(format!("/ping {}\n", now_ms).as_bytes())?;
                            continue;
                        }
                        stream.write_all(format!("{}\n", msg).as_bytes())?;
                        continue;
                    }

                    {
                        let (lock, _) = &*MEMBERS;
                        if let Ok(mut map) = lock.lock() { map.clear(); }
                    }

                    if let Ok(state) = MY_STATE.lock() {
                        if let ClientState::InRoom = &*state {
                            let mut stream_clone = stream.try_clone()?;
                            stream.write_all(b"/members? full\n")?;
                            let members = get_room_members();
                            if !members.is_empty() {
                                let _ = broadcast_message(&mut stream_clone, &members, &msg);
                            }
                        } else {
                            stream.write_all(format!("{}\n", msg).as_bytes())?;
                        }
                    }
                }

                KeyCode::Tab => {
                    if app.popup_visible && !app.popup_candidates.is_empty() {
                        app.input = app.popup_candidates[app.popup_selected].clone();
                        app.popup_visible = false;
                        app.popup_candidates.clear();
                        app.popup_selected = 0;
                        app.autocomplete.reset();

                    } else {
                        app.refresh_member_names();
                        let members = app.member_names.clone();
                        let current = app.input.clone();
                        let mut probe = Autocomplete::new();
                        probe.populate(&current, &members);
                        if !probe.candidates.is_empty() {
                            app.popup_candidates = probe.candidates;
                            app.popup_selected = 0;
                            app.popup_visible = true;
                        }
                    }
                }

                KeyCode::Backspace => {
                    app.input.pop();
                    app.autocomplete.reset();

                    app.popup_visible = false;
                    app.popup_candidates.clear();
                    app.popup_selected = 0;
                }

                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => return Ok(()),

                KeyCode::Esc => {
                    if app.popup_visible {
                        app.popup_visible = false;
                        app.popup_candidates.clear();
                        app.popup_selected = 0;
                    } else {
                        return Ok(());
                    }
                }

                KeyCode::Char(c) => {
                    app.input.push(c);
                    app.popup_visible = false;
                    app.popup_candidates.clear();
                    app.popup_selected = 0;

                    app.autocomplete.reset();
                }

                KeyCode::Up => {
                    if app.popup_visible && !app.popup_candidates.is_empty() {
                        app.popup_selected = if app.popup_selected == 0 {
                            app.popup_candidates.len() - 1
                        } else {
                            app.popup_selected - 1
                        };
                    } else {
                        let hist_len = app.input_history.len();
                        if hist_len == 0 { continue; }
                        let new_pos = match app.history_pos {
                            None => {
                                app.input_draft = app.input.clone();
                                hist_len - 1
                            }
                            Some(0) => 0,
                            Some(p) => p - 1,
                        };
                        app.history_pos = Some(new_pos);
                        app.input = app.input_history[new_pos].clone();
                    }
                }
                KeyCode::Down => {
                    if app.popup_visible && !app.popup_candidates.is_empty() {
                        app.popup_selected = (app.popup_selected + 1) % app.popup_candidates.len();
                    } else if let Some(pos) = app.history_pos {
                        if pos + 1 < app.input_history.len() {
                            let new_pos = pos + 1;
                            app.history_pos = Some(new_pos);
                            app.input = app.input_history[new_pos].clone();
                        } else {
                            app.history_pos = None;
                            app.input = app.input_draft.clone();
                        }
                    }
                }
                KeyCode::PageUp => {
                    app.scroll_offset = app.scroll_offset.saturating_add(10);
                }
                KeyCode::PageDown => {
                    app.scroll_offset = app.scroll_offset.saturating_sub(10);
                }
                KeyCode::Home => {
                    let total = app.messages.len();
                    let inner = 20usize;
                    app.scroll_offset = total.saturating_sub(inner);
                }
                KeyCode::End => {
                    app.scroll_offset = 0;
                }
                _ => {}
            }
        }

        if app.should_quit {
            return Ok(());
        }
    }
}
