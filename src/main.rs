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

#[derive(Parser)]
#[command(about = "Pretty-print JSON chat as 75 % bubbles.")]
struct Args {
    /// Path to JSON chat file
    input: PathBuf,
}

#[derive(Deserialize, Debug)]
struct Turn {
    #[serde(default)]
    role: String,
    #[serde(default)]
    content: String,
}

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

fn load_json(path: &PathBuf) -> Result<Vec<Turn>, Box<dyn std::error::Error>> {
    let raw = fs::read_to_string(path)?;
    let v: serde_json::Value = serde_json::from_str(&raw)?;
    if v.is_array() {
        Ok(serde_json::from_value(v)?)
    } else {
        Ok(vec![serde_json::from_value(v)?])
    }
}

fn page_output(text: &str) {
    if !std::io::stdout().is_terminal() {
        print!("{}", text);
        return;
    }
    if try_spawn_pager(text) {
        return;
    }
    builtin_page(text);
}

/// Try each external pager in priority order.
/// Returns true if one was successfully spawned and finished.
fn try_spawn_pager(text: &str) -> bool {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let mut candidates: Vec<(String, Vec<&'static str>)> = Vec::new();

    if let Ok(p) = std::env::var("PAGER") {
        candidates.push((p, vec![]));
    }
    candidates.push(("less".into(), vec!["-R"]));
    candidates.push(("more".into(), vec![]));

    for (prog, flags) in &candidates {
        let mut cmd = Command::new(prog);
        cmd.args(flags).stdin(Stdio::piped());

        if let Ok(mut child) = cmd.spawn() {
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(text.as_bytes());
            }
            let _ = child.wait();
            return true;
        }
    }
    false
}

// ─── built-in less-like pager ────────────────────────────────────────────────

/// Render one screenful plus the status bar.
/// `out` must already be in raw mode.
fn page_draw(
    out: &mut std::io::Stdout,
    lines: &[&str],
    offset: usize,
    term_w: usize,
    term_h: usize,
) {
    use crossterm::{
        cursor, execute,
        terminal::{Clear, ClearType},
    };
    use std::io::Write;

    // Reserve the last row for the status bar.
    let ph = term_h.saturating_sub(1);
    let total = lines.len();
    let end = (offset + ph).min(total);

    execute!(out, Clear(ClearType::All), cursor::MoveTo(0, 0)).unwrap();

    for line in &lines[offset..end] {
        // Raw mode: \n does NOT imply \r, so we write \r\n explicitly.
        write!(out, "{}\r\n", line).unwrap();
    }

    // ── Status bar ───────────────────────────────────────────────────────────
    let pct = if total == 0 { 100 } else { end * 100 / total };
    let at_end = end >= total;

    let status = format!(
        " {first}-{last}/{total} ({pct}%){end_marker}\
         \u{2502} q:quit  \u{2191}\u{2193}/jk:line  PgUp/PgDn:page  g/G:top/bot ",
        first = if total == 0 { 0 } else { offset + 1 },
        last = end,
        total = total,
        pct = pct,
        end_marker = if at_end { " END " } else { " " },
    );

    // Pad to terminal width, then hard-truncate so we never wrap.
    let bar: String = format!("{:<width$}", status, width = term_w)
        .chars()
        .take(term_w)
        .collect();

    execute!(out, cursor::MoveTo(0, ph as u16)).unwrap();
    // Reverse-video highlight for the status bar.
    write!(out, "\x1b[7m{}\x1b[0m", bar).unwrap();

    out.flush().unwrap();
}

/// Interactive less-like pager used when no external pager is available.
///
/// Keys
/// ────
/// q / Q / Ctrl-C   quit
/// ↓  j  Enter      scroll one line down
/// ↑  k             scroll one line up
/// PgDn  Space  f   scroll one page down
/// PgUp  b           scroll one page up
/// g  Home           jump to top
/// G  End            jump to bottom
fn builtin_page(text: &str) {
    use crossterm::{
        cursor,
        event::{self, Event, KeyCode, KeyModifiers},
        execute,
        terminal::{self, ClearType},
    };
    use std::io::{stdout, Write};

    let lines: Vec<&str> = text.lines().collect();
    let total = lines.len();

    terminal::enable_raw_mode().expect("failed to enable raw mode");
    let mut out = stdout();

    let (mut term_w, mut term_h) = terminal::size().unwrap_or((80, 24));
    let mut offset: usize = 0;

    page_draw(&mut out, &lines, offset, term_w as usize, term_h as usize);

    loop {
        match event::read().unwrap() {
            // ── keyboard ─────────────────────────────────────────────────────
            Event::Key(key) => {
                let ph = (term_h as usize).saturating_sub(1);

                match key.code {
                    // Quit
                    KeyCode::Char('q') | KeyCode::Char('Q') => break,
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => break,

                    // Scroll one line down
                    KeyCode::Down | KeyCode::Char('j') | KeyCode::Enter => {
                        if offset + ph < total {
                            offset += 1;
                        }
                    }

                    // Scroll one line up
                    KeyCode::Up | KeyCode::Char('k') => {
                        offset = offset.saturating_sub(1);
                    }

                    // Scroll one page down
                    KeyCode::PageDown | KeyCode::Char(' ') | KeyCode::Char('f') => {
                        offset = offset.saturating_add(ph).min(total.saturating_sub(ph));
                    }

                    // Scroll one page up
                    KeyCode::PageUp | KeyCode::Char('b') => {
                        offset = offset.saturating_sub(ph);
                    }

                    // Jump to top
                    KeyCode::Home | KeyCode::Char('g') => offset = 0,

                    // Jump to bottom
                    KeyCode::End | KeyCode::Char('G') => {
                        offset = total.saturating_sub(ph);
                    }

                    _ => continue, // unrecognised key → no redraw
                }

                page_draw(&mut out, &lines, offset, term_w as usize, term_h as usize);
            }

            // ── terminal resize ───────────────────────────────────────────────
            Event::Resize(w, h) => {
                term_w = w;
                term_h = h;
                let ph = (h as usize).saturating_sub(1);
                // Keep offset sane after resize.
                offset = offset.min(total.saturating_sub(ph));
                page_draw(&mut out, &lines, offset, term_w as usize, term_h as usize);
            }

            _ => {}
        }
    }

    // Restore terminal state and clear the screen before returning.
    terminal::disable_raw_mode().expect("failed to disable raw mode");
    execute!(
        out,
        crossterm::terminal::Clear(ClearType::All),
        cursor::MoveTo(0, 0)
    )
    .unwrap();
    out.flush().unwrap();
}

// ─── entry point ─────────────────────────────────────────────────────────────

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
