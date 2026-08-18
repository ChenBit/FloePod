//! 全局快捷键注册与分发。

use tauri::{AppHandle, Emitter};
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

    reg(&s.hotkeys.toggle_bar, on_toggle_bars)?;
    reg(&s.hotkeys.collect_clipboard, |app| {
        if let Some(id) = collect_into_first_pod(app) {
            let _ = app.emit(
                events::COLLECT_CLIPBOARD,
                serde_json::json!({ "podId": id }),
            );
        }
    })?;
    reg(&s.hotkeys.open_panel, on_open_panel)?;
    Ok(())
}

fn on_toggle_bars(app: &AppHandle) {
    crate::tray::toggle_bars(app);
}

/// 打开第一个可用匣的面板。
fn on_open_panel(app: &AppHandle) {
    let id = manager::current_settings(app)
        .pods
        .into_iter()
        .find(|p| p.enabled)
        .map(|p| p.id);
    if let Some(id) = id {
        manager::toggle_panel(app, id);
    }
}

/// 收集剪贴板：把文字暂存到第一个可用匣。
pub fn collect_into_first_pod(app: &AppHandle) -> Option<u64> {
    manager::current_settings(app)
        .pods
        .into_iter()
        .find(|p| p.enabled)
        .map(|p| p.id)
}
