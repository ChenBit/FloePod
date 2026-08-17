//! 浮匣 FloePod - 本地优先的屏幕边缘文件暂存工具。

mod commands;
mod db;
mod events;
mod hotkeys;
mod lnk;
mod manager;
mod paths;
mod settings;
mod state;
mod tray;
mod watcher;
mod win;

use tauri::Manager;

const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn run() {
    // 数据目录与数据库在窗口创建前准备好并注册，避免前端过早调用命令时 state 未就绪
    let data_dir = paths::resolve();
    let conn = match db::open(&data_dir) {
        Ok(c) => c,
        Err(e) => panic!("无法打开数据库: {e}"),
    };
    if let Err(e) = db::ensure_default_scene(&conn) {
        panic!("初始化场景失败: {e}");
    }
    let app_state = state::AppState::new(conn, data_dir.clone());

    tauri::Builder::default()
        .manage(app_state)
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            // 二次启动：唤起已有实例
            manager::open_settings(&app);
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None::<Vec<&str>>,
        ))
        .plugin(tauri_plugin_drag::init())
        .setup(|app| {
            let settings = {
                let state = app.state::<state::AppState>();
                let conn = state.db.lock().unwrap();
                settings::load(
                    &conn,
                    &state.data_dir.to_string_lossy(),
                    VERSION,
                )?
            };

            tray::init(app.handle())?;

            // 首启 / 未配置：引导设置；否则直接亮相
            if settings.staging_folder.is_some() && settings.first_run_done {
                if let Some(bar) = manager::bar_webview(app.handle()) {
                    let _ = bar.show();
                }
            }

            manager::apply_settings(app.handle(), &settings);
            if let Err(e) = hotkeys::register(app.handle(), &settings) {
                eprintln!("[hotkeys] {e}");
            }
            manager::spawn_watchdog(app.handle().clone());

            if settings.staging_folder.is_none() || !settings.first_run_done {
                manager::open_settings(app.handle());
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                match window.label() {
                    "settings" => {
                        api.prevent_close();
                        let _ = window.hide();
                    }
                    "bar" => {
                        api.prevent_close();
                    }
                    "panel" => {
                        api.prevent_close();
                        manager::hide_panel(&window.app_handle());
                    }
                    _ => {}
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_bootstrap,
            commands::get_modifier_state,
            commands::get_hotkey_defaults,
            commands::stage_paths,
            commands::stage_text,
            commands::list_items,
            commands::remove_items,
            commands::finalize_drag_cut,
            commands::export_items,
            commands::read_thumbnail,
            commands::list_scenes,
            commands::create_scene,
            commands::rename_scene,
            commands::delete_scene,
            commands::set_active_scene,
            commands::save_settings,
            commands::show_panel,
            commands::toggle_panel,
            commands::hide_panel,
            commands::set_panel_mode,
            commands::hold_pending_drop,
            commands::report_presence,
            commands::set_bar_hover,
            commands::open_settings,
            commands::set_panel_size,
            commands::quit_app,
        ])
        .run(tauri::generate_context!())
        .expect("error while running FloePod");
}
