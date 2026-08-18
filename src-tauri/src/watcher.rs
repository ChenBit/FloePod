//! 暂存文件夹监听：用户在资源管理器手动增删文件时对账数据库（每个匣独立监听）。

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::time::Duration;

use notify::{RecursiveMode, Watcher};
use tauri::{AppHandle, Emitter, Manager};

use crate::db::StagedItem;
use crate::events;
use crate::manager;
use crate::state::AppState;

/// 常驻对账线程：有脏标记且非应用自身写入后，整盘对账。
pub fn spawn(app: AppHandle) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(800));
        let state = app.state::<AppState>();
        if !state.watcher_dirty.swap(false, Ordering::Relaxed) {
            continue;
        }
        if state.staged_recently() {
            continue;
        }
        drop(state);
        if let Err(e) = reconcile_all(&app) {
            eprintln!("[watcher] 对账失败: {e}");
        }
    });
}

/// 按当前设置重建所有匣的监听。
pub fn restart_all(app: &AppHandle) {
    let settings = manager::current_settings(app);
    let folders: Vec<(u64, String)> = settings
        .pods
        .iter()
        .filter(|p| p.enabled && !p.staging_folder.is_empty())
        .map(|p| (p.id, p.staging_folder.clone()))
        .collect();

    let state = app.state::<AppState>();
    let mut guard = state.watcher.lock().unwrap();
    guard.clear();

    for (pod_id, path) in folders {
        let dir = PathBuf::from(&path);
        if !dir.is_dir() {
            continue;
        }
        let app2 = app.clone();
        if let Ok(mut w) = notify::recommended_watcher(move |_res| {
            let st = app2.state::<AppState>();
            st.watcher_dirty.store(true, Ordering::Relaxed);
        }) {
            if w.watch(&dir, RecursiveMode::NonRecursive).is_ok() {
                guard.insert(pod_id, w);
            }
        }
    }
}

fn reconcile_all(app: &AppHandle) -> Result<(), String> {
    let settings = manager::current_settings(app);
    let folders: Vec<(u64, String)> = settings
        .pods
        .iter()
        .filter(|p| p.enabled)
        .map(|p| (p.id, p.staging_folder.clone()))
        .collect();

    let state = app.state::<AppState>();
    let conn = state.db.lock().unwrap();
    let mut changed = false;

    for (pod_id, folder) in folders {
        if folder.is_empty() {
            continue;
        }
        let folder = PathBuf::from(&folder);
        if !folder.is_dir() {
            continue;
        }
        let mut known: HashSet<String> = crate::db::items_of_pod(&conn, pod_id as i64)?
            .into_iter()
            .map(|i| i.staging_path)
            .collect();

        // 磁盘上有、库里没有 -> 入库
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
                let _ = crate::db::insert_item(
                    &conn,
                    &StagedItem {
                        id: 0,
                        pod_id: pod_id as i64,
                        kind: kind.into(),
                        staging_path: ps,
                        original_path: None,
                        name,
                        ext,
                        size: meta.map(|m| m.len() as i64).unwrap_or(0),
                        created_at: crate::db::now_ms(),
                    },
                );
                changed = true;
            }
        }

        // 库里有、磁盘上没有 -> 移除记录
        if !known.is_empty() {
            crate::db::delete_items_by_paths(&conn, &known.into_iter().collect::<Vec<_>>())?;
            changed = true;
        }
    }

    drop(conn);
    if changed {
        for pod in settings.pods.iter().filter(|p| p.enabled) {
            let payload = serde_json::json!({ "podId": pod.id });
            if let Some(panel) = manager::pod_panel(app, pod.id) {
                let _ = panel.emit(events::ITEMS_CHANGED, payload.clone());
            }
            if let Some(bar) = manager::pod_bar(app, pod.id) {
                let _ = bar.emit(events::ITEMS_CHANGED, payload);
            }
        }
    }
    Ok(())
}
