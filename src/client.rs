use std::io::{self, Write};
use std::net::TcpStream;
use std::sync::mpsc::{self, Receiver, Sender};
use std::{env, thread};
use std::time::{SystemTime, UNIX_EPOCH};

mod shared;
mod frontend;

use crate::shared::crypto::{broadcast_message};
use crate::frontend::app::{App, AppMessage, ClientState, MY_STATE, get_room_members};
use crate::frontend::ui::ui;
use crate::frontend::network::handle_recv;

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    Terminal,
};

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


                    if let Ok(state) = MY_STATE.lock() {
                        if let ClientState::InRoom = &*state {
                            let members = get_room_members();
                            if !members.is_empty() {
                                let mut stream_clone = stream.try_clone()?;
                                let _ = broadcast_message(&mut stream_clone, &members, &msg);
                            } else {
                                stream.write_all(b"/members? full\n")?;
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
                        app.autocomplete.populate(&current, &members);
                        if !app.autocomplete.candidates.is_empty() {
                            app.popup_candidates = app.autocomplete.candidates.clone();
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
                    app.scroll_offset = total.saturating_sub(20);
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
