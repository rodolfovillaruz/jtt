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

fn char_len(s: &str) -> usize {
    s.chars().count()
}

fn pad_right(s: &str, width: usize) -> String {
    let len = char_len(s);
    if len >= width {
        s.to_string()
    } else {
        let mut out = String::from(s);
        out.push_str(&" ".repeat(width - len));
        out
    }
}

fn take_chars(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

fn wrap(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut lines = Vec::new();

    for para in text.split('\n') {
        let mut chars: Vec<char> = para.chars().collect();

        while chars.len() > width {
            let upper = width.min(chars.len().saturating_sub(1));
            let mut split: Option<usize> = None;
            for i in (0..=upper).rev() {
                if chars[i] == ' ' {
                    split = Some(i);
                    break;
                }
            }
            let split_idx = split.unwrap_or(width);

            let first: String = chars[..split_idx].iter().collect();
            lines.push(first.trim_end().to_string());

            let mut rest_start = split_idx;
            while rest_start < chars.len() && chars[rest_start].is_whitespace() {
                rest_start += 1;
            }
            chars = chars[rest_start..].to_vec();
        }

        lines.push(chars.iter().collect());
    }

    lines
}

// ─── Chat renderer ───────────────────────────────────────────────────────────

fn json_to_pretty_chat(data: &[Turn]) -> String {
    let user_label = "You";
    let assistant_label = "Assistant";

    let width = term_width();
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
            "assistant" => ("left", bubble_max_width),
            "system" => ("full", width),
            _ => ("left", bubble_max_width),
        };

        let max_inner_possible = if max_outer > 4 { max_outer - 4 } else { 1 };

        let mut display_label = label.to_string();
        if char_len(&display_label) > max_inner_possible {
            let keep = max_inner_possible.saturating_sub(1);
            display_label = format!("{}…", take_chars(&display_label, keep));
        }

        let wrapped_body = wrap(&body, max_inner_possible);

        let body_max_len = wrapped_body.iter().map(|l| char_len(l)).max().unwrap_or(0);
        let mut inner_width = char_len(&display_label).max(body_max_len);

        let mut w = inner_width + 4;
        if w > max_outer {
            inner_width = max_outer.saturating_sub(4);
            w = max_outer;
            if char_len(&display_label) > inner_width {
                display_label = take_chars(&display_label, inner_width);
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

    lines.join("\n")
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

/// Full-screen, keyboard-driven pager backed by ratatui + crossterm.
///
/// Keys
/// ────
/// q / Q / Ctrl-C        quit
/// ↓  j  Enter           scroll one line down
/// ↑  k                  scroll one line up
/// PgDn  Space  f        scroll one page down
/// PgUp  b               scroll one page up
/// g  Home               jump to top
/// G  End                jump to bottom
fn ratatui_page(text: &str) {
    use crossterm::{
        event::{self, Event, KeyCode, KeyModifiers},
        execute,
        terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
    };
    use ratatui::{
        backend::CrosstermBackend,
        layout::Rect,
        style::{Modifier, Style},
        text::{Line, Text},
        widgets::Paragraph,
        Terminal,
    };
    use std::io::stdout;

    // ── Pre-process text into lines ──────────────────────────────────────────
    let lines: Vec<&str> = text.lines().collect();
    let total = lines.len();
    let mut offset: usize = 0;

    // ── Terminal setup ───────────────────────────────────────────────────────
    terminal::enable_raw_mode().expect("enable raw mode");
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen).expect("enter alternate screen");
    let mut terminal =
        Terminal::new(CrosstermBackend::new(stdout)).expect("create ratatui terminal");

    // ── Event loop ───────────────────────────────────────────────────────────
    loop {
        // Draw one frame.
        terminal
            .draw(|frame| {
                let area = frame.area();
                let total_h = area.height as usize;
                let ph = total_h.saturating_sub(1); // rows available for content
                let end = (offset + ph).min(total);

                // ── Content pane ─────────────────────────────────────────────
                let content_rect = Rect {
                    x: area.x,
                    y: area.y,
                    width: area.width,
                    height: area.height.saturating_sub(1),
                };

                let visible = Text::from(
                    lines[offset..end]
                        .iter()
                        .map(|l| Line::raw(*l))
                        .collect::<Vec<_>>(),
                );
                // No ratatui-level wrapping: the text is already pre-wrapped.
                frame.render_widget(Paragraph::new(visible), content_rect);

                // ── Status bar ────────────────────────────────────────────────
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

        // Process one input event.
        match event::read().expect("read event") {
            Event::Key(key) => {
                // Recalculate page height from the live terminal size.
                let ph = terminal
                    .size()
                    .map(|s| (s.height as usize).saturating_sub(1))
                    .unwrap_or(23);

                match key.code {
                    // ── Quit ─────────────────────────────────────────────────
                    KeyCode::Char('q') | KeyCode::Char('Q') => break,
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        break;
                    }

                    // ── One line down ─────────────────────────────────────────
                    KeyCode::Down | KeyCode::Char('j') | KeyCode::Enter => {
                        if offset + ph < total {
                            offset += 1;
                        }
                    }

                    // ── One line up ───────────────────────────────────────────
                    KeyCode::Up | KeyCode::Char('k') => {
                        offset = offset.saturating_sub(1);
                    }

                    // ── One page down ─────────────────────────────────────────
                    KeyCode::PageDown | KeyCode::Char(' ') | KeyCode::Char('f') => {
                        offset = (offset + ph).min(total.saturating_sub(ph));
                    }

                    // ── One page up ───────────────────────────────────────────
                    KeyCode::PageUp | KeyCode::Char('b') => {
                        offset = offset.saturating_sub(ph);
                    }

                    // ── Jump to top ───────────────────────────────────────────
                    KeyCode::Home | KeyCode::Char('g') => offset = 0,

                    // ── Jump to bottom ────────────────────────────────────────
                    KeyCode::End | KeyCode::Char('G') => {
                        offset = total.saturating_sub(ph);
                    }

                    _ => {} // unknown key: just redraw on next iteration
                }
            }

            // Keep the offset sane when the window is resized.
            Event::Resize(_, h) => {
                let ph = (h as usize).saturating_sub(1);
                offset = offset.min(total.saturating_sub(ph));
            }

            _ => {}
        }
    }

    // ── Tear down ────────────────────────────────────────────────────────────
    terminal::disable_raw_mode().expect("disable raw mode");
    execute!(terminal.backend_mut(), LeaveAlternateScreen).expect("leave alternate screen");
    terminal.show_cursor().expect("show cursor");
}

// ─── Output dispatcher ───────────────────────────────────────────────────────

fn page_output(text: &str) {
    // When stdout is a pipe or file, just print directly.
    if !std::io::stdout().is_terminal() {
        print!("{}", text);
        return;
    }
    ratatui_page(text);
}

// ─── Entry point ─────────────────────────────────────────────────────────────

fn main() -> ExitCode {
    let args = Args::parse();
    match load_json(&args.input) {
        Ok(chat) => {
            let output = json_to_pretty_chat(&chat);
            page_output(&output);
            ExitCode::from(0)
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            ExitCode::from(1)
        }
    }
}
