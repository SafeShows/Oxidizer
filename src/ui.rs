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
static LOG: Mutex<VecDeque<String>> = Mutex::new(VecDeque::new());
static MODS: Mutex<Vec<Status>> = Mutex::new(Vec::new());

#[derive(Clone)]
struct Status {
    id: String,
    kind: &'static str,
    /// None = running.
    problem: Option<String>,
}

pub fn push_log(line: String) {
    let mut log = LOG.lock().unwrap_or_else(|e| e.into_inner());
    if log.len() == LOG_LINES {
        log.pop_front();
    }
    log.push_back(line);
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
    let log: Vec<String> = LOG.lock().unwrap_or_else(|e| e.into_inner()).iter().cloned().collect();

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
