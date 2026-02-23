use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, Paragraph},
    Frame,
};

use crate::frontend::app::{App, AVAILABLE_ROOMS, VISIBLE_USERS, MY_STATE, ClientState};

pub const C_BG: Color = Color::Rgb(18, 18, 18);
pub const C_SURFACE: Color = Color::Rgb(28, 28, 28);
pub const C_BORDER: Color = Color::Rgb(60, 60, 60);
pub const C_BORDER_ACTIVE: Color = Color::Rgb(160, 160, 160);
pub const C_TEXT: Color = Color::Rgb(240, 240, 240);
pub const C_DIM: Color = Color::DarkGray;
pub const C_ACCENT: Color = Color::Rgb(200, 200, 200);
pub const C_ACCENT2: Color = Color::Rgb(150, 150, 150);
pub const C_YELLOW: Color = Color::Rgb(220, 220, 220);
pub const C_RED: Color = Color::Red;
pub const C_GREEN: Color = Color::Rgb(120, 160, 120);
pub const C_SYSTEM: Color = Color::Gray;

pub enum LineKind {
    System,
    Error,
    Success,
    SelfMsg,
    UserMsg(String),
    Plain,
}

pub fn classify_line(s: &str) -> LineKind {
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

pub fn styled_line(s: &str) -> Line<'static> {
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
            let display_prefix = format!("{full_prefix}: ");
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

pub fn wrap_line(line: Line<'static>, width: usize) -> Vec<Line<'static>> {
    if width == 0 {
        return vec![line];
    }
    let mut lines = Vec::new();
    let mut current_spans = Vec::new();
    let mut current_width = 0;

    for span in line.spans {
        let style = span.style;
        let content = span.content;
        let words = content.split_inclusive(' ');

        for word in words {
            let word_len = word.len();
            if current_width + word_len <= width {
                current_spans.push(Span::styled(word.to_string(), style));
                current_width += word_len;
            } else {
                let mut remaining_word = word;
                while !remaining_word.is_empty() {
                    let space_left = width.saturating_sub(current_width);
                    if space_left == 0 {
                        lines.push(Line::from(std::mem::take(&mut current_spans)));
                        current_width = 0;
                        continue;
                    }

                    let split_at = remaining_word.len().min(space_left);
                    let (head, tail) = remaining_word.split_at(split_at);
                    current_spans.push(Span::styled(head.to_string(), style));
                    current_width += head.len();
                    remaining_word = tail;

                    if current_width >= width {
                        lines.push(Line::from(std::mem::take(&mut current_spans)));
                        current_width = 0;
                    }
                }
            }
        }
    }
    if !current_spans.is_empty() {
        lines.push(Line::from(current_spans));
    }
    lines
}

pub fn parse_ansi(s: &str) -> Line<'static> {
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

pub fn ui(f: &mut Frame, app: &mut App) {
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

    let state = MY_STATE.lock().ok();
    let is_guest = matches!(state.as_deref(), Some(ClientState::Guest) | None);
    
    let (msg_area, panel_area) = if is_guest {
        (chunks[1], chunks[1])
    } else {
        let msg_panel_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(80),
                Constraint::Percentage(20),
            ])
            .split(chunks[1]);
        (msg_panel_chunks[0], msg_panel_chunks[1])
    };
    
    let inner_height = msg_area.height.saturating_sub(2) as usize;
    let inner_width = msg_area.width.saturating_sub(2) as usize;

    let display_buffer = 300;
    let start_idx = app.messages.len().saturating_sub(display_buffer);
    
    let mut all_lines = Vec::new();
    for m in app.messages.iter().skip(start_idx) {
        all_lines.extend(wrap_line(styled_line(m), inner_width));
    }

    let total_lines = all_lines.len();
    let max_scroll = total_lines.saturating_sub(inner_height);
    app.scroll_offset = app.scroll_offset.min(max_scroll);
    let offset = app.scroll_offset;
    
    let first = max_scroll.saturating_sub(offset);
    let visible_lines: Vec<ListItem> = all_lines
        .into_iter()
        .skip(first)
        .take(inner_height)
        .map(ListItem::new)
        .collect();

    let scroll_indicator = if offset > 0 {
        format!(" Messages  ↑{offset} lines up — ↓ to return ")
    } else {
        " Messages ".to_string()
    };

    let messages_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(C_BORDER))
        .title(Span::styled(scroll_indicator, Style::default().fg(C_DIM)))
        .style(Style::default().bg(C_BG));

    let msg_list = List::new(visible_lines)
        .block(messages_block);

    f.render_widget(msg_list, msg_area);

    if !is_guest {
        match state.as_deref() {
            Some(ClientState::LoggedIn) => {
                let rooms = AVAILABLE_ROOMS.lock().ok().map(|r| r.clone()).unwrap_or_default();
                let room_items: Vec<ListItem> = if rooms.is_empty() {
                    vec![ListItem::new(Line::from(Span::styled(
                        "  No rooms available",
                        Style::default().fg(C_DIM)
                    )))]
            } else {
                rooms.iter().map(|(name, count)| {
                    let text = if *count == 1 {
                        format!("{name} ({count} user)")
                    } else {
                        format!("{name} ({count} users)")
                    };
                    ListItem::new(Line::from(Span::styled(text, Style::default().fg(C_TEXT))))
                }).collect()
            };
            
            let panel_block = Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(C_BORDER))
                .title(Span::styled(" Rooms ", Style::default().fg(C_DIM)))
                .style(Style::default().bg(C_BG));
            
            let panel_list = List::new(room_items).block(panel_block);
            f.render_widget(panel_list, panel_area);
        }
        Some(ClientState::InRoom) => {
            let users = VISIBLE_USERS.lock().ok().map(|u| u.clone()).unwrap_or_default();
            let user_items: Vec<ListItem> = if users.is_empty() {
                vec![ListItem::new(Line::from(Span::styled(
                    "  No users online",
                    Style::default().fg(C_DIM)
                )))]
            } else {
                users.iter().map(|formatted_user| {
                    let line = if formatted_user.contains('\x1b') {
                        parse_ansi(formatted_user)
                    } else {
                        Line::from(Span::styled(
                            formatted_user.clone(),
                            Style::default().fg(C_TEXT)
                        ))
                    };

                    let mut spans = vec![Span::styled("".to_string(), Style::default())];
                    spans.extend(line.spans);
                    ListItem::new(Line::from(spans))
                }).collect()
            };
            
            let panel_block = Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(C_BORDER))
                .title(Span::styled(" Users ", Style::default().fg(C_DIM)))
                .style(Style::default().bg(C_BG));
            
            let panel_list = List::new(user_items).block(panel_block);
            f.render_widget(panel_list, panel_area);
            }
            _ => {}
        }
    }

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
                ListItem::new(Span::styled(format!(" {cmd} "), style))
            }).collect();

        let visible_count = win_end - win_start;
        let popup_height = visible_count as u16 + 2;
        let popup_title = if total > max_visible {
            format!(" Completions ({}/{}) ", app.popup_selected + 1, total)
        } else {
            " Completions ".to_string()
        };
        let longest_cmd_len = app.popup_candidates.iter().map(|c| c.len()).max().unwrap_or(0) as u16;
        let title_len = popup_title.len() as u16;
        let popup_width = (title_len + 2).max(longest_cmd_len + 4).min(input_area.width);

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
