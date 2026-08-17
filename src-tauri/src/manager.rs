//! 窗口编排：浮动条几何、面板定位与不抢焦点显隐、在场看门狗、设置应用。

use std::sync::atomic::Ordering;
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewWindow};

use crate::events;
use crate::settings::Settings;
use crate::state::{AppState, PanelMode};
use crate::win;

const STRIP_IDLE_W: i32 = 12;
const STRIP_EXPANDED_W: i32 = 48;
const BOOKMARK_W: i32 = 44;
const BOOKMARK_H: i32 = 176;
const PANEL_GAP: i32 = 10;

fn monitor(app: &AppHandle) -> Option<(i32, i32, i32, i32)> {
    // (x, y, w, h) 物理像素
    let mon = app.primary_monitor().ok().flatten()?;
    let size = mon.size();
    let pos = mon.position();
    Some((pos.x, pos.y, size.width as i32, size.height as i32))
}

pub fn bar_webview(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window("bar")
}

pub fn panel_webview(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window("panel")
}

fn is_expanded(state: &AppState) -> bool {
    state.bar_hovering.load(Ordering::Relaxed) || state.panel_visible.load(Ordering::Relaxed)
}

fn bar_anchor_width(app: &AppHandle, s: &Settings) -> i32 {
    let state = app.state::<AppState>();
    match s.bar_form.as_str() {
        "bookmark" => BOOKMARK_W,
        _ => {
            if is_expanded(&state) {
                STRIP_EXPANDED_W
            } else {
                STRIP_IDLE_W
            }
        }
    }
}

/// 按形态 / 边缘 / 展开态摆放浮动窗。条形态占满屏幕全高，书签形态垂直居中。
pub fn apply_bar(app: &AppHandle, s: &Settings) {
    let Some(bar) = bar_webview(app) else { return };
    let Some((mx, my, mw, mh)) = monitor(app) else { return };
    let state = app.state::<AppState>();

    let (w, h, y) = match s.bar_form.as_str() {
        "bookmark" => (BOOKMARK_W, BOOKMARK_H, my + (mh - BOOKMARK_H) / 2),
        _ => {
            let w = if is_expanded(&state) {
                STRIP_EXPANDED_W
            } else {
                STRIP_IDLE_W
            };
            (w, mh, my)
        }
    };
    let x = if s.edge == "right" {
        mx + mw - w
    } else {
        mx
    };
    let _ = bar.set_size(PhysicalSize::new(w as u32, h as u32));
    let _ = bar.set_position(PhysicalPosition::new(x, y));
}

/// 悬停态：true 立即加宽条窗；false 延迟约一个动画周期再收窄（避免裁剪收起动画）。
pub fn set_bar_hover(app: &AppHandle, hovering: bool) {
    let state = app.state::<AppState>();
    state.bar_hovering.store(hovering, Ordering::Relaxed);
    let s = current_settings(app);
    if hovering {
        apply_bar(app, &s);
    } else {
        let app2 = app.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(380));
            let state = app2.state::<AppState>();
            if !state.bar_hovering.load(Ordering::Relaxed)
                && !state.panel_visible.load(Ordering::Relaxed)
            {
                let s = current_settings(&app2);
                apply_bar(&app2, &s);
            }
        });
    }
}

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

pub fn apply_material(window: &WebviewWindow, material: &str) {
    use tauri::window::{Effect, EffectsBuilder};
    let effects: Vec<Effect> = if material == "acrylic" {
        vec![Effect::Acrylic]
    } else {
        vec![]
    };
    let _ = window.set_effects(EffectsBuilder::new().effects(effects));
}

/// 面板定位：贴着展开后的浮匣；y 跟随鼠标（悬停唤起时）或屏幕居中。
fn place_panel(app: &AppHandle, s: &Settings, cursor_y: Option<f64>) {
    let Some(panel) = panel_webview(app) else { return };
    let Some((mx, my, mw, mh)) = monitor(app) else { return };
    let state = app.state::<AppState>();

    let scale = panel.scale_factor().unwrap_or(1.0);
    let pw = (s.panel_width as f64 * scale).round() as i32;
    let ph = state.panel_height.load(Ordering::Relaxed) as i32;

    let bar_w = bar_anchor_width(app, s);
    let x = if s.edge == "right" {
        mx + mw - bar_w - PANEL_GAP - pw
    } else {
        mx + bar_w + PANEL_GAP
    };

    let mut y = match cursor_y {
        Some(cy) => {
            let bar_scale = bar_webview(app)
                .and_then(|b| b.scale_factor().ok())
                .unwrap_or(1.0);
            my + (cy * bar_scale).round() as i32 - 56
        }
        None => my + (mh - ph) / 2,
    };
    let lo = my + 8;
    let hi = (my + mh - ph - 56).max(lo);
    y = y.clamp(lo, hi);

    let _ = panel.set_size(PhysicalSize::new(pw as u32, ph.max(120) as u32));
    let _ = panel.set_position(PhysicalPosition::new(x, y));
}

pub fn emit_panel_mode(app: &AppHandle) {
    let state = app.state::<AppState>();
    let mode = *state.panel_mode.lock().unwrap();
    let paths = state.pending_drop.lock().unwrap().clone();
    let _ = app.emit(
        events::PANEL_MODE,
        serde_json::json!({ "mode": mode.as_str(), "paths": paths }),
    );
}

/// 悬停唤起：不抢焦点显示，不改变固定状态。
pub fn show_panel(app: &AppHandle, cursor_y: Option<f64>) {
    let state = app.state::<AppState>();
    if state.panel_visible.load(Ordering::Relaxed) {
        return;
    }
    let s = current_settings(app);
    if let Some(panel) = panel_webview(app) {
        place_panel(app, &s, cursor_y);
        apply_material(&panel, &s.material);
        if let Ok(hwnd) = panel.hwnd() {
            win::show_no_activate(hwnd.0 as isize);
            state.panel_visible.store(true, Ordering::Relaxed);
            emit_panel_mode(app);
            apply_bar(app, &s);
        }
    }
}

/// 单击 / 托盘 / 热键：未开 -> 打开并固定；已固定 -> 收起；悬停展开中 -> 固定。
pub fn toggle_panel(app: &AppHandle) {
    let state = app.state::<AppState>();
    if !state.panel_visible.load(Ordering::Relaxed) {
        show_panel(app, None);
        state.panel_pinned.store(true, Ordering::Relaxed);
    } else if state.panel_pinned.load(Ordering::Relaxed) {
        hide_panel(app);
    } else {
        state.panel_pinned.store(true, Ordering::Relaxed);
    }
}

pub fn hide_panel(app: &AppHandle) {
    let state = app.state::<AppState>();
    if let Some(panel) = panel_webview(app) {
        let _ = panel.hide();
    }
    state.panel_visible.store(false, Ordering::Relaxed);
    state.panel_pinned.store(false, Ordering::Relaxed);
    state.set_mode(PanelMode::List);
    let s = current_settings(app);
    apply_bar(app, &s);
}

pub fn report_presence(app: &AppHandle, window: &str, inside: bool) {
    let state = app.state::<AppState>();
    let mut p = state.presence.lock().unwrap();
    match window {
        "bar" => p.bar_inside = inside,
        "panel" => p.panel_inside = inside,
        _ => {}
    }
    p.last_change = Some(std::time::Instant::now());
}

/// 看门狗：面板未固定且指针已离开所有窗口 -> 自动收起。
pub fn spawn_watchdog(app: AppHandle) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(120));
        let state = app.state::<AppState>();
        let should_hide = {
            let p = state.presence.lock().unwrap();
            state.panel_visible.load(Ordering::Relaxed)
                && !state.panel_pinned.load(Ordering::Relaxed)
                && !p.bar_inside
                && !p.panel_inside
                && p.last_change
                    .map(|t| t.elapsed() > Duration::from_millis(320))
                    .unwrap_or(false)
        };
        if should_hide {
            hide_panel(&app);
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

/// 设置落地：几何、材质、热键、自启、监听、托盘全量应用。
pub fn apply_settings(app: &AppHandle, s: &Settings) {
    apply_bar(app, s);
    if let Some(bar) = bar_webview(app) {
        apply_material(&bar, &s.material);
    }
    if let Some(panel) = panel_webview(app) {
        apply_material(&panel, &s.material);
    }

    // 自启
    use tauri_plugin_autostart::ManagerExt as _;
    let autolaunch = app.autolaunch();
    if s.autostart {
        let _ = autolaunch.enable();
    } else {
        let _ = autolaunch.disable();
    }

    // 暂存文件夹监听
    if let Some(folder) = &s.staging_folder {
        crate::watcher::restart(app, folder.clone());
    }

    crate::tray::refresh_scenes(app);

    let _ = app.emit(events::SETTINGS_CHANGED, s.clone());
}
