/// Colored output helpers for terminal display.
/// All functions return formatted strings — the caller decides stderr vs stdout.

// ANSI color codes
const RESET: &str = "\x1b[0m";
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const RED: &str = "\x1b[31m";
const CYAN: &str = "\x1b[36m";
const BLUE: &str = "\x1b[34m";
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";

pub fn success(msg: &str) -> String {
    format!("{GREEN}{BOLD}  ✓ {msg}{RESET}")
}

pub fn info(msg: &str) -> String {
    format!("{CYAN}  ℹ {msg}{RESET}")
}

pub fn warn(msg: &str) -> String {
    format!("{YELLOW}  ⚠ {msg}{RESET}")
}

pub fn error_msg(msg: &str) -> String {
    format!("{RED}{BOLD}  ✗ {msg}{RESET}")
}

pub fn hint(msg: &str) -> String {
    format!("{DIM}  → {msg}{RESET}")
}

pub fn header(msg: &str) -> String {
    format!("\n{BOLD}{BLUE}{msg}{RESET}\n")
}

pub fn domain(msg: &str) -> String {
    format!("{CYAN}{msg}{RESET}")
}

pub fn bold(msg: &str) -> String {
    format!("{BOLD}{msg}{RESET}")
}

pub fn risk_color(risk: &str) -> &'static str {
    match risk.to_uppercase().as_str() {
        "CRITICAL" => RED,
        "HIGH" => YELLOW,
        "MEDIUM" => CYAN,
        "LOW" => GREEN,
        _ => RESET,
    }
}

pub fn colored_risk(risk: &str) -> String {
    let color = risk_color(risk);
    format!("{color}{BOLD}{risk}{RESET}")
}

// ---------------------------------------------------------------------------
// Table
// ---------------------------------------------------------------------------

pub struct Table {
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
}

impl Table {
    pub fn new(headers: Vec<&str>) -> Self {
        Self {
            headers: headers.into_iter().map(|s| s.to_string()).collect(),
            rows: Vec::new(),
        }
    }

    pub fn add_row(&mut self, row: Vec<String>) {
        self.rows.push(row);
    }

    pub fn render(&self) -> String {
        if self.headers.is_empty() {
            return String::new();
        }

        // Calculate column widths (using stripped lengths for ANSI)
        let mut widths: Vec<usize> = self.headers.iter().map(|h| h.len()).collect();
        for row in &self.rows {
            for (i, cell) in row.iter().enumerate() {
                if i < widths.len() {
                    let stripped_len = strip_ansi_len(cell);
                    if stripped_len > widths[i] {
                        widths[i] = stripped_len;
                    }
                }
            }
        }

        let mut out = String::new();

        // Header
        let header_line: Vec<String> = self
            .headers
            .iter()
            .enumerate()
            .map(|(i, h)| format!("{BOLD}{h:<width$}{RESET}", width = widths[i]))
            .collect();
        out.push_str("  ");
        out.push_str(&header_line.join("  "));
        out.push('\n');

        // Separator
        let sep: Vec<String> = widths.iter().map(|w| "─".repeat(*w)).collect();
        out.push_str("  ");
        out.push_str(&sep.join("  "));
        out.push('\n');

        // Rows
        for row in &self.rows {
            let cells: Vec<String> = row
                .iter()
                .enumerate()
                .map(|(i, cell)| {
                    let w = if i < widths.len() { widths[i] } else { 0 };
                    let stripped = strip_ansi_len(cell);
                    let padding = if w > stripped { w - stripped } else { 0 };
                    format!("{cell}{}", " ".repeat(padding))
                })
                .collect();
            out.push_str("  ");
            out.push_str(&cells.join("  "));
            out.push('\n');
        }

        out
    }
}

fn strip_ansi_len(s: &str) -> usize {
    let mut len = 0usize;
    let mut in_escape = false;
    for ch in s.chars() {
        if in_escape {
            if ch == 'm' {
                in_escape = false;
            }
        } else if ch == '\x1b' {
            in_escape = true;
        } else {
            len += 1;
        }
    }
    len
}

// ---------------------------------------------------------------------------
// Spinner (simple stderr spinner)
// ---------------------------------------------------------------------------

use std::sync::{Arc, Mutex};
use std::io::Write;

pub struct Spinner {
    message: Arc<Mutex<String>>,
    running: Arc<Mutex<bool>>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl Spinner {
    pub fn new(message: &str) -> Self {
        let msg = Arc::new(Mutex::new(message.to_string()));
        let running = Arc::new(Mutex::new(true));
        let msg_clone = Arc::clone(&msg);
        let running_clone = Arc::clone(&running);

        let handle = std::thread::spawn(move || {
            let frames = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
            let mut i = 0;
            loop {
                {
                    let r = running_clone.lock().unwrap();
                    if !*r {
                        break;
                    }
                }
                {
                    let m = msg_clone.lock().unwrap();
                    eprint!("\r{CYAN}  {} {}{RESET}  ", frames[i % frames.len()], *m);
                    let _ = std::io::stderr().flush();
                }
                i += 1;
                std::thread::sleep(std::time::Duration::from_millis(80));
            }
        });

        Self {
            message: msg,
            running,
            handle: Some(handle),
        }
    }

    pub fn update_message(&self, msg: &str) {
        let mut m = self.message.lock().unwrap();
        *m = msg.to_string();
    }

    pub fn stop_with_message(&mut self, msg: &str) {
        {
            let mut r = self.running.lock().unwrap();
            *r = false;
        }
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
        // Clear spinner line and print final message
        eprint!("\r\x1b[2K");
        eprintln!("{msg}");
    }
}

impl Drop for Spinner {
    fn drop(&mut self) {
        {
            let mut r = self.running.lock().unwrap();
            *r = false;
        }
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
        eprint!("\r\x1b[2K");
    }
}

// ---------------------------------------------------------------------------
// Utilities
// ---------------------------------------------------------------------------

pub fn format_number(n: usize) -> String {
    if n < 1_000 {
        return n.to_string();
    }
    let s = n.to_string();
    let mut result = String::new();
    for (i, ch) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(ch);
    }
    result.chars().rev().collect()
}
