//! Tauri 命令层：暂存 / 导出 / 场景 / 设置 / 窗口编排。

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, PhysicalSize};

use crate::db::{self, StagedItem};
use crate::events;
use crate::lnk;
use crate::manager;
use crate::settings::{self, Settings};
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

/* ---------- 启动信息 ---------- */

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bootstrap {
    settings: Settings,
    scenes: Vec<db::Scene>,
    items: Vec<StagedItem>,
    panel_mode: String,
    pending_drop: Option<PendingDrop>,
    version: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingDrop {
    paths: Vec<String>,
}

#[tauri::command]
pub fn get_bootstrap(app: AppHandle) -> Result<Bootstrap, String> {
    let state = app.state::<AppState>();
    let conn = state.db.lock().unwrap();
    let settings = load_settings_conn(&conn, &state)?;
    let scenes = db::list_scenes(&conn)?;
    let items = db::list_items(&conn)?;
    let mode = *state.panel_mode.lock().unwrap();
    let pending = state.pending_drop.lock().unwrap().clone();
    drop(conn);
    Ok(Bootstrap {
        settings,
        scenes,
        items,
        panel_mode: mode.as_str().to_string(),
        pending_drop: if pending.is_empty() {
            None
        } else {
            Some(PendingDrop { paths: pending })
        },
        version: VERSION.to_string(),
    })
}

#[tauri::command]
pub fn get_modifier_state() -> crate::win::ModifierState {
    crate::win::modifier_state()
}

#[tauri::command]
pub fn get_hotkey_defaults() -> settings::Hotkeys {
    settings::Hotkeys::with_defaults()
}

/* ---------- 暂存 ---------- */

fn staging_dir(settings: &Settings) -> Result<PathBuf, String> {
    let folder = settings
        .staging_folder
        .as_ref()
        .ok_or_else(|| "尚未设置暂存文件夹，请先在设置中选择".to_string())?;
    let dir = PathBuf::from(folder);
    fs::create_dir_all(&dir).map_err(|e| format!("暂存文件夹不可用: {e}"))?;
    Ok(dir)
}

fn active_scene(conn: &rusqlite::Connection, settings: &Settings) -> Result<i64, String> {
    let scenes = db::list_scenes(conn)?;
    if scenes.is_empty() {
        return Err("没有可用场景".into());
    }
    if scenes.iter().any(|s| s.id == settings.active_scene_id) {
        return Ok(settings.active_scene_id);
    }
    Ok(scenes[0].id)
}

#[tauri::command]
pub fn stage_paths(
    app: AppHandle,
    paths: Vec<String>,
    action: String,
) -> Result<Vec<StagedItem>, String> {
    let state = app.state::<AppState>();
    let conn = state.db.lock().unwrap();
    let settings = load_settings_conn(&conn, &state)?;
    let dir = staging_dir(&settings)?;
    let scene_id = active_scene(&conn, &settings)?;

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
                        scene_id,
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
                        scene_id,
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
    let _ = app.emit(events::ITEMS_CHANGED, ());
    Ok(created)
}

#[tauri::command]
pub fn stage_text(app: AppHandle, content: String) -> Result<StagedItem, String> {
    if content.trim().is_empty() {
        return Err("内容为空".into());
    }
    let state = app.state::<AppState>();
    let conn = state.db.lock().unwrap();
    let settings = load_settings_conn(&conn, &state)?;
    let dir = staging_dir(&settings)?;
    let scene_id = active_scene(&conn, &settings)?;

    let base = sanitize_text_name(content.lines().next().unwrap_or("文字"));
    let mut used = HashSet::new();
    let target = unique_target(&dir, &format!("{base}.txt"), &mut used);
    let size = content.len() as i64;
    fs::write(&target, content).map_err(|e| format!("写入失败: {e}"))?;

    let item = db::insert_item(
        &conn,
        &StagedItem {
            id: 0,
            scene_id,
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
    let _ = app.emit(events::ITEMS_CHANGED, ());
    Ok(item)
}

#[tauri::command]
pub fn list_items(app: AppHandle) -> Result<Vec<StagedItem>, String> {
    let state = app.state::<AppState>();
    let conn = state.db.lock().unwrap();
    db::list_items(&conn)
}

#[tauri::command]
pub fn remove_items(app: AppHandle, ids: Vec<i64>, delete_files: bool) -> Result<(), String> {
    let state = app.state::<AppState>();
    let conn = state.db.lock().unwrap();
    let items = db::items_by_ids(&conn, &ids)?;
    if delete_files {
        for it in &items {
            let p = Path::new(&it.staging_path);
            if p.exists() {
                let _ = trash::delete(p);
            }
        }
    }
    db::delete_items_by_ids(&conn, &ids)?;
    drop(conn);
    let _ = app.emit(events::ITEMS_CHANGED, ());
    Ok(())
}

/// 剪切拖出后的源清理：目标已接收（OLE 移动契约），删除暂存源（进回收站，可反悔）。
#[tauri::command]
pub fn finalize_drag_cut(app: AppHandle, paths: Vec<String>) -> Result<(), String> {
    let state = app.state::<AppState>();
    let conn = state.db.lock().unwrap();
    for p in &paths {
        let path = Path::new(p);
        if path.exists() {
            let _ = trash::delete(path);
        }
    }
    db::delete_items_by_paths(&conn, &paths)?;
    drop(conn);
    state.mark_staged();
    let _ = app.emit(events::ITEMS_CHANGED, ());
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

    if mode == "move" {
        db::delete_items_by_ids(&conn, &ids)?;
    }
    drop(conn);
    state.mark_staged();
    let _ = app.emit(events::ITEMS_CHANGED, ());
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
    let settings = {
        let conn = state.db.lock().unwrap();
        load_settings_conn(&conn, &state)?
    };
    let Some(folder) = settings.staging_folder else {
        return Ok(None);
    };
    // 仅允许读取暂存文件夹内的图片
    let base = PathBuf::from(&folder)
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let target = PathBuf::from(&path).canonicalize().map_err(|e| e.to_string())?;
    if !target.starts_with(&base) {
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

    // 缩到 256px 以内，PNG 编码后回传（避免大 JSON）
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

/* ---------- 场景 ---------- */

#[tauri::command]
pub fn list_scenes(app: AppHandle) -> Result<Vec<db::Scene>, String> {
    let state = app.state::<AppState>();
    let conn = state.db.lock().unwrap();
    db::list_scenes(&conn)
}

#[tauri::command]
pub fn create_scene(app: AppHandle, name: String) -> Result<db::Scene, String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("场景名不能为空".into());
    }
    let state = app.state::<AppState>();
    let scene = {
        let conn = state.db.lock().unwrap();
        db::create_scene(&conn, &name)?
    };
    crate::tray::refresh_scenes(&app);
    Ok(scene)
}

#[tauri::command]
pub fn rename_scene(app: AppHandle, id: i64, name: String) -> Result<(), String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("场景名不能为空".into());
    }
    let state = app.state::<AppState>();
    {
        let conn = state.db.lock().unwrap();
        db::rename_scene(&conn, id, &name)?;
    }
    crate::tray::refresh_scenes(&app);
    let _ = app.emit(events::ITEMS_CHANGED, ());
    Ok(())
}

#[tauri::command]
pub fn delete_scene(app: AppHandle, id: i64) -> Result<(), String> {
    let state = app.state::<AppState>();
    let settings = {
        let conn = state.db.lock().unwrap();
        let scenes = db::list_scenes(&conn)?;
        if scenes.len() <= 1 {
            return Err("至少保留一个场景".into());
        }
        let removed = db::delete_scene(&conn, id)?;
        for it in &removed {
            let p = Path::new(&it.staging_path);
            if p.exists() {
                let _ = trash::delete(p);
            }
        }
        let mut settings = load_settings_conn(&conn, &state)?;
        if settings.active_scene_id == id {
            if let Some(first) = db::first_scene_id(&conn)? {
                settings.active_scene_id = first;
                settings::persist(&conn, &settings)?;
            }
        }
        settings
    };
    drop(state);
    let _ = app.emit(events::ITEMS_CHANGED, ());
    let _ = app.emit(events::SETTINGS_CHANGED, settings);
    crate::tray::refresh_scenes(&app);
    Ok(())
}

pub fn set_active_scene_impl(app: &AppHandle, id: i64) -> Result<(), String> {
    let state = app.state::<AppState>();
    let settings = {
        let conn = state.db.lock().unwrap();
        let scenes = db::list_scenes(&conn)?;
        if !scenes.iter().any(|s| s.id == id) {
            return Err("场景不存在".into());
        }
        let mut settings = load_settings_conn(&conn, &state)?;
        settings.active_scene_id = id;
        settings::persist(&conn, &settings)?;
        settings
    };
    let _ = app.emit(events::SETTINGS_CHANGED, settings);
    crate::tray::refresh_scenes(app);
    Ok(())
}

#[tauri::command]
pub fn set_active_scene(app: AppHandle, id: i64) -> Result<(), String> {
    set_active_scene_impl(&app, id)
}

/* ---------- 设置 ---------- */

#[tauri::command]
pub fn save_settings(app: AppHandle, patch: serde_json::Value) -> Result<Settings, String> {
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

/* ---------- 窗口编排 ---------- */

#[tauri::command]
pub fn show_panel(app: AppHandle, cursor_y: Option<f64>) {
    manager::show_panel(&app, cursor_y);
}

#[tauri::command]
pub fn toggle_panel(app: AppHandle) {
    manager::toggle_panel(&app);
}

#[tauri::command]
pub fn hide_panel(app: AppHandle) {
    manager::hide_panel(&app);
}

#[tauri::command]
pub fn set_panel_mode(app: AppHandle, mode: String) {
    let state = app.state::<AppState>();
    state.set_mode(PanelMode::parse(&mode));
    manager::emit_panel_mode(&app);
}

#[tauri::command]
pub fn hold_pending_drop(app: AppHandle, paths: Vec<String>) {
    let state = app.state::<AppState>();
    *state.pending_drop.lock().unwrap() = paths;
    state.set_mode(PanelMode::Ask);
    manager::show_panel(&app, None);
    manager::emit_panel_mode(&app);
}

#[tauri::command]
pub fn report_presence(app: AppHandle, window: String, inside: bool) {
    manager::report_presence(&app, &window, inside);
}

#[tauri::command]
pub fn set_bar_hover(app: AppHandle, hovering: bool) {
    manager::set_bar_hover(&app, hovering);
}

#[tauri::command]
pub fn open_settings(app: AppHandle) {
    manager::open_settings(&app);
}

#[tauri::command]
pub fn set_panel_size(app: AppHandle, _width: u32, height: u32) {
    let state = app.state::<AppState>();
    let settings = load_settings(&state).unwrap_or_default();
    if let Some(panel) = manager::panel_webview(&app) {
        let scale = panel.scale_factor().unwrap_or(1.0);
        let w = (settings.panel_width as f64 * scale).round() as u32;
        let h = ((height as f64 * scale).round() as u32).clamp(160, 900);
        state.panel_height.store(h, std::sync::atomic::Ordering::Relaxed);
        let _ = panel.set_size(PhysicalSize::new(w, h));
    }
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
}
