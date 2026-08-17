//! .lnk 快捷方式创建（经 WScript.Shell COM，由 PowerShell 承载）。
//! 选择子进程方案而非直接 COM 绑定：实现短、无额外 crate、批量一条命令完成。

use std::process::Command;

/// 为每个 (目标, 输出.lnk) 创建快捷方式。
pub fn create_shortcuts(pairs: &[(std::path::PathBuf, std::path::PathBuf)]) -> Result<(), String> {
    if pairs.is_empty() {
        return Ok(());
    }
    let mut script = String::from("$ws = New-Object -ComObject WScript.Shell\n");
    for (target, out) in pairs {
        let t = ps_quote(&target.to_string_lossy());
        let o = ps_quote(&out.to_string_lossy());
        script.push_str(&format!(
            "$s = $ws.CreateShortcut({o}); $s.TargetPath = {t}; $s.Save()\n"
        ));
    }
    let mut cmd = Command::new("powershell");
    cmd.arg("-NoProfile").arg("-NonInteractive").arg("-Command").arg(&script);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW，避免闪出控制台
        cmd.creation_flags(0x0800_0000);
    }
    let status = cmd.output().map_err(|e| format!("启动 PowerShell 失败: {e}"))?;
    if !status.status.success() {
        return Err("创建快捷方式失败".to_string());
    }
    Ok(())
}

fn ps_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// 从目标文件名推导快捷方式显示名：`报告.docx` -> `报告 - 快捷方式.lnk`
pub fn shortcut_name_for(file_name: &str) -> String {
    format!("{file_name} - 快捷方式.lnk")
}
