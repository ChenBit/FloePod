use std::fs;
use std::path::PathBuf;

use tauri::AppHandle;

/// 数据目录解析：便携优先。
/// exe 旁 `FloePodData` 可写则使用（真正的 U 盘便携）；否则回退 %APPDATA%\FloePod。
pub fn resolve(app: &AppHandle) -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let portable = dir.join("FloePodData");
            if ensure_writable(&portable) {
                return portable;
            }
        }
    }
    let fallback = app
        .path()
        .app_data_dir()
        .unwrap_or_else(|_| PathBuf::from("FloePodData"));
    let _ = fs::create_dir_all(&fallback);
    fallback
}

fn ensure_writable(dir: &PathBuf) -> bool {
    if fs::create_dir_all(dir).is_err() {
        return false;
    }
    let probe = dir.join(".write-probe");
    match fs::write(&probe, b"ok") {
        Ok(()) => {
            let _ = fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writable_dir_passes_probe() {
        let tmp = tempfile::tempdir().unwrap();
        let d = tmp.path().to_path_buf();
        assert!(ensure_writable(&d));
    }
}
