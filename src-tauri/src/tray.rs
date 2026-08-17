//! 托盘：常驻入口与场景快捷切换。

use tauri::menu::{Menu, MenuBuilder, MenuItem, SubmenuBuilder};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager};

use crate::events;
use crate::manager;
use crate::state::AppState;

pub fn init(app: &AppHandle) -> tauri::Result<()> {
    let menu = build_menu(app)?;
    let icon = app
        .default_window_icon()
        .cloned()
        .ok_or_else(|| "缺少应用图标".to_string())?;
    TrayIconBuilder::with_id("tray")
        .icon(icon)
        .tooltip("浮匣 FloePod")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(on_menu_event)
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                manager::toggle_panel(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

fn build_menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let state = app.state::<AppState>();
    let conn = state.db.lock().unwrap();
    let settings = crate::settings::load(
        &conn,
        &state.data_dir.to_string_lossy(),
        env!("CARGO_PKG_VERSION"),
    )
    .unwrap_or_default();
    let scenes = db_scenes(&conn);
    drop(conn);

    let open_panel = MenuItem::with_id(app, "open_panel", "打开暂存面板", true, None::<&str>)?;
    let open_settings = MenuItem::with_id(app, "open_settings", "设置", true, None::<&str>)?;
    let open_folder = MenuItem::with_id(app, "open_folder", "打开暂存文件夹", true, None::<&str>)?;
    let collect = MenuItem::with_id(app, "collect_clipboard", "收集剪贴板文字", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出浮匣", true, None::<&str>)?;

    let mut sub = SubmenuBuilder::new(app, "场景");
    for sc in &scenes {
        let label = if sc.id == settings.active_scene_id {
            format!("{} ✓", sc.name)
        } else {
            sc.name.clone()
        };
        sub = sub.item(&MenuItem::with_id(
            app,
            format!("scene:{}", sc.id),
            label,
            true,
            None::<&str>,
        )?);
    }
    let scenes_menu = sub.build()?;

    MenuBuilder::new(app)
        .item(&open_panel)
        .item(&open_settings)
        .item(&open_folder)
        .item(&collect)
        .separator()
        .item(&scenes_menu)
        .separator()
        .item(&quit)
        .build()
}

fn db_scenes(conn: &rusqlite::Connection) -> Vec<crate::db::Scene> {
    crate::db::list_scenes(conn).unwrap_or_default()
}

pub fn refresh_scenes(app: &AppHandle) {
    if let Some(tray) = app.tray_by_id("tray") {
        if let Ok(menu) = build_menu(app) {
            let _ = tray.set_menu(Some(menu));
        }
    }
}

fn on_menu_event(app: &AppHandle, event: tauri::menu::MenuEvent) {
    match event.id().as_ref() {
        "open_panel" => manager::toggle_panel(app),
        "open_settings" => manager::open_settings(app),
        "open_folder" => open_staging_folder(app),
        "collect_clipboard" => {
            let _ = app.emit(events::COLLECT_CLIPBOARD, ());
        }
        "quit" => app.exit(0),
        id => {
            if let Some(id_str) = id.strip_prefix("scene:") {
                if let Ok(sid) = id_str.parse::<i64>() {
                    let _ = crate::commands::set_active_scene_impl(app, sid);
                }
            }
        }
    }
}

pub fn open_staging_folder(app: &AppHandle) {
    let state = app.state::<AppState>();
    let folder = {
        let conn = state.db.lock().unwrap();
        crate::settings::load(
            &conn,
            &state.data_dir.to_string_lossy(),
            env!("CARGO_PKG_VERSION"),
        )
        .ok()
        .and_then(|s| s.staging_folder)
    };
    if let Some(folder) = folder {
        use tauri_plugin_opener::OpenerExt;
        let _ = app.opener().open_path(folder, None::<&str>);
    }
}
