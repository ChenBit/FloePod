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

/// 一个「匣」：贴在屏幕边缘的独立暂存点，拥有自己的保存文件夹与外观。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Pod {
    pub id: u64,
    pub name: String,
    /// top / right / bottom / left
    pub edge: String,
    /// 显示器名；空串 = 主显示器
    pub monitor: String,
    /// 沿边缘的位置 0.0 - 1.0
    pub offset: f64,
    pub staging_folder: String,
    pub opacity: f64,
    pub material: String,
    pub panel_width: u32,
    pub hover_delay_ms: u64,
    pub drop_action: String,
    pub enabled: bool,
}

impl Default for Pod {
    fn default() -> Self {
        Pod {
            id: 0,
            name: "新匣".into(),
            edge: "left".into(),
            monitor: String::new(),
            offset: 0.5,
            staging_folder: String::new(),
            opacity: 0.85,
            material: "acrylic".into(),
            panel_width: 380,
            hover_delay_ms: 120,
            drop_action: "ask".into(),
            enabled: true,
        }
    }
}

impl Pod {
    pub fn is_vertical(&self) -> bool {
        matches!(self.edge.as_str(), "left" | "right")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    #[serde(default = "d_theme")]
    pub theme: String,
    #[serde(default)]
    pub first_run_done: bool,
    #[serde(default)]
    pub autostart: bool,
    #[serde(default = "Hotkeys::with_defaults")]
    pub hotkeys: Hotkeys,
    #[serde(default)]
    pub pods: Vec<Pod>,
    /// 只读：由应用在读取时注入，不持久化
    #[serde(skip_deserializing, default)]
    pub version: String,
    #[serde(skip_deserializing, default)]
    pub data_dir: String,
}

fn d_theme() -> String {
    "system".into()
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            theme: d_theme(),
            first_run_done: false,
            autostart: false,
            hotkeys: Hotkeys::with_defaults(),
            pods: Vec::new(),
            version: String::new(),
            data_dir: String::new(),
        }
    }
}

pub fn load(conn: &Connection, data_dir: &str, version: &str) -> Result<Settings, String> {
    let mut s: Settings = match db::kv_get(conn, KEY)? {
        Some(json) => {
            let v: serde_json::Value = serde_json::from_str(&json).map_err(|e| e.to_string())?;
            let mut s: Settings =
                serde_json::from_value(v.clone()).map_err(|e| e.to_string())?;
            migrate_legacy(&mut s, &v);
            s
        }
        None => Settings::default(),
    };
    s.version = version.to_string();
    s.data_dir = data_dir.to_string();
    Ok(s)
}

/// 旧版（0.2/0.3）单个暂存配置 -> 生成一个默认「匣」，保证老用户升级不丢配置。
fn migrate_legacy(s: &mut Settings, v: &serde_json::Value) {
    if !s.pods.is_empty() {
        return;
    }
    let folder = match v.get("stagingFolder").and_then(|x| x.as_str()) {
        Some(f) if !f.is_empty() => f.to_string(),
        _ => return,
    };
    let edge = v
        .get("edge")
        .and_then(|x| x.as_str())
        .filter(|e| matches!(*e, "top" | "right" | "bottom" | "left"))
        .unwrap_or("left");
    s.pods.push(Pod {
        id: 1,
        name: "我的匣".into(),
        edge: edge.into(),
        monitor: String::new(),
        offset: 0.5,
        staging_folder: folder,
        opacity: v.get("opacity").and_then(|x| x.as_f64()).unwrap_or(0.85),
        material: v
            .get("material")
            .and_then(|x| x.as_str())
            .unwrap_or("acrylic")
            .into(),
        panel_width: v.get("panelWidth").and_then(|x| x.as_u64()).unwrap_or(380) as u32,
        hover_delay_ms: v.get("hoverDelayMs").and_then(|x| x.as_u64()).unwrap_or(120),
        drop_action: v
            .get("dropAction")
            .and_then(|x| x.as_str())
            .unwrap_or("ask")
            .into(),
        enabled: true,
    });
}

pub fn persist(conn: &Connection, s: &Settings) -> Result<(), String> {
    let json = serde_json::to_string(s).map_err(|e| e.to_string())?;
    db::kv_set(conn, KEY, &json)
}

/// 用 patch 合并当前设置并持久化；返回合并后的完整设置。
/// pod 列表不通过此命令修改（走独立 pod 命令），仅合并标量字段。
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
            if k == "version" || k == "dataDir" || k == "pods" {
                continue;
            }
            stored.insert(k.clone(), v.clone());
        }
    }
    let json = serde_json::to_string(&stored).map_err(|e| e.to_string())?;
    db::kv_set(conn, KEY, &json)?;
    load(conn, data_dir, version)
}

/* ---------- pod 增删改（读写设置并持久化） ---------- */

pub fn next_pod_id(conn: &Connection, data_dir: &str, version: &str) -> Result<u64, String> {
    let s = load(conn, data_dir, version)?;
    Ok(s.pods.iter().map(|p| p.id).max().unwrap_or(0) + 1)
}

pub fn upsert_pod(
    conn: &Connection,
    pod: &Pod,
    data_dir: &str,
    version: &str,
) -> Result<Settings, String> {
    let mut s = load(conn, data_dir, version)?;
    if let Some(existing) = s.pods.iter_mut().find(|p| p.id == pod.id) {
        *existing = pod.clone();
    } else {
        s.pods.push(pod.clone());
    }
    persist(conn, &s)?;
    Ok(s)
}

pub fn delete_pod(
    conn: &Connection,
    id: u64,
    data_dir: &str,
    version: &str,
) -> Result<Settings, String> {
    let mut s = load(conn, data_dir, version)?;
    s.pods.retain(|p| p.id != id);
    persist(conn, &s)?;
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> Connection {
        let mut c = Connection::open_in_memory().unwrap();
        db::migrate(&mut c).unwrap();
        c
    }

    #[test]
    fn legacy_settings_migrates_to_pod() {
        let c = conn();
        db::kv_set(
            &c,
            KEY,
            r#"{"stagingFolder":"D:\\暂存","edge":"right","firstRunDone":true}"#,
        )
        .unwrap();
        let s = load(&c, "DATA", "0.4.0").unwrap();
        assert!(s.first_run_done);
        assert_eq!(s.pods.len(), 1);
        assert_eq!(s.pods[0].staging_folder, "D:\\暂存");
        assert_eq!(s.pods[0].edge, "right");
        assert_eq!(s.pods[0].id, 1);
    }

    #[test]
    fn merge_ignores_pods_and_version() {
        let c = conn();
        db::kv_set(&c, KEY, r#"{"theme":"system","pods":[]}"#).unwrap();
        let s = merge_persist(
            &c,
            serde_json::json!({"theme":"dark","pods":[{"id":99}],"version":"9.9"}),
            "DATA",
            "0.4.0",
        )
        .unwrap();
        assert_eq!(s.theme, "dark");
        assert!(s.pods.is_empty());
        assert_eq!(s.version, "0.4.0");
    }

    #[test]
    fn pod_upsert_delete() {
        let c = conn();
        let pod = Pod {
            id: 1,
            name: "A".into(),
            edge: "left".into(),
            monitor: String::new(),
            offset: 0.5,
            staging_folder: "C:\\a".into(),
            opacity: 0.85,
            material: "acrylic".into(),
            panel_width: 380,
            hover_delay_ms: 120,
            drop_action: "ask".into(),
            enabled: true,
        };
        upsert_pod(&c, &pod, "D", "0.4.0").unwrap();
        assert_eq!(load(&c, "D", "0.4.0").unwrap().pods.len(), 1);
        delete_pod(&c, 1, "D", "0.4.0").unwrap();
        assert!(load(&c, "D", "0.4.0").unwrap().pods.is_empty());
    }

    #[test]
    fn pod_deserializes_without_id() {
        // 前端创建匣时不携带 id（由后端分配），缺省字段应成功反序列化
        let v = serde_json::json!({
            "name": "我的匣",
            "edge": "right",
            "stagingFolder": "D:\\暂存",
        });
        let pod: Pod = serde_json::from_value(v).unwrap();
        assert_eq!(pod.id, 0);
        assert_eq!(pod.name, "我的匣");
        assert_eq!(pod.edge, "right");
        assert_eq!(pod.staging_folder, "D:\\暂存");
        assert_eq!(pod.offset, 0.5); // 来自 Default
        assert!(pod.enabled);
    }
}
