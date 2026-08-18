//! Tauri 命令层：匣 / 暂存 / 导出 / 设置 / 窗口编排。

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, PhysicalSize};

use crate::db::{self, StagedItem};
use crate::events;
use crate::lnk;
use crate::manager;
use crate::settings::{self, Pod, Settings};
use crate::state::{AppState, PanelMode};

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn data_dir_str(state: &AppState) -> String {
    state.data_dir.to_string_lossy().to_string()
}

fn load_settings(state: &AppState) -> Result<Settings, String> {
    let conn = state.db.lock().unwrap();
    settings::load(&conn, &data_dir_str(state), VERSION)
}

fn load_settings_conn(conn: &rusqlite::Connection, state: &AppState) -> Result<Settings, String> {
    settings::load(conn, &data_dir_str(state), VERSION)
}

/* ---------- 工具 ---------- */

pub fn ext_of(name: &str) -> Option<String> {
    let idx = name.rfind('.')?;
    if idx == 0 {
        return None; // ".gitignore" 之类视为无扩展名
    }
    Some(name[idx + 1..].to_ascii_lowercase())
}

/// 目标目录内唯一文件名：`a.pdf` -> `a (2).pdf`
pub fn unique_target(dir: &Path, desired: &str, used: &mut HashSet<String>) -> PathBuf {
    let mut name = desired.to_string();
    let mut n = 1;
    loop {
        let candidate = dir.join(&name);
        let key = candidate.to_string_lossy().to_string();
        if !candidate.exists() && !used.contains(&key) {
            used.insert(key);
            return candidate;
        }
        n += 1;
        let (stem, ext) = match desired.rfind('.') {
            Some(i) if i > 0 => (&desired[..i], &desired[i..]),
            _ => (desired, ""),
        };
        name = format!("{stem} ({n}){ext}");
    }
}

fn copy_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    if src.is_dir() {
        fs::create_dir_all(dst)?;
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            copy_all(&entry.path(), &dst.join(entry.file_name()))?;
        }
        Ok(())
    } else {
        fs::copy(src, dst).map(|_| ())
    }
}

fn sanitize_text_name(raw: &str) -> String {
    let bad = ['<', '>', ':', '"', '/', '\\', '|', '?', '*'];
    let cleaned: String = raw
        .chars()
        .take(18)
        .map(|c| if bad.contains(&c) || c.is_control() { ' ' } else { c })
        .collect();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        "文字".to_string()
    } else {
        trimmed.to_string()
    }
}

fn pod_of_conn(conn: &rusqlite::Connection, state: &AppState, id: u64) -> Result<Pod, String> {
    let settings = load_settings_conn(conn, state)?;
    settings
        .pods
        .into_iter()
        .find(|p| p.id == id)
        .ok_or_else(|| "匣不存在".to_string())
}

fn staging_dir(pod: &Pod) -> Result<PathBuf, String> {
    let dir = PathBuf::from(&pod.staging_folder);
    fs::create_dir_all(&dir).map_err(|e| format!("暂存文件夹不可用: {e}"))?;
    Ok(dir)
}

/* ---------- 启动信息 ---------- */

/// 调试日志：追加写入数据目录 debug.log（release 无控制台，用文件排查）。
pub fn debug_log(msg: &str) {
    use std::io::Write;
    let dir = crate::paths::resolve();
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("debug.log"))
    {
        let _ = writeln!(f, "{}", msg);
    }
    eprintln!("{msg}");
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bootstrap {
    settings: Settings,
    monitors: Vec<serde_json::Value>,
    version: String,
}

#[tauri::command]
pub fn get_bootstrap(app: AppHandle) -> Result<Bootstrap, String> {
    let state = app.state::<AppState>();
    let settings = {
        let conn = state.db.lock().unwrap();
        load_settings_conn(&conn, &state)?
    };
    let monitors = manager::list_monitors(&app);
    Ok(Bootstrap {
        settings,
        monitors,
        version: VERSION.to_string(),
    })
}

#[tauri::command]
pub fn get_pod(app: AppHandle, pod_id: u64) -> Result<Option<Pod>, String> {
    Ok(load_settings(&app.state::<AppState>())?
        .pods
        .into_iter()
        .find(|p| p.id == pod_id))
}

#[tauri::command]
pub fn get_monitors(app: AppHandle) -> Vec<serde_json::Value> {
    manager::list_monitors(&app)
}

#[tauri::command]
pub fn get_modifier_state() -> crate::win::ModifierState {
    crate::win::modifier_state()
}

#[tauri::command]
pub fn get_hotkey_defaults() -> settings::Hotkeys {
    settings::Hotkeys::with_defaults()
}

/* ---------- 匣 CRUD ---------- */

/// 必须为 async 命令：apply_settings 会经 sync_pods 创建 WebView 窗口，
/// 同步命令在 Windows 上会与主线程消息循环互相等待而死锁（OOBE 创建匣卡死的根因）。
#[tauri::command]
pub async fn create_pod(app: AppHandle, config: serde_json::Value) -> Result<Pod, String> {
    let state = app.state::<AppState>();
    let pod = {
        let conn = state.db.lock().unwrap();
        let id = settings::next_pod_id(&conn, &data_dir_str(&state), VERSION)?;
        let mut pod = pod_from_config(&config)?;
        pod.id = id;
        settings::upsert_pod(&conn, &pod, &data_dir_str(&state), VERSION)?;
        pod
    };
    drop(state);
    manager::apply_settings(&app, &manager::current_settings(&app));
    // 定位了新文件夹：触发对账，把文件夹中已有的文件读入列表
    app.state::<AppState>()
        .watcher_dirty
        .store(true, std::sync::atomic::Ordering::Relaxed);
    let _ = app.emit(events::PODS_CHANGED, ());
    Ok(pod)
}

/// 从前端配置构造 Pod：宽容解析类型（数字 / 数字字符串均可），缺省用默认值。
fn pod_from_config(v: &serde_json::Value) -> Result<Pod, String> {
    let s = |k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or_default().to_string();
    let f = |k: &str, d: f64| {
        v.get(k)
            .and_then(|x| x.as_f64().or_else(|| x.as_str().and_then(|s| s.trim().parse().ok())))
            .unwrap_or(d)
    };
    let u = |k: &str, d: u32| {
        v.get(k)
            .and_then(|x| x.as_u64().or_else(|| Some(f(k, d as f64) as u64)))
            .unwrap_or(d as u64) as u32
    };
    let u64v = |k: &str, d: u64| {
        v.get(k)
            .and_then(|x| x.as_u64().or_else(|| Some(f(k, d as f64) as u64)))
            .unwrap_or(d)
    };
    let b = |k: &str, d: bool| v.get(k).and_then(|x| x.as_bool()).unwrap_or(d);
    Ok(Pod {
        id: 0,
        name: s("name"),
        edge: s("edge"),
        monitor: s("monitor"),
        offset: f("offset", 0.5),
        staging_folder: s("stagingFolder"),
        opacity: f("opacity", 0.85),
        material: s("material"),
        panel_width: u("panelWidth", 380),
        hover_delay_ms: u64v("hoverDelayMs", 120),
        drop_action: s("dropAction"),
        enabled: b("enabled", true),
    })
}

/// async 命令：与 create_pod 同理，可能触发窗口创建，须避开主线程。
#[tauri::command]
pub async fn update_pod(app: AppHandle, pod_id: u64, patch: serde_json::Value) -> Result<Pod, String> {
    let state = app.state::<AppState>();
    let (pod, folder_changed) = {
        let conn = state.db.lock().unwrap();
        let mut pod = pod_of_conn(&conn, &state, pod_id)?;
        let old_folder = pod.staging_folder.clone();
        if let Some(obj) = patch.as_object() {
            for (k, v) in obj {
                // 按字段合并（string/number/bool 均可，数值宽容解析）
                let fv = |d: f64| {
                    v.as_f64()
                        .or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))
                        .unwrap_or(d)
                };
                let uv = |d: u32| {
                    v.as_u64()
                        .or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))
                        .unwrap_or(d as u64) as u32
                };
                match k.as_str() {
                    "name" => {
                        if let Some(s) = v.as_str() {
                            pod.name = s.to_string();
                        }
                    }
                    "edge" => {
                        if let Some(s) = v.as_str() {
                            pod.edge = s.to_string();
                        }
                    }
                    "monitor" => {
                        if let Some(s) = v.as_str() {
                            pod.monitor = s.to_string();
                        }
                    }
                    "offset" => {
                        pod.offset = fv(pod.offset).clamp(0.0, 1.0);
                    }
                    "stagingFolder" => {
                        if let Some(s) = v.as_str() {
                            pod.staging_folder = s.to_string();
                        }
                    }
                    "opacity" => {
                        pod.opacity = fv(pod.opacity).clamp(0.4, 1.0);
                    }
                    "material" => {
                        if let Some(s) = v.as_str() {
                            pod.material = s.to_string();
                        }
                    }
                    "panelWidth" => {
                        pod.panel_width = uv(pod.panel_width).clamp(300, 520);
                    }
                    "hoverDelayMs" => {
                        let d = pod.hover_delay_ms as f64;
                        pod.hover_delay_ms = fv(d).clamp(0.0, 600.0) as u64;
                    }
                    "dropAction" => {
                        if let Some(s) = v.as_str() {
                            pod.drop_action = s.to_string();
                        }
                    }
                    "enabled" => {
                        if let Some(b) = v.as_bool() {
                            pod.enabled = b;
                        }
                    }
                    _ => {}
                }
            }
        }
        settings::upsert_pod(&conn, &pod, &data_dir_str(&state), VERSION)?;
        let folder_changed = old_folder != pod.staging_folder;
        (pod, folder_changed)
    };
    drop(state);
    manager::apply_settings(&app, &manager::current_settings(&app));
    // 重新定位了暂存文件夹：触发对账，读取新文件夹中已有的文件
    if folder_changed {
        app.state::<AppState>()
            .watcher_dirty
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }
    let _ = app.emit(events::PODS_CHANGED, ());
    Ok(pod)
}

/// async 命令：与 create_pod 同理，可能触发窗口创建，须避开主线程。
#[tauri::command]
pub async fn delete_pod(app: AppHandle, pod_id: u64, recycle_files: bool) -> Result<(), String> {
    let state = app.state::<AppState>();
    let removed: Vec<StagedItem> = {
        let conn = state.db.lock().unwrap();
        let removed = if recycle_files {
            db::delete_items_by_pod(&conn, pod_id as i64)?
        } else {
            Vec::new()
        };
        settings::delete_pod(&conn, pod_id, &data_dir_str(&state), VERSION)?;
        removed
    };
    if recycle_files {
        for it in &removed {
            let p = Path::new(&it.staging_path);
            if p.exists() {
                let _ = trash::delete(p);
            }
        }
    }
    drop(state);
    manager::apply_settings(&app, &manager::current_settings(&app));
    let _ = app.emit(events::PODS_CHANGED, ());
    let _ = app.emit(events::ITEMS_CHANGED, ());
    Ok(())
}

/* ---------- 设置 ---------- */

/// async 命令：apply_settings 内可能创建窗口，同步执行会死锁主线程。
#[tauri::command]
pub async fn save_settings(app: AppHandle, patch: serde_json::Value) -> Result<Settings, String> {
    let state = app.state::<AppState>();
    let (prev, next) = {
        let conn = state.db.lock().unwrap();
        let prev = load_settings_conn(&conn, &state)?;
        let next = settings::merge_persist(&conn, patch, &data_dir_str(&state), VERSION)?;
        (prev, next)
    };
    // 快捷键变更需可注册；失败则回滚热键字段并报错
    if next.hotkeys.toggle_bar != prev.hotkeys.toggle_bar
        || next.hotkeys.collect_clipboard != prev.hotkeys.collect_clipboard
        || next.hotkeys.open_panel != prev.hotkeys.open_panel
    {
        if let Err(e) = crate::hotkeys::register(&app, &next) {
            let rolled = {
                let conn = state.db.lock().unwrap();
                let mut fixed = next.clone();
                fixed.hotkeys = prev.hotkeys.clone();
                settings::persist(&conn, &fixed)?;
                fixed
            };
            let _ = app.emit(events::SETTINGS_CHANGED, rolled);
            return Err(e);
        }
    }
    drop(state);
    manager::apply_settings(&app, &next);
    Ok(next)
}

/* ---------- 暂存 ---------- */

#[tauri::command]
pub fn stage_paths(
    app: AppHandle,
    pod_id: u64,
    paths: Vec<String>,
    action: String,
) -> Result<Vec<StagedItem>, String> {
    let state = app.state::<AppState>();
    let conn = state.db.lock().unwrap();
    let pod = pod_of_conn(&conn, &state, pod_id)?;
    let dir = staging_dir(&pod)?;

    let sources: Vec<PathBuf> = paths
        .iter()
        .map(PathBuf::from)
        .filter(|p| p.exists())
        .collect();
    if sources.is_empty() {
        return Err("没有可暂存的文件".into());
    }

    let mut used: HashSet<String> = HashSet::new();
    let mut created: Vec<StagedItem> = Vec::new();

    match action.as_str() {
        "shortcut" => {
            let mut pairs = Vec::new();
            for src in &sources {
                let name = src
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "目标".into());
                let target = unique_target(&dir, &lnk::shortcut_name_for(&name), &mut used);
                pairs.push((src.clone(), target));
            }
            lnk::create_shortcuts(&pairs)?;
            for (src, target) in pairs {
                let name = target
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                created.push(db::insert_item(
                    &conn,
                    &StagedItem {
                        id: 0,
                        pod_id: pod.id as i64,
                        kind: "shortcut".into(),
                        staging_path: target.to_string_lossy().to_string(),
                        original_path: Some(src.to_string_lossy().to_string()),
                        name,
                        ext: Some("lnk".into()),
                        size: 0,
                        created_at: db::now_ms(),
                    },
                )?);
            }
        }
        act @ ("copy" | "move") => {
            for src in &sources {
                let name = src
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "未命名".into());
                let target = unique_target(&dir, &name, &mut used);
                if act == "move" {
                    let moved = fs::rename(src, &target).or_else(|_| {
                        copy_all(src, &target)?;
                        if src.is_dir() {
                            fs::remove_dir_all(src)
                        } else {
                            fs::remove_file(src)
                        }
                    });
                    moved.map_err(|e| format!("移动 {} 失败: {e}", name))?;
                } else {
                    copy_all(src, &target).map_err(|e| format!("复制 {} 失败: {e}", name))?;
                }
                let size = if src.is_dir() { 0 } else { fs::metadata(&target).map(|m| m.len() as i64).unwrap_or(0) };
                let kind = if src.is_dir() { "folder" } else { "file" };
                created.push(db::insert_item(
                    &conn,
                    &StagedItem {
                        id: 0,
                        pod_id: pod.id as i64,
                        kind: kind.into(),
                        staging_path: target.to_string_lossy().to_string(),
                        original_path: Some(src.to_string_lossy().to_string()),
                        name: target
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_default(),
                        ext: ext_of(&name),
                        size,
                        created_at: db::now_ms(),
                    },
                )?);
            }
        }
        other => return Err(format!("未知动作: {other}")),
    }

    drop(conn);
    state.mark_staged();
    emit_items_changed(&app, pod.id);
    Ok(created)
}

#[tauri::command]
pub fn stage_text(app: AppHandle, pod_id: u64, content: String) -> Result<StagedItem, String> {
    if content.trim().is_empty() {
        return Err("内容为空".into());
    }
    let state = app.state::<AppState>();
    let conn = state.db.lock().unwrap();
    let pod = pod_of_conn(&conn, &state, pod_id)?;
    let dir = staging_dir(&pod)?;

    let base = sanitize_text_name(content.lines().next().unwrap_or("文字"));
    let mut used = HashSet::new();
    let target = unique_target(&dir, &format!("{base}.txt"), &mut used);
    let size = content.len() as i64;
    fs::write(&target, content).map_err(|e| format!("写入失败: {e}"))?;

    let item = db::insert_item(
        &conn,
        &StagedItem {
            id: 0,
            pod_id: pod.id as i64,
            kind: "text".into(),
            staging_path: target.to_string_lossy().to_string(),
            original_path: None,
            name: target
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default(),
            ext: Some("txt".into()),
            size,
            created_at: db::now_ms(),
        },
    )?;
    drop(conn);
    state.mark_staged();
    emit_items_changed(&app, pod.id);
    Ok(item)
}

#[tauri::command]
pub fn list_pod_items(app: AppHandle, pod_id: u64) -> Result<Vec<StagedItem>, String> {
    let state = app.state::<AppState>();
    let conn = state.db.lock().unwrap();
    db::items_of_pod(&conn, pod_id as i64)
}

#[tauri::command]
pub fn remove_items(app: AppHandle, ids: Vec<i64>, delete_files: bool) -> Result<(), String> {
    let state = app.state::<AppState>();
    let pod_ids: Vec<i64> = {
        let conn = state.db.lock().unwrap();
        let items = db::items_by_ids(&conn, &ids)?;
        let pod_ids = items.iter().map(|i| i.pod_id).collect::<HashSet<_>>();
        if delete_files {
            for it in &items {
                let p = Path::new(&it.staging_path);
                if p.exists() {
                    let _ = trash::delete(p);
                }
            }
        }
        db::delete_items_by_ids(&conn, &ids)?;
        pod_ids.into_iter().collect()
    };
    drop(state);
    for pid in pod_ids {
        emit_items_changed(&app, pid as u64);
    }
    Ok(())
}

/// 剪切拖出后的源清理：目标已接收（OLE 移动契约），删除暂存源（进回收站，可反悔）。
#[tauri::command]
pub fn finalize_drag_cut(app: AppHandle, paths: Vec<String>) -> Result<(), String> {
    let state = app.state::<AppState>();
    let pod_ids: Vec<i64> = {
        let conn = state.db.lock().unwrap();
        let affected: HashSet<i64> = paths
            .iter()
            .filter_map(|p| db::find_by_path(&conn, p).ok().flatten())
            .map(|i| i.pod_id)
            .collect();
        for p in &paths {
            let path = Path::new(p);
            if path.exists() {
                let _ = trash::delete(path);
            }
        }
        db::delete_items_by_paths(&conn, &paths)?;
        affected.into_iter().collect()
    };
    state.mark_staged();
    for pid in pod_ids {
        emit_items_changed(&app, pid as u64);
    }
    Ok(())
}

/* ---------- 导出 ---------- */

#[tauri::command]
pub fn export_items(
    app: AppHandle,
    ids: Vec<i64>,
    dest_dir: String,
    mode: String,
    on_conflict: String,
) -> Result<Vec<String>, String> {
    let state = app.state::<AppState>();
    let conn = state.db.lock().unwrap();
    let items = db::items_by_ids(&conn, &ids)?;
    if items.is_empty() {
        return Ok(vec![]);
    }
    let dest = PathBuf::from(&dest_dir);
    fs::create_dir_all(&dest).map_err(|e| format!("目标文件夹不可用: {e}"))?;

    let conflicts: Vec<String> = items
        .iter()
        .filter(|it| dest.join(&it.name).exists())
        .map(|it| it.name.clone())
        .collect();
    if on_conflict == "ask" && !conflicts.is_empty() {
        return Ok(conflicts);
    }

    let mut used: HashSet<String> = HashSet::new();
    for it in &items {
        let src = Path::new(&it.staging_path);
        if !src.exists() {
            continue;
        }
        let target = match on_conflict.as_str() {
            "overwrite" => dest.join(&it.name),
            "skip" => {
                if dest.join(&it.name).exists() {
                    continue;
                }
                dest.join(&it.name)
            }
            _ => unique_target(&dest, &it.name, &mut used),
        };
        copy_all(src, &target).map_err(|e| format!("导出 {} 失败: {e}", it.name))?;
        if mode == "move" {
            let _ = trash::delete(src);
        }
    }

    let pod_ids: Vec<i64> = items.iter().map(|i| i.pod_id).collect::<HashSet<_>>().into_iter().collect();
    if mode == "move" {
        db::delete_items_by_ids(&conn, &ids)?;
    }
    drop(conn);
    state.mark_staged();
    for pid in pod_ids {
        emit_items_changed(&app, pid as u64);
    }
    Ok(vec![])
}

/* ---------- 缩略图 ---------- */

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThumbnailPayload {
    mime: String,
    bytes: Vec<u8>,
}

const THUMB_MAX_SIDE: u32 = 256;
const THUMB_MAX_SRC: u64 = 64 * 1024 * 1024;

#[tauri::command]
pub fn read_thumbnail(app: AppHandle, path: String) -> Result<Option<ThumbnailPayload>, String> {
    let state = app.state::<AppState>();
    let folders: Vec<PathBuf> = {
        let conn = state.db.lock().unwrap();
        load_settings_conn(&conn, &state)?
            .pods
            .into_iter()
            .map(|p| PathBuf::from(p.staging_folder))
            .collect()
    };
    if folders.is_empty() {
        return Ok(None);
    }
    let target = PathBuf::from(&path).canonicalize().map_err(|e| e.to_string())?;
    let allowed = folders.iter().any(|f| {
        f.canonicalize()
            .map(|b| target.starts_with(&b))
            .unwrap_or(false)
    });
    if !allowed {
        return Ok(None);
    }
    let ext = ext_of(
        &target
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default(),
    );
    let Some(ext) = ext else {
        return Ok(None);
    };
    if !matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "ico") {
        return Ok(None);
    }
    let meta = fs::metadata(&target).map_err(|e| e.to_string())?;
    if meta.len() > THUMB_MAX_SRC {
        return Ok(None);
    }
    let bytes = fs::read(&target).map_err(|e| e.to_string())?;

    let img = image::load_from_memory(&bytes).map_err(|e| e.to_string())?;
    let thumb = img.thumbnail(THUMB_MAX_SIDE, THUMB_MAX_SIDE);
    let mut png = Vec::new();
    image::DynamicImage::write_to(&thumb, std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(Some(ThumbnailPayload {
        mime: "image/png".into(),
        bytes: png,
    }))
}

/* ---------- 窗口编排（按匣） ---------- */

fn emit_items_changed(app: &AppHandle, pod_id: u64) {
    let payload = serde_json::json!({ "podId": pod_id });
    if let Some(panel) = manager::pod_panel(app, pod_id) {
        let _ = panel.emit(events::ITEMS_CHANGED, payload.clone());
    }
    if let Some(bar) = manager::pod_bar(app, pod_id) {
        let _ = bar.emit(events::ITEMS_CHANGED, payload);
    }
}

#[tauri::command]
pub fn show_panel(app: AppHandle, pod_id: u64) {
    manager::show_panel(&app, pod_id);
}

#[tauri::command]
pub fn toggle_panel(app: AppHandle, pod_id: u64) {
    manager::toggle_panel(&app, pod_id);
}

#[tauri::command]
pub fn hide_panel(app: AppHandle, pod_id: u64) {
    manager::hide_panel(&app, pod_id);
}

#[tauri::command]
pub fn set_panel_mode(app: AppHandle, pod_id: u64, mode: String) {
    {
        let state = app.state::<AppState>();
        let mut guard = state.pods.lock().unwrap();
        if let Some(r) = guard.get_mut(&pod_id) {
            r.mode = PanelMode::parse(&mode);
            if r.mode == PanelMode::List {
                r.pending_drop.clear();
            }
        }
    }
    manager::emit_panel_mode(&app, pod_id);
}

#[tauri::command]
pub fn hold_pending_drop(app: AppHandle, pod_id: u64, paths: Vec<String>) {
    {
        let state = app.state::<AppState>();
        let mut guard = state.pods.lock().unwrap();
        if let Some(r) = guard.get_mut(&pod_id) {
            r.pending_drop = paths;
            r.mode = PanelMode::Ask;
        }
    }
    manager::show_panel(&app, pod_id);
    manager::emit_panel_mode(&app, pod_id);
}

#[tauri::command]
pub fn report_presence(app: AppHandle, pod_id: u64, window: String, inside: bool) {
    manager::report_presence(&app, pod_id, &window, inside);
}

#[tauri::command]
pub fn set_panel_pinned(app: AppHandle, pod_id: u64, pinned: bool) {
    manager::set_panel_pinned(&app, pod_id, pinned);
}

#[tauri::command]
pub fn set_dragging_out(app: AppHandle, pod_id: u64, dragging: bool) {
    manager::set_dragging_out(&app, pod_id, dragging);
}

#[tauri::command]
pub fn set_pod_accept(app: AppHandle, pod_id: u64, accepting: bool) {
    manager::set_pod_accept(&app, pod_id, accepting);
}

#[tauri::command]
pub fn set_panel_size(app: AppHandle, pod_id: u64, _width: u32, height: u32) {
    let state = app.state::<AppState>();
    let settings = load_settings(&state).unwrap_or_default();
    let pod = settings.pods.iter().find(|p| p.id == pod_id);
    if let Some(pod) = pod {
        if let Some(panel) = manager::pod_panel(&app, pod_id) {
            let scale = panel.scale_factor().unwrap_or(1.0);
            let w = (pod.panel_width as f64 * scale).round() as u32;
            let h = ((height as f64 * scale).round() as u32).clamp(160, 900);
            let resize_now = {
                let mut guard = state.pods.lock().unwrap();
                if let Some(r) = guard.get_mut(&pod_id) {
                    let changed = r.panel_height != h;
                    r.panel_height = h;
                    // 面板可见且高度变化才调整窗口；隐藏期间只记录高度，
                    // 下次显示时按新尺寸摆放，避免「显示后跳一下」的闪烁。
                    changed && r.panel_visible
                } else {
                    false
                }
            };
            if resize_now {
                let _ = panel.set_size(PhysicalSize::new(w, h));
                manager::place_panel_dyn(&app, pod_id);
            }
        }
    }
}

#[tauri::command]
pub fn toggle_all_bars(app: AppHandle) {
    let visible = {
        let pod = manager::current_settings(&app)
            .pods
            .into_iter()
            .find(|p| p.enabled);
        match pod {
            Some(p) => manager::pod_bar(&app, p.id)
                .map(|b| b.is_visible().unwrap_or(false))
                .unwrap_or(false),
            None => false,
        }
    };
    manager::set_all_bars(&app, !visible);
}

#[tauri::command]
pub fn open_settings(app: AppHandle) {
    manager::open_settings(&app);
}

/// 前端错误上报（写入数据目录 debug.log，用于排查）。
#[tauri::command]
pub fn log_frontend(msg: String) {
    debug_log(&format!("[frontend] {msg}"));
}

/// 前端生命周期日志（写入数据目录 debug.log，用于排查创建流程）。
#[tauri::command]
pub fn app_log(msg: String) {
    debug_log(&format!("[ui] {msg}"));
}

#[tauri::command]
pub fn quit_app(app: AppHandle) {
    app.exit(0);
}

/* ---------- 测试 ---------- */

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unique_target_appends_number() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        fs::write(dir.join("a.pdf"), b"x").unwrap();
        let mut used = HashSet::new();
        let t1 = unique_target(dir, "a.pdf", &mut used);
        assert_eq!(t1.file_name().unwrap().to_string_lossy(), "a (2).pdf");
        let t2 = unique_target(dir, "a.pdf", &mut used);
        assert_eq!(t2.file_name().unwrap().to_string_lossy(), "a (3).pdf");
        let t3 = unique_target(dir, "b.txt", &mut used);
        assert_eq!(t3.file_name().unwrap().to_string_lossy(), "b.txt");
    }

    #[test]
    fn ext_of_handles_edge_cases() {
        assert_eq!(ext_of("a.PDF").as_deref(), Some("pdf"));
        assert_eq!(ext_of(".gitignore"), None);
        assert_eq!(ext_of("noext"), None);
        assert_eq!(ext_of("arch.tar.gz").as_deref(), Some("gz"));
    }

    #[test]
    fn sanitize_keeps_readable_head() {
        assert_eq!(sanitize_text_name("héllo world"), "héllo world");
        assert_eq!(sanitize_text_name("a<b>c:d"), "a b c d");
        assert_eq!(sanitize_text_name("   "), "文字");
        let long: String = std::iter::repeat('字').take(50).collect();
        assert_eq!(sanitize_text_name(&long).chars().count(), 18);
    }

    #[test]
    fn pod_from_config_accepts_numeric_strings() {
        // 前端 range 输入可能传出字符串数字，应宽容解析
        let v = serde_json::json!({
            "name": "我的匣",
            "edge": "left",
            "stagingFolder": "D:\\暂存",
            "opacity": "0.85",
            "panelWidth": "380",
        });
        let pod = pod_from_config(&v).unwrap();
        assert_eq!(pod.name, "我的匣");
        assert_eq!(pod.opacity, 0.85);
        assert_eq!(pod.panel_width, 380);
        assert!(pod.enabled);
    }
}
