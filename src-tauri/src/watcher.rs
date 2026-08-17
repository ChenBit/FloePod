//! 暂存文件夹监听：用户在资源管理器手动增删文件时对账数据库。

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use notify::{RecursiveMode, Watcher};
use tauri::{AppHandle, Emitter, Manager};

use crate::db::{self, StagedItem};
use crate::events;
use crate::state::AppState;

pub fn restart(app: &AppHandle, path: String) {
    let state = app.state::<AppState>();
    let mut guard = state.watcher.lock().unwrap();
    *guard = None; // 丢弃旧监听（Drop 即解绑）

    let dir = PathBuf::from(&path);
    if !dir.is_dir() {
        return;
    }
    let (tx, rx) = mpsc::channel();
    let mut watcher = match notify::recommended_watcher(move |res| {
        let _ = tx.send(res);
    }) {
        Ok(w) => w,
        Err(_) => return,
    };
    if watcher.watch(&dir, RecursiveMode::NonRecursive).is_err() {
        return;
    }
    *guard = Some(watcher);

    let app = app.clone();
    std::thread::spawn(move || loop {
        // 等到第一个事件，再吸收 500ms 内的后续事件，然后对账
        if rx.recv().is_err() {
            return; // watcher 被替换 / 释放
        }
        while rx.recv_timeout(Duration::from_millis(500)).is_ok() {}
        let state = app.state::<AppState>();
        if state.staged_recently() {
            continue;
        }
        drop(state);
        if let Err(e) = reconcile(&app) {
            eprintln!("[watcher] 对账失败: {e}");
        }
    });
}

fn reconcile(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let conn = state.db.lock().unwrap();
    let settings = crate::settings::load(
        &conn,
        &state.data_dir.to_string_lossy(),
        env!("CARGO_PKG_VERSION"),
    )?;
    let Some(folder) = settings.staging_folder.clone() else {
        return Ok(());
    };
    let folder = PathBuf::from(&folder);
    let mut known: HashSet<String> = db::all_staging_paths(&conn)?
        .into_iter()
        .collect();

    let mut changed = false;

    // 磁盘上有、库里没有 -> 入库（归入当前场景）
    if let Ok(entries) = std::fs::read_dir(&folder) {
        for entry in entries.flatten() {
            let path = entry.path();
            let ps = path.to_string_lossy().to_string();
            if known.remove(&ps) {
                continue;
            }
            let meta = entry.metadata().ok();
            let name = entry.file_name().to_string_lossy().to_string();
            let ext = crate::commands::ext_of(&name);
            let kind = if path.is_dir() {
                "folder"
            } else if ext.as_deref() == Some("lnk") {
                "shortcut"
            } else {
                "file"
            };
            let _ = db::insert_item(
                &conn,
                &StagedItem {
                    id: 0,
                    scene_id: settings.active_scene_id,
                    kind: kind.into(),
                    staging_path: ps,
                    original_path: None,
                    name,
                    ext,
                    size: meta.map(|m| m.len() as i64).unwrap_or(0),
                    created_at: db::now_ms(),
                },
            );
            changed = true;
        }
    }

    // 库里有、磁盘上没有 -> 移除记录
    if !known.is_empty() {
        db::delete_items_by_paths(&conn, &known.into_iter().collect::<Vec<_>>())?;
        changed = true;
    }

    drop(conn);
    if changed {
        let _ = app.emit(events::ITEMS_CHANGED, ());
    }
    Ok(())
}
