use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::db;

pub const KEY: &str = "app";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Hotkeys {
    #[serde(default = "d_toggle_bar")]
    pub toggle_bar: String,
    #[serde(default = "d_collect_clipboard")]
    pub collect_clipboard: String,
    #[serde(default = "d_open_panel")]
    pub open_panel: String,
}

fn d_toggle_bar() -> String {
    "Alt+Shift+F".into()
}
fn d_collect_clipboard() -> String {
    "Alt+Shift+S".into()
}
fn d_open_panel() -> String {
    "Alt+Shift+P".into()
}

impl Hotkeys {
    pub fn with_defaults() -> Self {
        Self {
            toggle_bar: d_toggle_bar(),
            collect_clipboard: d_collect_clipboard(),
            open_panel: d_open_panel(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    #[serde(default)]
    pub staging_folder: Option<String>,
    #[serde(default = "d_drop_action")]
    pub drop_action: String,
    #[serde(default = "d_bar_form")]
    pub bar_form: String,
    #[serde(default = "d_edge")]
    pub edge: String,
    #[serde(default = "d_opacity")]
    pub opacity: f64,
    #[serde(default = "d_material")]
    pub material: String,
    #[serde(default = "d_hover_delay")]
    pub hover_delay_ms: u64,
    #[serde(default = "d_panel_width")]
    pub panel_width: u32,
    #[serde(default = "d_theme")]
    pub theme: String,
    #[serde(default)]
    pub autostart: bool,
    #[serde(default)]
    pub active_scene_id: i64,
    #[serde(default)]
    pub first_run_done: bool,
    #[serde(default = "Hotkeys::with_defaults")]
    pub hotkeys: Hotkeys,
    /// 只读：由应用在读取时注入，不持久化
    #[serde(skip_deserializing, default)]
    pub version: String,
    #[serde(skip_deserializing, default)]
    pub data_dir: String,
}

fn d_drop_action() -> String {
    "ask".into()
}
fn d_bar_form() -> String {
    "strip".into()
}
fn d_edge() -> String {
    "left".into()
}
fn d_opacity() -> f64 {
    0.85
}
fn d_material() -> String {
    "acrylic".into()
}
fn d_hover_delay() -> u64 {
    120
}
fn d_panel_width() -> u32 {
    380
}
fn d_theme() -> String {
    "system".into()
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            staging_folder: None,
            drop_action: d_drop_action(),
            bar_form: d_bar_form(),
            edge: d_edge(),
            opacity: d_opacity(),
            material: d_material(),
            hover_delay_ms: d_hover_delay(),
            panel_width: d_panel_width(),
            theme: d_theme(),
            autostart: false,
            active_scene_id: 0,
            first_run_done: false,
            hotkeys: Hotkeys::with_defaults(),
            version: String::new(),
            data_dir: String::new(),
        }
    }
}

pub fn load(conn: &Connection, data_dir: &str, version: &str) -> Result<Settings, String> {
    let mut s = match db::kv_get(conn, KEY)? {
        Some(json) => serde_json::from_str::<Settings>(&json).map_err(|e| e.to_string())?,
        None => Settings::default(),
    };
    s.version = version.to_string();
    s.data_dir = data_dir.to_string();
    Ok(s)
}

pub fn persist(conn: &Connection, s: &Settings) -> Result<(), String> {
    let json = serde_json::to_string(s).map_err(|e| e.to_string())?;
    db::kv_set(conn, KEY, &json)
}

/// 用 patch 合并当前设置并持久化；返回合并后的完整设置。
pub fn merge_persist(
    conn: &Connection,
    patch: serde_json::Value,
    data_dir: &str,
    version: &str,
) -> Result<Settings, String> {
    let mut stored: serde_json::Map<String, serde_json::Value> = match db::kv_get(conn, KEY)? {
        Some(json) => serde_json::from_str(&json).map_err(|e| e.to_string())?,
        None => serde_json::Map::new(),
    };
    if let Some(obj) = patch.as_object() {
        for (k, v) in obj {
            if k == "version" || k == "dataDir" {
                continue;
            }
            stored.insert(k.clone(), v.clone());
        }
    }
    // 首次选定暂存文件夹 => 完成首启
    if patch
        .get("stagingFolder")
        .map(|v| v.is_string())
        .unwrap_or(false)
    {
        let done = stored
            .get("firstRunDone")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if !done {
            stored.insert("firstRunDone".into(), true.into());
        }
    }
    let json = serde_json::to_string(&stored).map_err(|e| e.to_string())?;
    db::kv_set(conn, KEY, &json)?;
    load(conn, data_dir, version)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_writes_keys_and_first_run() {
        let conn = Connection::open_in_memory().unwrap();
        db::migrate(&conn).unwrap();
        let patch = serde_json::json!({ "stagingFolder": "D:\\暂存", "edge": "right" });
        let s = merge_persist(&conn, patch, "DATA", "0.1.0").unwrap();
        assert_eq!(s.staging_folder.as_deref(), Some("D:\\暂存"));
        assert_eq!(s.edge, "right");
        assert!(s.first_run_done);
        assert_eq!(s.version, "0.1.0");
        // 二次修改不重置
        let s2 = merge_persist(&conn, serde_json::json!({ "edge": "left" }), "DATA", "0.1.0").unwrap();
        assert!(s2.first_run_done);
        assert_eq!(s2.staging_folder.as_deref(), Some("D:\\暂存"));
    }
}
