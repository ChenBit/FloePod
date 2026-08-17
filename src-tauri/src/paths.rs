use std::fs;
use std::path::PathBuf;

/// 数据目录解析：便携优先。
/// exe 旁 `FloePodData` 可写则使用（真正的 U 盘便携）；否则回退 %APPDATA%\FloePod。
/// 不依赖 AppHandle，可在 Builder 阶段（窗口创建前）完成状态注册。
pub fn resolve() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let portable = dir.join("FloePodData");
            if ensure_writable(&portable) {
                return portable;
            }
        }
    }
    let base = std::env::var("APPDATA").unwrap_or_default();
    let fallback = PathBuf::from(base).join("FloePod");
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
