//! Pretty-print JSON chat as left/right 75 % bubbles.  No timestamps.
//! System messages span 0 % → 100 % of terminal width.
//! Tab characters inside the JSON content are replaced by 4 spaces
//! so they don't break the border drawing.

use clap::Parser;
use serde::Deserialize;
use std::fs;
use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::ExitCode;
use terminal_size::{terminal_size, Width};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

// ─── CLI ─────────────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(about = "Pretty-print JSON chat as 75 % bubbles.")]
struct Args {
    /// Path to JSON chat file
    input: PathBuf,
}

// ─── Data model ──────────────────────────────────────────────────────────────

#[derive(Deserialize, Debug)]
struct Turn {
    #[serde(default)]
    role: String,
    #[serde(default)]
    content: String,
}

// ─── Terminal helpers ─────────────────────────────────────────────────────────

fn term_width() -> usize {
    if let Some((Width(w), _)) = terminal_size() {
        w as usize
    } else {
        80
    }
}

/// Returns the number of **terminal columns** a string occupies.
fn display_width(s: &str) -> usize {
    s.width()
}

/// Pad `s` to exactly `width` **columns** with trailing spaces.
fn pad_right(s: &str, width: usize) -> String {
    let w = display_width(s);
    if w >= width {
        s.to_string()
    } else {
        let mut out = s.to_string();
        out.push_str(&" ".repeat(width - w));
        out
    }
}

/// Take characters from `s` until the accumulated column count would exceed `cols`.
fn take_cols(s: &str, cols: usize) -> String {
    let mut out = String::new();
    let mut used = 0;
    for ch in s.chars() {
        let w = UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + w > cols {
            break;
        }
        out.push(ch);
        used += w;
    }
    out
}

/// Word-wrap `text` so that no line exceeds `cols` **terminal columns**.
fn wrap(text: &str, cols: usize) -> Vec<String> {
    let cols = cols.max(1);
    let mut lines: Vec<String> = Vec::new();

    for para in text.split('\n') {
        let mut remaining = para;

        loop {
            if display_width(remaining) <= cols {
                lines.push(remaining.to_string());
                break;
            }

            let mut col: usize = 0;
            let mut last_space: Option<usize> = None;
            let mut hard_cut: usize = remaining.len();

            for (byte_idx, ch) in remaining.char_indices() {
                let ch_w = UnicodeWidthChar::width(ch).unwrap_or(0);
                if col + ch_w > cols {
                    hard_cut = byte_idx;
                    break;
                }
                if ch == ' ' {
                    last_space = Some(byte_idx);
                }
                col += ch_w;
            }

            let split_at = last_space.unwrap_or(hard_cut);
            lines.push(remaining[..split_at].trim_end().to_string());
            remaining = remaining[split_at..].trim_start_matches(' ');
        }
    }

    lines
}

// ─── Chat renderer ───────────────────────────────────────────────────────────

/// Render turns into individual display lines using the given terminal width.
/// Called once at startup and again every time the terminal is resized.
fn render_lines(data: &[Turn], width: usize) -> Vec<String> {
    let user_label = "You";
    let assistant_label = "Assistant";

    let bubble_max_width = width * 75 / 100;
    let mut lines: Vec<String> = Vec::new();

    for turn in data {
        let role = turn.role.to_lowercase();
        let body = turn.content.replace('\t', "    ");

        let label = match role.as_str() {
            "user" => user_label,
            "assistant" => assistant_label,
            "system" => "System",
            _ => "Unknown",
        };

        let (side, max_outer) = match role.as_str() {
            "user" => ("right", bubble_max_width),
            "system" => ("full", width),
            _ => ("left", bubble_max_width),
        };

        let max_inner_possible = if max_outer > 4 { max_outer - 4 } else { 1 };

        let mut display_label = label.to_string();
        if display_width(&display_label) > max_inner_possible {
            let keep = max_inner_possible.saturating_sub(1);
            display_label = format!("{}…", take_cols(&display_label, keep));
        }

        let wrapped_body = wrap(&body, max_inner_possible);

        let body_max = wrapped_body
            .iter()
            .map(|l| display_width(l))
            .max()
            .unwrap_or(0);
        let mut inner_width = display_width(&display_label).max(body_max);

        let mut w = inner_width + 4;
        if w > max_outer {
            inner_width = max_outer.saturating_sub(4);
            w = max_outer;
            if display_width(&display_label) > inner_width {
                display_label = take_cols(&display_label, inner_width);
            }
        }

        let dash = "─".repeat(w.saturating_sub(2));
        let mut bubble: Vec<String> = Vec::new();

        bubble.push(format!("╭{}╮", dash));
        bubble.push(format!("│ {} │", pad_right(&display_label, inner_width)));
        bubble.push(format!("├{}┤", dash));
        for line in &wrapped_body {
            bubble.push(format!("│ {} │", pad_right(line, inner_width)));
        }
        bubble.push(format!("╰{}╯", dash));

        let left_pad = if side == "right" {
            width.saturating_sub(w)
        } else {
            0
        };
        let pad_str = " ".repeat(left_pad);

        for b in bubble {
            lines.push(format!("{}{}", pad_str, b));
        }
        lines.push(String::new());
    }

    lines
}

fn json_to_pretty_chat(data: &[Turn]) -> String {
    render_lines(data, term_width()).join("\n")
}

// ─── JSON loader ─────────────────────────────────────────────────────────────

fn load_json(path: &PathBuf) -> Result<Vec<Turn>, Box<dyn std::error::Error>> {
    let raw = fs::read_to_string(path)?;
    let v: serde_json::Value = serde_json::from_str(&raw)?;
    if v.is_array() {
        Ok(serde_json::from_value(v)?)
    } else {
        Ok(vec![serde_json::from_value(v)?])
    }
}

// ─── Ratatui pager ───────────────────────────────────────────────────────────

fn ratatui_page(turns: &[Turn]) {
    use crossterm::{
        event::{self, Event, KeyCode, KeyModifiers},
        execute,
        terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
    };
    use ratatui::{
        backend::CrosstermBackend,
        layout::{Rect, Size},
        style::{Modifier, Style},
        text::{Line, Text},
        widgets::Paragraph,
        Terminal,
    };
    use std::io::stdout;

    terminal::enable_raw_mode().expect("enable raw mode");
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen).expect("enter alternate screen");
    let mut terminal =
        Terminal::new(CrosstermBackend::new(stdout)).expect("create ratatui terminal");

    // Render at the actual initial terminal width reported by the backend.
    let initial_size = terminal.size().unwrap_or(Size {
        width: 80,
        height: 24,
    });
    let mut lines = render_lines(turns, initial_size.width as usize);
    let mut total = lines.len();
    let mut offset: usize = 0;

    loop {
        terminal
            .draw(|frame| {
                let area = frame.area();
                let ph = (area.height as usize).saturating_sub(1);
                let end = (offset + ph).min(total);

                let content_rect = Rect {
                    x: area.x,
                    y: area.y,
                    width: area.width,
                    height: area.height.saturating_sub(1),
                };
                let visible = Text::from(
                    lines[offset..end]
                        .iter()
                        .map(|l| Line::raw(l.as_str()))
                        .collect::<Vec<_>>(),
                );
                frame.render_widget(Paragraph::new(visible), content_rect);

                let status_rect = Rect {
                    x: area.x,
                    y: area.y + area.height.saturating_sub(1),
                    width: area.width,
                    height: 1,
                };
                let pct = if total == 0 { 100 } else { end * 100 / total };
                let status = format!(
                    " {first}–{last}/{total} ({pct}%){end_marker}\
                     \u{2502} q:quit  \u{2191}\u{2193}/jk:line  \
                     PgUp/PgDn:page  g/G:top/bot ",
                    first = if total == 0 { 0 } else { offset + 1 },
                    last = end,
                    total = total,
                    pct = pct,
                    end_marker = if end >= total { " END " } else { " " },
                );
                let bar =
                    Paragraph::new(status).style(Style::default().add_modifier(Modifier::REVERSED));
                frame.render_widget(bar, status_rect);
            })
            .expect("draw frame");

        match event::read().expect("read event") {
            Event::Key(key) => {
                let ph = terminal
                    .size()
                    .map(|s| (s.height as usize).saturating_sub(1))
                    .unwrap_or(23);

                match key.code {
                    KeyCode::Char('q') | KeyCode::Char('Q') => break,
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => break,
                    KeyCode::Down | KeyCode::Char('j') | KeyCode::Enter => {
                        if offset + ph < total {
                            offset += 1;
                        }
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        offset = offset.saturating_sub(1);
                    }
                    KeyCode::PageDown | KeyCode::Char(' ') | KeyCode::Char('f') => {
                        offset = (offset + ph).min(total.saturating_sub(ph));
                    }
                    KeyCode::PageUp | KeyCode::Char('b') => {
                        offset = offset.saturating_sub(ph);
                    }
                    KeyCode::Home | KeyCode::Char('g') => offset = 0,
                    KeyCode::End | KeyCode::Char('G') => {
                        offset = total.saturating_sub(ph);
                    }
                    _ => {}
                }
            }

            Event::Resize(new_w, new_h) => {
                // Re-wrap all bubbles at the new terminal width.
                // Preserve the fractional scroll position so the user
                // stays roughly at the same place in the conversation.
                let frac = if total > 0 {
                    offset as f64 / total as f64
                } else {
                    0.0
                };

                lines = render_lines(turns, new_w as usize);
                total = lines.len();

                let ph = (new_h as usize).saturating_sub(1);
                offset = ((frac * total as f64) as usize).min(total.saturating_sub(ph));
            }

            _ => {}
        }
    }

    terminal::disable_raw_mode().expect("disable raw mode");
    execute!(terminal.backend_mut(), LeaveAlternateScreen).expect("leave alternate screen");
    terminal.show_cursor().expect("show cursor");
}

// ─── Output dispatcher ───────────────────────────────────────────────────────

fn page_output(text: &str, turns: &[Turn]) {
    if !std::io::stdout().is_terminal() {
        // Piped / redirected — just emit the pre-rendered text.
        print!("{}", text);
        return;
    }
    ratatui_page(turns);
}

// ─── Entry point ─────────────────────────────────────────────────────────────

fn main() -> ExitCode {
    let args = Args::parse();
    match load_json(&args.input) {
        Ok(chat) => {
            // Render once for the non-terminal (pipe) path.
            let output = json_to_pretty_chat(&chat);
            page_output(&output, &chat);
            ExitCode::from(0)
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            ExitCode::from(1)
        }
    }
}
