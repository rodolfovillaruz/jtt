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
            // Find the last space in chars[0..=width]  (Python: rfind(" ", 0, width+1))
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

            // lstrip on remainder
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

        // Truncate label if needed.
        let mut display_label = label.to_string();
        if char_len(&display_label) > max_inner_possible {
            let keep = max_inner_possible.saturating_sub(1);
            display_label = format!("{}…", take_chars(&display_label, keep));
        }

        // Wrap the body.
        let wrapped_body = wrap(&body, max_inner_possible);

        // Required inner width.
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

        // Build the bubble.
        let dash = "─".repeat(w.saturating_sub(2));
        let mut bubble: Vec<String> = Vec::new();

        bubble.push(format!("╭{}╮", dash));
        bubble.push(format!("│ {} │", pad_right(&display_label, inner_width)));
        bubble.push(format!("├{}┤", dash));
        for line in &wrapped_body {
            bubble.push(format!("│ {} │", pad_right(line, inner_width)));
        }
        bubble.push(format!("╰{}╯", dash));

        // Alignment.
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
        println!("{}", text);
        return;
    }

    let pager = minus::Pager::new();
    if pager.push_str(text).is_err() {
        println!("{}", text);
        return;
    }
    // Scroll to bottom on launch, like `less +G`.
    let _ = pager.follow_output(true);

    if minus::page_all(pager).is_err() {
        println!("{}", text);
    }
}

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
