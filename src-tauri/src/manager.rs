//! 窗口编排：多「匣」窗口的创建与摆放、面板显隐（不抢焦点 + 看门狗动画收起）。

use std::collections::HashMap;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, State, WebviewWindow, WebviewWindowBuilder};

use crate::events;
use crate::settings::{Pod, Settings};
use crate::state::{AppState, PanelMode, PodRuntime};
use crate::win;

/// 匣（胶囊条）的短边（贴屏幕边缘一侧）与长边
const POD_BAR_SHORT: i32 = 44;
const POD_BAR_LONG: i32 = 190;
/// 拖入接纳态：短条变宽为圆角矩形
const POD_BAR_ACCEPT: i32 = 62;
const PANEL_GAP: i32 = 10;

pub fn current_settings(app: &AppHandle) -> Settings {
    let state = app.state::<AppState>();
    let conn = state.db.lock().unwrap();
    crate::settings::load(
        &conn,
        &state.data_dir.to_string_lossy(),
        env!("CARGO_PKG_VERSION"),
    )
    .unwrap_or_default()
}

fn pod_of(app: &AppHandle, id: u64) -> Option<Pod> {
    current_settings(app).pods.into_iter().find(|p| p.id == id)
}

pub fn pod_bar(app: &AppHandle, id: u64) -> Option<WebviewWindow> {
    app.get_webview_window(&format!("pod_{id}"))
}

pub fn pod_panel(app: &AppHandle, id: u64) -> Option<WebviewWindow> {
    app.get_webview_window(&format!("pod_{id}_panel"))
}

fn pods_guard<'a>(
    state: &'a State<'_, AppState>,
) -> std::sync::MutexGuard<'a, HashMap<u64, PodRuntime>> {
    state.pods.lock().unwrap()
}

/// 找到匣所在显示器：按名称匹配，空名或未找到回退主显示器。
fn monitor(app: &AppHandle, pod: &Pod) -> Option<(i32, i32, i32, i32)> {
    let monitors = app.available_monitors().ok()?;
    if !pod.monitor.is_empty() {
        for m in &monitors {
            if m.name().map(|s| s.as_str()) == Some(pod.monitor.as_str()) {
                let size = m.size();
                let pos = m.position();
                return Some((pos.x, pos.y, size.width as i32, size.height as i32));
            }
        }
    }
    let m = app.primary_monitor().ok().flatten()?;
    let size = m.size();
    let pos = m.position();
    Some((pos.x, pos.y, size.width as i32, size.height as i32))
}

pub fn list_monitors(app: &AppHandle) -> Vec<serde_json::Value> {
    let Some(monitors) = app.available_monitors().ok() else {
        return vec![];
    };
    let primary = app.primary_monitor().ok().flatten();
    let mut out = Vec::new();
    let mut idx = 0usize;
    for m in &monitors {
        let is_primary = primary.as_ref().map(|p| p.name() == m.name()).unwrap_or(false);
        idx += 1;
        let label = if is_primary {
            "主显示器".to_string()
        } else {
            format!("显示器 {idx}")
        };
        out.push(serde_json::json!({
            "name": m.name().map(|s| s.as_str()).unwrap_or(""),
            "label": label,
            "primary": is_primary,
        }));
    }
    out
}

/// 胶囊条窗口的几何（长边方向由边缘决定）。
fn bar_geometry(app: &AppHandle, pod: &Pod, accepting: bool) -> Option<(i32, i32, i32, i32)> {
    // (x, y, w, h)，物理像素
    let (mx, my, mw, mh) = monitor(app, pod)?;
    let short = if accepting { POD_BAR_ACCEPT } else { POD_BAR_SHORT };
    let vertical = pod.is_vertical();
    let (w, h) = if vertical {
        (short, POD_BAR_LONG)
    } else {
        (POD_BAR_LONG, short)
    };
    let (x, y) = match pod.edge.as_str() {
        "right" => (mx + mw - w, my + (mh as f64 * pod.offset).round() as i32 - h / 2),
        "bottom" => (mx + (mw as f64 * pod.offset).round() as i32 - w / 2, my + mh - h),
        "top" => (mx + (mw as f64 * pod.offset).round() as i32 - w / 2, my),
        _ => (mx, my + (mh as f64 * pod.offset).round() as i32 - h / 2),
    };
    let max_y = (my + mh - h).max(my);
    let y = y.clamp(my, max_y);
    let max_x = (mx + mw - w).max(mx);
    let x = x.clamp(mx, max_x);
    Some((x, y, w, h))
}

pub fn place_pod_bar(app: &AppHandle, pod: &Pod, accepting: bool) {
    let Some(bar) = pod_bar(app, pod.id) else { return };
    if let Some((x, y, w, h)) = bar_geometry(app, pod, accepting) {
        let _ = bar.set_size(PhysicalSize::new(w as u32, h as u32));
        let _ = bar.set_position(PhysicalPosition::new(x, y));
    }
}

/// 面板尺寸变化后重新贴靠（按当前已保存的 panel_height）。
pub fn place_panel_dyn(app: &AppHandle, pod_id: u64) {
    if let Some(pod) = pod_of(app, pod_id) {
        place_panel(app, &pod);
    }
}

/// 面板：贴着匣弹出，长边方向垂直/水平时对齐到匣中心。
fn place_panel(app: &AppHandle, pod: &Pod) {
    let Some(panel) = pod_panel(app, pod.id) else { return };
    let state = app.state::<AppState>();
    let ph = {
        let guard = state.pods.lock().unwrap();
        guard.get(&pod.id).map(|r| r.panel_height).unwrap_or(420)
    };
    let scale = panel.scale_factor().unwrap_or(1.0);
    let pw = (pod.panel_width as f64 * scale).round() as i32;
    let ph = (ph as f64 * scale).round() as i32;

    let Some((mx, my, mw, mh)) = monitor(app, pod) else { return };
    let (bx, by, bw, bh) = bar_geometry(app, pod, false).unwrap_or((mx, my, POD_BAR_SHORT, POD_BAR_LONG));

    let (x, y) = match pod.edge.as_str() {
        "right" => (bx - PANEL_GAP - pw, by + bh / 2 - ph / 2),
        "bottom" => (bx + bw / 2 - pw / 2, by - PANEL_GAP - ph),
        "top" => (bx + bw / 2 - pw / 2, by + bh + PANEL_GAP),
        _ => (bx + bw + PANEL_GAP, by + bh / 2 - ph / 2),
    };
    let x = x.clamp(mx + 8, mx + mw - pw - 8).max(mx);
    let y = y.clamp(my + 8, my + mh - ph - 8).max(my);
    let _ = panel.set_size(PhysicalSize::new(pw as u32, ph.max(120) as u32));
    let _ = panel.set_position(PhysicalPosition::new(x, y));
}

fn ensure_pod_windows(app: &AppHandle, pod: &Pod) {
    let bar_label = format!("pod_{}", pod.id);
    let panel_label = format!("pod_{}_panel", pod.id);
    if app.get_webview_window(&bar_label).is_none() {
        let _ = WebviewWindowBuilder::new(
            app,
            &bar_label,
            tauri::WebviewUrl::App("index.html".into()),
        )
        .title(&pod.name)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .shadow(false)
        .focusable(false)
        .visible(false)
        .build();
    }
    if app.get_webview_window(&panel_label).is_none() {
        let _ = WebviewWindowBuilder::new(
            app,
            &panel_label,
            tauri::WebviewUrl::App("index.html".into()),
        )
        .title(format!("{} 面板", pod.name))
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .shadow(true)
        .visible(false)
        .build();
    }
    place_pod_bar(app, pod, false);
}

fn destroy_pod_windows(app: &AppHandle, id: u64) {
    let labels = [format!("pod_{id}"), format!("pod_{id}_panel")];
    for l in labels {
        if let Some(w) = app.get_webview_window(&l) {
            let _ = w.destroy();
        }
    }
}

/// 让所有窗口与当前设置中的匣对齐：创建缺失、销毁多余的。
pub fn sync_pods(app: &AppHandle) {
    let s = current_settings(app);
    let wanted: HashMap<u64, &Pod> = s
        .pods
        .iter()
        .filter(|p| p.enabled)
        .map(|p| (p.id, p))
        .collect();

    let existing: Vec<u64> = app
        .webview_windows()
        .keys()
        .filter_map(|l| {
            if let Some(rest) = l.strip_prefix("pod_") {
                let id_str = rest.strip_suffix("_panel").unwrap_or(rest);
                id_str.parse::<u64>().ok()
            } else {
                None
            }
        })
        .collect();

    for id in existing {
        if !wanted.contains_key(&id) {
            destroy_pod_windows(app, id);
            app.state::<AppState>().pods.lock().unwrap().remove(&id);
        }
    }

    for pod in s.pods.iter().filter(|p| p.enabled) {
        ensure_pod_windows(app, pod);
    }
}

fn apply_material(window: &WebviewWindow, material: &str) {
    use tauri::window::{Effect, EffectsBuilder};
    if material == "acrylic" {
        let config = EffectsBuilder::new().effects([Effect::Acrylic]).build();
        let _ = window.set_effects(Some(config));
    } else {
        let _ = window.set_effects(None);
    }
}

/* ---------- 面板显隐（按匣） ---------- */

pub fn emit_panel_mode(app: &AppHandle, id: u64) {
    let state = app.state::<AppState>();
    let (mode, paths) = {
        let guard = pods_guard(&state);
        let r = guard.get(&id);
        (
            r.map(|r| r.mode).unwrap_or(PanelMode::List).as_str().to_string(),
            r.map(|r| r.pending_drop.clone()).unwrap_or_default(),
        )
    };
    if let Some(panel) = pod_panel(app, id) {
        let _ = panel.emit(events::PANEL_MODE, serde_json::json!({ "mode": mode, "paths": paths }));
    }
}

pub fn emit_panel_pinned(app: &AppHandle, id: u64) {
    let state = app.state::<AppState>();
    let pinned = pods_guard(&state).get(&id).map(|r| r.panel_pinned).unwrap_or(false);
    if let Some(panel) = pod_panel(app, id) {
        let _ = panel.emit(events::PANEL_PINNED, serde_json::json!({ "pinned": pinned }));
    }
}

pub fn show_panel(app: &AppHandle, id: u64) {
    let state = app.state::<AppState>();
    {
        let mut guard = state.pods.lock().unwrap();
        let r = guard.entry(id).or_default();
        if r.panel_visible {
            // 正在动画收起中被再次唤起：取消收起，重播滑入
            if r.panel_hiding {
                r.panel_hiding = false;
                if let Some(panel) = pod_panel(app, id) {
                    let _ = panel.emit(events::PANEL_SHOWN, ());
                }
            }
            return;
        }
        r.panel_visible = true;
        r.panel_hiding = false;
    }
    let Some(pod) = pod_of(app, id) else {
        state.pods.lock().unwrap().get_mut(&id).map(|r| r.panel_visible = false);
        return;
    };
    if let Some(panel) = pod_panel(app, id) {
        place_panel(app, &pod);
        apply_material(&panel, &pod.material);
        if let Ok(hwnd) = panel.hwnd() {
            win::show_no_activate(hwnd.0 as isize);
            emit_panel_mode(app, id);
        }
        let _ = panel.emit(events::PANEL_SHOWN, ());
    }
    emit_panel_pinned(app, id);
}

pub fn hide_panel(app: &AppHandle, id: u64) {
    // 用 Win32 直接隐藏：Tauri 的 hide() 对 WebView2 调 SetIsVisible(false) 会重新显示窗口
    if let Some(panel) = pod_panel(app, id) {
        if let Ok(hwnd) = panel.hwnd() {
            win::hide_window(hwnd.0 as isize);
        }
    }
    let state = app.state::<AppState>();
    {
        let mut guard = pods_guard(&state);
        if let Some(r) = guard.get_mut(&id) {
            r.panel_visible = false;
            r.panel_pinned = false;
            r.panel_hiding = false;
            r.mode = PanelMode::List;
            r.pending_drop.clear();
        }
    }
    emit_panel_pinned(app, id);
}

pub fn toggle_panel(app: &AppHandle, id: u64) {
    let state = app.state::<AppState>();
    let (visible, pinned) = {
        let guard = pods_guard(&state);
        let r = guard.get(&id);
        (
            r.map(|r| r.panel_visible).unwrap_or(false),
            r.map(|r| r.panel_pinned).unwrap_or(false),
        )
    };
    if !visible {
        show_panel(app, id);
        pods_guard(&state).get_mut(&id).map(|r| r.panel_pinned = true);
    } else if pinned {
        hide_panel(app, id);
    } else {
        pods_guard(&state).get_mut(&id).map(|r| r.panel_pinned = true);
    }
    emit_panel_pinned(app, id);
}

pub fn set_panel_pinned(app: &AppHandle, id: u64, pinned: bool) {
    let state = app.state::<AppState>();
    if pinned {
        let visible = pods_guard(&state).get(&id).map(|r| r.panel_visible).unwrap_or(false);
        if !visible {
            show_panel(app, id);
        }
        pods_guard(&state).get_mut(&id).map(|r| {
            r.panel_pinned = true;
            r.panel_hiding = false;
        });
    } else {
        pods_guard(&state).get_mut(&id).map(|r| r.panel_pinned = false);
    }
    emit_panel_pinned(app, id);
}

pub fn set_dragging_out(app: &AppHandle, id: u64, dragging: bool) {
    let state = app.state::<AppState>();
    pods_guard(&state).get_mut(&id).map(|r| r.dragging_out = dragging);
}

pub fn report_presence(app: &AppHandle, id: u64, window: &str, inside: bool) {
    let state = app.state::<AppState>();
    let mut guard = pods_guard(&state);
    let r = guard.entry(id).or_default();
    match window {
        "bar" => r.bar_inside = inside,
        "panel" => r.panel_inside = inside,
        _ => {}
    }
    r.last_change = Some(Instant::now());
    // 指针回到任意窗口内：取消正在进行的动画收起
    if inside {
        r.panel_hiding = false;
    }
}

/// 拖入接纳：短条变为圆角矩形（窗口加宽），结束后收回。
pub fn set_pod_accept(app: &AppHandle, id: u64, accepting: bool) {
    let Some(pod) = pod_of(app, id) else { return };
    place_pod_bar(app, &pod, accepting);
}

/// 看门狗：逐个匣检查——面板未固定、未在拖出、列表模式且指针离开 -> 动画收起。
pub fn spawn_watchdog(app: AppHandle) {
    std::thread::spawn(move || {
        let mut hide_requested_at: HashMap<u64, Instant> = HashMap::new();
        loop {
            std::thread::sleep(Duration::from_millis(100));
            let state = app.state::<AppState>();
            let ids: Vec<u64> = state.pods.lock().unwrap().keys().copied().collect();
            for id in ids {
                let action: Option<bool> = {
                    let mut guard = state.pods.lock().unwrap();
                    let Some(r) = guard.get_mut(&id) else { continue };
                    if !r.panel_visible {
                        hide_requested_at.remove(&id);
                        continue;
                    }
                    if r.panel_hiding {
                        if hide_requested_at
                            .get(&id)
                            .map(|t| t.elapsed() > Duration::from_millis(1500))
                            .unwrap_or(false)
                        {
                            Some(true) // 兜底强制隐藏
                        } else {
                            None
                        }
                    } else {
                        let away = !r.panel_pinned
                            && !r.dragging_out
                            && r.mode == PanelMode::List
                            && !r.bar_inside
                            && !r.panel_inside
                            && r.last_change
                                .map(|t| t.elapsed() > Duration::from_millis(320))
                                .unwrap_or(false);
                        if away {
                            r.panel_hiding = true;
                            hide_requested_at.insert(id, Instant::now());
                            Some(false) // 播收起动画
                        } else {
                            hide_requested_at.remove(&id);
                            None
                        }
                    }
                };
                match action {
                    Some(false) => {
                        if let Some(panel) = pod_panel(&app, id) {
                            let _ = panel.emit(events::PANEL_HIDE_REQUEST, ());
                        }
                    }
                    Some(true) => {
                        hide_panel(&app, id);
                        hide_requested_at.remove(&id);
                    }
                    None => {}
                }
            }
        }
    });
}

pub fn open_settings(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("settings") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// 显示 / 隐藏全部匣。
pub fn set_all_bars(app: &AppHandle, visible: bool) {
    let s = current_settings(app);
    for pod in s.pods.iter().filter(|p| p.enabled) {
        if let Some(bar) = pod_bar(app, pod.id) {
            if let Ok(hwnd) = bar.hwnd() {
                if visible {
                    win::show_no_activate(hwnd.0 as isize);
                } else {
                    win::hide_window(hwnd.0 as isize);
                    hide_panel(app, pod.id);
                }
            }
        }
    }
}

/// 设置落地：同步匣窗口、材质、自启、监听、托盘全量应用。
pub fn apply_settings(app: &AppHandle, s: &Settings) {
    sync_pods(app);

    for pod in s.pods.iter().filter(|p| p.enabled) {
        if let Some(bar) = pod_bar(app, pod.id) {
            apply_material(&bar, "plain");
        }
    }

    // 自启
    use tauri_plugin_autostart::ManagerExt as _;
    let autolaunch = app.autolaunch();
    if s.autostart {
        let _ = autolaunch.enable();
    } else {
        let _ = autolaunch.disable();
    }

    // 暂存文件夹监听（每个匣一个）
    crate::watcher::restart_all(app);

    // 配置完成（OOBE 结束）后亮相
    if s.first_run_done && !s.pods.is_empty() {
        for pod in s.pods.iter().filter(|p| p.enabled) {
            if let Some(bar) = pod_bar(app, pod.id) {
                if let Ok(hwnd) = bar.hwnd() {
                    win::show_no_activate(hwnd.0 as isize);
                }
            }
        }
    }

    crate::tray::refresh_menu(app);
    let _ = app.emit(events::SETTINGS_CHANGED, s.clone());
}
