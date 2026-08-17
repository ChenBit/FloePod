use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension, Row};

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

pub fn open(dir: &Path) -> Result<Connection, String> {
    let _ = std::fs::create_dir_all(dir);
    let conn = Connection::open(dir.join("data.db")).map_err(|e| e.to_string())?;
    conn.pragma_update(None, "journal_mode", "WAL")
        .map_err(|e| e.to_string())?;
    conn.pragma_update(None, "foreign_keys", "ON")
        .map_err(|e| e.to_string())?;
    migrate(&conn)?;
    Ok(conn)
}

pub fn migrate(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS scenes (
          id INTEGER PRIMARY KEY AUTOINCREMENT,
          name TEXT NOT NULL,
          sort INTEGER NOT NULL DEFAULT 0,
          created_at INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS items (
          id INTEGER PRIMARY KEY AUTOINCREMENT,
          scene_id INTEGER NOT NULL REFERENCES scenes(id) ON DELETE CASCADE,
          kind TEXT NOT NULL,
          staging_path TEXT NOT NULL UNIQUE,
          original_path TEXT,
          name TEXT NOT NULL,
          ext TEXT,
          size INTEGER NOT NULL DEFAULT 0,
          created_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_items_scene ON items(scene_id);
        CREATE TABLE IF NOT EXISTS settings (
          key TEXT PRIMARY KEY,
          value TEXT NOT NULL
        );
        "#,
    )
    .map_err(|e| e.to_string())
}

pub fn ensure_default_scene(conn: &Connection) -> Result<(), String> {
    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM scenes", [], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    if n == 0 {
        conn.execute(
            "INSERT INTO scenes (name, sort, created_at) VALUES (?1, 0, ?2)",
            params!["默认", now_ms()],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/* ---------- 类型 ---------- */

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StagedItem {
    pub id: i64,
    pub scene_id: i64,
    pub kind: String,
    pub staging_path: String,
    pub original_path: Option<String>,
    pub name: String,
    pub ext: Option<String>,
    pub size: i64,
    pub created_at: i64,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Scene {
    pub id: i64,
    pub name: String,
    pub sort: i64,
    pub created_at: i64,
}

fn item_from_row(row: &Row) -> rusqlite::Result<StagedItem> {
    Ok(StagedItem {
        id: row.get(0)?,
        scene_id: row.get(1)?,
        kind: row.get(2)?,
        staging_path: row.get(3)?,
        original_path: row.get(4)?,
        name: row.get(5)?,
        ext: row.get(6)?,
        size: row.get(7)?,
        created_at: row.get(8)?,
    })
}

/* ---------- items ---------- */

pub fn insert_item(conn: &Connection, it: &StagedItem) -> Result<StagedItem, String> {
    conn.execute(
        "INSERT OR IGNORE INTO items (scene_id, kind, staging_path, original_path, name, ext, size, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            it.scene_id,
            it.kind,
            it.staging_path,
            it.original_path,
            it.name,
            it.ext,
            it.size,
            it.created_at
        ],
    )
    .map_err(|e| e.to_string())?;
    find_by_path(conn, &it.staging_path)?.ok_or_else(|| "插入后未找到记录".to_string())
}

pub fn find_by_path(conn: &Connection, path: &str) -> Result<Option<StagedItem>, String> {
    conn.query_row(
        "SELECT id, scene_id, kind, staging_path, original_path, name, ext, size, created_at
         FROM items WHERE staging_path = ?1",
        params![path],
        item_from_row,
    )
    .optional()
    .map_err(|e| e.to_string())
}

pub fn list_items(conn: &Connection) -> Result<Vec<StagedItem>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, scene_id, kind, staging_path, original_path, name, ext, size, created_at
             FROM items ORDER BY created_at DESC, id DESC",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], item_from_row)
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(rows)
}

pub fn items_by_ids(conn: &Connection, ids: &[i64]) -> Result<Vec<StagedItem>, String> {
    let mut out = Vec::new();
    for id in ids {
        let found: Option<StagedItem> = conn
            .query_row(
                "SELECT id, scene_id, kind, staging_path, original_path, name, ext, size, created_at
                 FROM items WHERE id = ?1",
                params![id],
                item_from_row,
            )
            .optional()
            .map_err(|e| e.to_string())?;
        if let Some(i) = found {
            out.push(i);
        }
    }
    Ok(out)
}

pub fn items_of_scene(conn: &Connection, scene_id: i64) -> Result<Vec<StagedItem>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, scene_id, kind, staging_path, original_path, name, ext, size, created_at
             FROM items WHERE scene_id = ?1 ORDER BY created_at DESC, id DESC",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![scene_id], item_from_row)
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(rows)
}

pub fn all_staging_paths(conn: &Connection) -> Result<Vec<String>, String> {
    let mut stmt = conn
        .prepare("SELECT staging_path FROM items")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(rows)
}

pub fn delete_items_by_ids(conn: &Connection, ids: &[i64]) -> Result<(), String> {
    for id in ids {
        conn.execute("DELETE FROM items WHERE id = ?1", params![id])
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn delete_items_by_paths(conn: &Connection, paths: &[String]) -> Result<(), String> {
    for p in paths {
        conn.execute("DELETE FROM items WHERE staging_path = ?1", params![p])
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/* ---------- scenes ---------- */

fn scene_from_row(row: &Row) -> rusqlite::Result<Scene> {
    Ok(Scene {
        id: row.get(0)?,
        name: row.get(1)?,
        sort: row.get(2)?,
        created_at: row.get(3)?,
    })
}

const SCENE_COLS: &str = "id, name, sort, created_at";

pub fn list_scenes(conn: &Connection) -> Result<Vec<Scene>, String> {
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {SCENE_COLS} FROM scenes ORDER BY sort ASC, id ASC"
        ))
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], scene_from_row)
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(rows)
}

pub fn create_scene(conn: &Connection, name: &str) -> Result<Scene, String> {
    let max_sort: i64 = conn
        .query_row("SELECT COALESCE(MAX(sort), -1) FROM scenes", [], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO scenes (name, sort, created_at) VALUES (?1, ?2, ?3)",
        params![name, max_sort + 1, now_ms()],
    )
    .map_err(|e| e.to_string())?;
    conn.query_row(
        &format!(
            "SELECT {SCENE_COLS} FROM scenes WHERE id = last_insert_rowid()"
        ),
        [],
        scene_from_row,
    )
    .map_err(|e| e.to_string())
}

pub fn rename_scene(conn: &Connection, id: i64, name: &str) -> Result<(), String> {
    conn.execute("UPDATE scenes SET name = ?2 WHERE id = ?1", params![id, name])
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn delete_scene(conn: &Connection, id: i64) -> Result<Vec<StagedItem>, String> {
    let items = items_of_scene(conn, id)?;
    conn.execute("DELETE FROM scenes WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    Ok(items)
}

pub fn first_scene_id(conn: &Connection) -> Result<Option<i64>, String> {
    let id: Option<i64> = conn
        .query_row(
            "SELECT id FROM scenes ORDER BY sort ASC, id ASC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    Ok(id)
}

/* ---------- settings kv ---------- */

pub fn kv_get(conn: &Connection, key: &str) -> Result<Option<String>, String> {
    conn.query_row("SELECT value FROM settings WHERE key = ?1", params![key], |r| {
        r.get(0)
    })
    .optional()
    .map_err(|e| e.to_string())
}

pub fn kv_set(conn: &Connection, key: &str, value: &str) -> Result<(), String> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/* ---------- tests ---------- */

#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> Connection {
        let mut c = Connection::open_in_memory().unwrap();
        c.pragma_update(None, "foreign_keys", "ON").unwrap();
        migrate(&mut c).unwrap();
        ensure_default_scene(&mut c).unwrap();
        c
    }

    #[test]
    fn default_scene_created_once() {
        let c = conn();
        let scenes = list_scenes(&c).unwrap();
        assert_eq!(scenes.len(), 1);
        assert_eq!(scenes[0].name, "默认");
    }

    #[test]
    fn insert_and_fetch_item() {
        let c = conn();
        let scene = list_scenes(&c).unwrap().remove(0);
        let it = StagedItem {
            id: 0,
            scene_id: scene.id,
            kind: "file".into(),
            staging_path: "C:\\staging\\a.pdf".into(),
            original_path: Some("C:\\orig\\a.pdf".into()),
            name: "a.pdf".into(),
            ext: Some("pdf".into()),
            size: 1024,
            created_at: now_ms(),
        };
        let saved = insert_item(&c, &it).unwrap();
        assert!(saved.id > 0);
        assert_eq!(find_by_path(&c, "C:\\staging\\a.pdf").unwrap().unwrap().name, "a.pdf");
    }

    #[test]
    fn duplicate_path_ignored() {
        let c = conn();
        let scene_id = list_scenes(&c).unwrap()[0].id;
        for _ in 0..2 {
            let _ = insert_item(
                &c,
                &StagedItem {
                    id: 0,
                    scene_id,
                    kind: "file".into(),
                    staging_path: "C:\\dup.txt".into(),
                    original_path: None,
                    name: "dup.txt".into(),
                    ext: Some("txt".into()),
                    size: 1,
                    created_at: now_ms(),
                },
            )
            .unwrap();
        }
        assert_eq!(list_items(&c).unwrap().len(), 1);
    }
}
