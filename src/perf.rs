//! What each mod costs per frame, for **Mods > Oxidizer > Performance**: the time spent in its callbacks, hooks,
//! events and services. Time is exclusive: when a mod calls a game method that another mod hooks, the hook's time
//! counts for the other mod only.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Default)]
struct Stat {
    /// Time in the current frame.
    now: Duration,
    /// Smoothed milliseconds per frame.
    avg: f64,
    /// The highest frame in the last second, and in the second before (what is shown).
    peak: f64,
    shown_peak: f64,
}

struct Perf {
    mods: BTreeMap<String, Stat>,
    frame_avg: f64,
    second: Option<Instant>,
}

static PERF: Mutex<Perf> = Mutex::new(Perf { mods: BTreeMap::new(), frame_avg: 0.0, second: None });

thread_local! {
    /// The mods being timed on this thread, innermost last, with when their current stretch started.
    static STACK: RefCell<Vec<(String, Instant)>> = const { RefCell::new(Vec::new()) };
}

pub(crate) fn add(name: &str, d: Duration) {
    let mut p = PERF.lock().unwrap_or_else(|e| e.into_inner());
    match p.mods.get_mut(name) {
        Some(s) => s.now += d,
        None => {
            p.mods.insert(name.into(), Stat { now: d, ..Stat::default() });
        }
    }
}

/// Runs `f` as time spent by mod `name`.
pub fn time<R>(name: &str, f: impl FnOnce() -> R) -> R {
    let now = Instant::now();
    STACK.with_borrow_mut(|s| {
        if let Some((outer, since)) = s.last_mut() {
            add(outer, now - *since); // the outer mod's stretch ends here
        }
        s.push((name.to_string(), now));
    });
    let r = f();
    let end = Instant::now();
    STACK.with_borrow_mut(|s| {
        if let Some((me, since)) = s.pop() {
            add(&me, end - since);
        }
        if let Some((_, since)) = s.last_mut() {
            *since = end; // the outer mod's next stretch starts
        }
    });
    r
}

/// Closes a frame (`dt`: its length in seconds): the frame's times go into the averages.
pub fn end_frame(dt: f64) {
    let mut p = PERF.lock().unwrap_or_else(|e| e.into_inner());
    let new_second = p.second.is_none_or(|t| t.elapsed() >= Duration::from_secs(1));
    if new_second {
        p.second = Some(Instant::now());
    }
    p.frame_avg = smooth(p.frame_avg, dt * 1000.0);
    for s in p.mods.values_mut() {
        let ms = s.now.as_secs_f64() * 1000.0;
        s.now = Duration::ZERO;
        s.avg = smooth(s.avg, ms);
        s.peak = s.peak.max(ms);
        if new_second {
            (s.shown_peak, s.peak) = (s.peak, 0.0);
        }
    }
}

/// An average over roughly the last 30 frames.
fn smooth(avg: f64, x: f64) -> f64 {
    if avg == 0.0 { x } else { avg + (x - avg) / 30.0 }
}

/// (frame milliseconds, [(mod, average ms per frame, highest ms in a frame over the last second)]), costliest first.
pub fn snapshot() -> (f64, Vec<(String, f64, f64)>) {
    let p = PERF.lock().unwrap_or_else(|e| e.into_inner());
    let mut rows: Vec<_> = p.mods.iter().map(|(n, s)| (n.clone(), s.avg, s.shown_peak.max(s.peak))).collect();
    rows.sort_by(|a, b| b.1.total_cmp(&a.1));
    (p.frame_avg, rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_time_counts_once() {
        time("perf_outer", || {
            std::thread::sleep(Duration::from_millis(20));
            time("perf_inner", || std::thread::sleep(Duration::from_millis(30)));
        });
        let p = PERF.lock().unwrap();
        let ms = |n: &str| p.mods[n].now.as_secs_f64() * 1000.0;
        assert!((20.0..29.0).contains(&ms("perf_outer")), "outer {}", ms("perf_outer"));
        assert!((30.0..39.0).contains(&ms("perf_inner")), "inner {}", ms("perf_inner"));
    }
}
