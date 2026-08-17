//! 全局快捷键注册与分发。

use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

use crate::events;
use crate::manager;
use crate::settings::Settings;

pub fn register(app: &AppHandle, s: &Settings) -> Result<(), String> {
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();

    let reg = |combo: &str, action: fn(&AppHandle)| -> Result<(), String> {
        if combo.is_empty() {
            return Ok(());
        }
        gs.on_shortcut(combo, move |app, _shortcut, e| {
                if e.state() == ShortcutState::Pressed {
                    action(app);
                }
            })
            .map_err(|err| format!("快捷键「{combo}」注册失败，可能与其他软件冲突（{err}）"))
    };

    reg(&s.hotkeys.toggle_bar, on_toggle_bar)?;
    reg(&s.hotkeys.collect_clipboard, |app| {
        let _ = app.emit(events::COLLECT_CLIPBOARD, ());
    })?;
    reg(&s.hotkeys.open_panel, |app| manager::toggle_panel(app))?;
    Ok(())
}

fn on_toggle_bar(app: &AppHandle) {
    if let Some(bar) = app.get_webview_window("bar") {
        match bar.is_visible() {
            Ok(true) => {
                let _ = bar.hide();
                // 条隐藏时面板一并收起
                let state = app.state::<crate::state::AppState>();
                if state.panel_visible.load(std::sync::atomic::Ordering::Relaxed) {
                    drop(state);
                    manager::hide_panel(app);
                }
            }
            _ => {
                let _ = bar.show();
            }
        }
    }
}
