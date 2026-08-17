use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU32, Ordering};
use std::sync::Mutex;
use std::time::Instant;

use notify::RecommendedWatcher;
use rusqlite::Connection;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PanelMode {
    List,
    Ask,
    Conflict,
}

impl PanelMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            PanelMode::List => "list",
            PanelMode::Ask => "ask",
            PanelMode::Conflict => "conflict",
        }
    }
    pub fn parse(s: &str) -> Self {
        match s {
            "ask" => PanelMode::Ask,
            "conflict" => PanelMode::Conflict,
            _ => PanelMode::List,
        }
    }
}

#[derive(Default)]
pub struct Presence {
    pub bar_inside: bool,
    pub panel_inside: bool,
    pub last_change: Option<Instant>,
}

pub struct AppState {
    pub db: Mutex<Connection>,
    pub data_dir: PathBuf,
    pub presence: Mutex<Presence>,
    pub panel_visible: AtomicBool,
    pub panel_pinned: AtomicBool,
    pub panel_mode: Mutex<PanelMode>,
    pub pending_drop: Mutex<Vec<String>>,
    pub bar_hovering: AtomicBool,
    pub panel_height: AtomicU32,
    pub last_stage_ms: AtomicU64,
    pub watcher: Mutex<Option<RecommendedWatcher>>,
}

impl AppState {
    pub fn new(db: Connection, data_dir: PathBuf) -> Self {
        Self {
            db: Mutex::new(db),
            data_dir,
            presence: Mutex::new(Presence::default()),
            panel_visible: AtomicBool::new(false),
            panel_pinned: AtomicBool::new(false),
            panel_mode: Mutex::new(PanelMode::List),
            pending_drop: Mutex::new(Vec::new()),
            bar_hovering: AtomicBool::new(false),
            panel_height: AtomicU32::new(420),
            last_stage_ms: AtomicU64::new(0),
            watcher: Mutex::new(None),
        }
    }

    pub fn mark_staged(&self) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        self.last_stage_ms.store(now, Ordering::Relaxed);
    }

    pub fn staged_recently(&self) -> bool {
        let last = self.last_stage_ms.load(Ordering::Relaxed);
        if last == 0 {
            return false;
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        now.saturating_sub(last) < 3_000
    }

    pub fn set_mode(&self, mode: PanelMode) {
        *self.panel_mode.lock().unwrap() = mode;
        if mode == PanelMode::List {
            self.pending_drop.lock().unwrap().clear();
        }
    }
}
