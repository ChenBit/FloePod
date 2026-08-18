use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Instant;

use notify::RecommendedWatcher;
use rusqlite::Connection;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum PanelMode {
    #[default]
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

/// 单个「匣」的运行时状态（看门狗 / 面板显隐）。
#[derive(Default)]
pub struct PodRuntime {
    pub bar_inside: bool,
    pub panel_inside: bool,
    pub panel_visible: bool,
    pub panel_pinned: bool,
    /// 面板正在向外拖出文件（OLE 拖拽进行中）
    pub dragging_out: bool,
    pub mode: PanelMode,
    pub pending_drop: Vec<String>,
    pub panel_height: u32,
    pub last_change: Option<Instant>,
    /// 已应用的窗口材质（避免每次显示都重设亚克力引起闪烁）
    pub material: Option<String>,
}

pub struct AppState {
    pub db: Mutex<Connection>,
    pub data_dir: PathBuf,
    /// pod_id -> 运行时状态
    pub pods: Mutex<HashMap<u64, PodRuntime>>,
    pub last_stage_ms: AtomicU64,
    /// 暂存文件夹监听的脏标记（有文件变化待对账）
    pub watcher_dirty: AtomicBool,
    /// pod_id -> 暂存文件夹监听器
    pub watcher: Mutex<HashMap<u64, RecommendedWatcher>>,
}

impl AppState {
    pub fn new(db: Connection, data_dir: PathBuf) -> Self {
        Self {
            db: Mutex::new(db),
            data_dir,
            pods: Mutex::new(HashMap::new()),
            last_stage_ms: AtomicU64::new(0),
            watcher_dirty: AtomicBool::new(false),
            watcher: Mutex::new(HashMap::new()),
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
}
