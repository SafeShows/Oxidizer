//! The Oxidizer window: loaded mods with their state, and the recent log.
//! Opened from Mods > Oxidizer in the game's menu bar.

use oxidizer_api::imgui;
use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

const LOG_LINES: usize = 300;
const RED: [f32; 4] = [1.0, 0.45, 0.4, 1.0];
const GREEN: [f32; 4] = [0.5, 0.9, 0.5, 1.0];
const GRAY: [f32; 4] = [0.6, 0.6, 0.6, 1.0];

static SHOW: AtomicBool = AtomicBool::new(false);
/// The last LOG_LINES lines, and the number the next line gets (lines are numbered from 0).
static LOG: Mutex<(VecDeque<String>, u64)> = Mutex::new((VecDeque::new(), 0));
static MODS: Mutex<Vec<Status>> = Mutex::new(Vec::new());

#[derive(Clone)]
struct Status {
    id: String,
    kind: &'static str,
    /// None = running.
    problem: Option<String>,
}

pub fn push_log(line: String) {
    let (log, next) = &mut *LOG.lock().unwrap_or_else(|e| e.into_inner());
    if log.len() == LOG_LINES {
        log.pop_front();
    }
    log.push_back(line);
    *next += 1;
}

/// Number the next log line gets.
pub fn log_next() -> u64 {
    LOG.lock().unwrap_or_else(|e| e.into_inner()).1
}

/// Kept log lines from number `from` on, joined by '\n'.
pub fn log_from(from: u64) -> String {
    let (log, next) = &*LOG.lock().unwrap_or_else(|e| e.into_inner());
    let first = next - log.len() as u64;
    let skip = from.saturating_sub(first) as usize;
    log.iter().skip(skip).cloned().collect::<Vec<_>>().join("\n")
}

/// Every mod as "id\tkind\tproblem" lines (problem empty while it runs; tabs and line breaks in it become spaces).
pub fn status_text() -> String {
    MODS.lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .map(|m| format!("{}\t{}\t{}\n", m.id, m.kind, m.problem.as_deref().unwrap_or("").replace(['\t', '\n', '\r'], " ")))
        .collect()
}

/// Record a mod's state; `problem` None means it is running.
pub fn set_status(id: &str, kind: &'static str, problem: Option<String>) {
    let mut mods = MODS.lock().unwrap_or_else(|e| e.into_inner());
    match mods.iter_mut().find(|m| m.id == id && m.kind == kind) {
        Some(m) => m.problem = problem,
        None => mods.push(Status { id: id.into(), kind, problem }),
    }
}

pub fn menu_item() {
    let show = SHOW.load(Ordering::Relaxed);
    if imgui::menu_item("Oxidizer", show) {
        SHOW.store(!show, Ordering::Relaxed);
    }
}

pub fn draw() {
    if !SHOW.load(Ordering::Relaxed) {
        return;
    }
    // Copies, so nothing that logs while drawing can deadlock on these locks.
    let mods = MODS.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let log: Vec<String> = LOG.lock().unwrap_or_else(|e| e.into_inner()).0.iter().cloned().collect();

    let mut open = true;
    imgui::set_next_window_size(560.0, 420.0);
    if imgui::begin_closable("Oxidizer", &mut open) {
        imgui::text_colored(GRAY, &format!("Oxidizer {}", env!("CARGO_PKG_VERSION")));
        imgui::separator();
        if mods.is_empty() {
            imgui::text("No Oxidizer mods found.");
        }
        for m in &mods {
            imgui::text(&format!("{} ({})", m.id, m.kind));
            imgui::same_line();
            match &m.problem {
                None => imgui::text_colored(GREEN, "running"),
                Some(p) => imgui::text_colored(RED, p),
            }
        }
        imgui::separator();
        draw_bus();
        imgui::text("Log");
        imgui::begin_child("oxidizer_log", 0.0);
        for line in &log {
            imgui::text(line);
        }
        imgui::stick_to_bottom();
        imgui::end_child();
    }
    imgui::end();
    if !open {
        SHOW.store(false, Ordering::Relaxed);
    }
}

/// What mods say to each other: topics with their subscribers and retained values, recent events, services.
fn draw_bus() {
    if !imgui::collapsing_header("Event bus") {
        return;
    }
    let b = crate::bus::snapshot();
    let short = |s: &str| if s.chars().count() > 80 { format!("{}...", s.chars().take(80).collect::<String>()) } else { s.to_string() };
    if b.topics.is_empty() && b.services.is_empty() {
        imgui::text_colored(GRAY, "No events or services yet.");
    }
    for (topic, subs, retained) in &b.topics {
        imgui::text(&format!("{topic}  ({subs} subscriber{})", if *subs == 1 { "" } else { "s" }));
        if let Some(v) = retained {
            imgui::same_line();
            imgui::text_colored(GRAY, &format!("now {}", short(v)));
        }
    }
    if !b.services.is_empty() {
        imgui::text("Services:");
        for name in &b.services {
            imgui::same_line();
            imgui::text_colored(GREEN, name);
        }
    }
    if !b.recent.is_empty() {
        imgui::text("Recent events:");
        for (topic, value) in b.recent.iter().take(8) {
            imgui::text_colored(GRAY, &format!("  {topic} {}", short(value)));
        }
    }
    imgui::separator();
}
