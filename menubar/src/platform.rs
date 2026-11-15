//! 平台相关实现：主目录、打开文件、数值输入框、日志路径、凭据存取。
//!
//! 三端（macOS / Windows / Linux）各自走原生方式，其余平台降级为可用即可。

#![allow(unused_variables)]

use std::path::{Path, PathBuf};
use std::process::Command;

// ---------------------------------------------------------------------------
// 主目录
// ---------------------------------------------------------------------------
pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

// ---------------------------------------------------------------------------
// 用系统默认程序打开文本文件
// ---------------------------------------------------------------------------
pub fn open_text_file(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let status = Command::new("open").arg("-t").arg(path).status().map_err(|e| e.to_string())?;
        if !status.success() { return Err(format!("系统打开文件失败: {status}")); }
    }
    #[cfg(target_os = "windows")]
    {
        Command::new("notepad").arg(path).spawn().map_err(|e| e.to_string())?;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if Command::new("xdg-open").arg(path).spawn().is_err() {
            Command::new("gedit").arg(path).spawn().map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// 数值输入框（自定义检测间隔）
// ---------------------------------------------------------------------------
#[cfg(target_os = "macos")]
pub fn prompt_number(title: &str, prompt: &str, default: u64) -> Option<u64> {
    let script = format!(
        "text returned of (display dialog \"{prompt}\" default answer \"{default}\" \
         with title \"{title}\" buttons {{\"取消\", \"保存\"}} default button \"保存\")"
    );
    let out = Command::new("osascript").arg("-e").arg(script).output().ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8_lossy(&out.stdout).trim().parse().ok()
}

#[cfg(target_os = "windows")]
pub fn prompt_number(title: &str, prompt: &str, default: u64) -> Option<u64> {
    let script = format!(
        "Add-Type -AssemblyName Microsoft.VisualBasic; \
         $v=[Microsoft.VisualBasic.Interaction]::InputBox('{prompt}','{title}','{default}'); \
         if($v){{Write-Output $v}}"
    );
    let out = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8_lossy(&out.stdout).trim().parse().ok()
}

#[cfg(all(unix, not(target_os = "macos")))]
pub fn prompt_number(title: &str, prompt: &str, default: u64) -> Option<u64> {
    let def = default.to_string();

    // zenity（GNOME）
    if let Ok(out) = Command::new("zenity")
        .args([
            "--entry",
            "--title",
            title,
            "--text",
            prompt,
            "--entry-text",
            &def,
        ])
        .output()
        && out.status.success()
        && let Ok(v) = String::from_utf8_lossy(&out.stdout).trim().parse()
    {
        return Some(v);
    }

    // kdialog（KDE）
    if let Ok(out) = Command::new("kdialog")
        .args(["--title", title, "--inputbox", prompt, &def])
        .output()
        && out.status.success()
        && let Ok(v) = String::from_utf8_lossy(&out.stdout).trim().parse()
    {
        return Some(v);
    }

    eprintln!("需要 zenity 或 kdialog 才能弹输入框；也可直接改配置文件或用 set-interval 命令");
    None
}

// ---------------------------------------------------------------------------
// 日志路径
// ---------------------------------------------------------------------------
pub fn log_path() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        if let Some(h) = home_dir() {
            return h.join("Library/Logs/jlau-srun.log");
        }
    }
    #[cfg(target_os = "windows")]
    {
        if let Some(d) = std::env::var_os("LOCALAPPDATA") {
            return PathBuf::from(d).join("JLAUSrun/logs/jlau-srun.log");
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Some(h) = home_dir() {
            return h.join(".local/state/jlau-srun/jlau-srun.log");
        }
    }
    PathBuf::from("jlau-srun.log")
}

// ---------------------------------------------------------------------------
// 打开链接、提示更新结果
// ---------------------------------------------------------------------------
pub fn open_url(url: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let status = Command::new("open").arg(url).status().map_err(|e| e.to_string())?;
        if !status.success() {
            return Err(format!("打开链接失败: {status}"));
        }
    }
    #[cfg(target_os = "windows")]
    {
        Command::new("cmd")
            .args(["/C", "start", "", url])
            .spawn()
            .map_err(|e| format!("打开链接失败: {e}"))?;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        Command::new("xdg-open")
            .arg(url)
            .spawn()
            .map_err(|e| format!("打开链接失败: {e}"))?;
    }
    Ok(())
}

pub fn alert(title: &str, message: &str) {
    let _ = dialog(title, message, false);
}

/// 用户选择确认时返回 true。没有图形对话框时返回 false。
pub fn confirm(title: &str, message: &str) -> bool {
    dialog(title, message, true).unwrap_or(false)
}

fn dialog(title: &str, message: &str, ask: bool) -> Option<bool> {
    #[cfg(target_os = "macos")]
    {
        return macos_dialog(title, message, ask);
    }
    #[cfg(target_os = "windows")]
    {
        return windows_dialog(title, message, ask);
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        return linux_dialog(title, message, ask);
    }
    #[allow(unreachable_code)]
    {
        let _ = (title, message, ask);
        None
    }
}

#[cfg(target_os = "macos")]
fn macos_dialog(title: &str, message: &str, ask: bool) -> Option<bool> {
    let title = apple_escape(title);
    let message = apple_escape(message);
    let buttons = if ask {
        "buttons {\"稍后\", \"前往下载\"} default button \"前往下载\""
    } else {
        "buttons {\"好\"} default button \"好\""
    };
    let script = format!("button returned of (display dialog \"{message}\" with title \"{title}\" {buttons})");
    let out = Command::new("osascript").arg("-e").arg(script).output().ok()?;
    if !out.status.success() {
        return Some(false);
    }
    let text = String::from_utf8_lossy(&out.stdout);
    Some(!ask || text.contains("前往下载"))
}

#[cfg(target_os = "macos")]
fn apple_escape(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(target_os = "windows")]
fn windows_dialog(title: &str, message: &str, ask: bool) -> Option<bool> {
    let title = title.replace('\'', "''");
    let message = message.replace('\'', "''");
    let buttons = if ask { "YesNo" } else { "OK" };
    let script = format!(
        "Add-Type -AssemblyName System.Windows.Forms; \
         $r=[System.Windows.Forms.MessageBox]::Show('{message}','{title}','{buttons}','Information'); \
         if($r -eq 'Yes'){{'yes'}}else{{'no'}}"
    );
    let out = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(!ask || String::from_utf8_lossy(&out.stdout).contains("yes"))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn linux_dialog(title: &str, message: &str, ask: bool) -> Option<bool> {
    let kind = if ask { "--question" } else { "--info" };
    if let Ok(status) = Command::new("zenity")
        .args([kind, "--title", title, "--text", message])
        .status()
    {
        return Some(!ask || status.success());
    }
    let kind = if ask { "--yesno" } else { "--msgbox" };
    if let Ok(status) = Command::new("kdialog").args(["--title", title, kind, message]).status() {
        return Some(!ask || status.success());
    }
    None
}

/// 追加不含账号、密码或认证参数的运行状态。
pub fn log_status(message: &str) -> std::io::Result<()> {
    use std::io::Write;

    let path = log_path();
    if let Some(dir) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)?;
    }
    let mut file = std::fs::OpenOptions::new().create(true).append(true).open(path)?;
    let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
    writeln!(file, "[{timestamp}] {message}")
}

// ---------------------------------------------------------------------------
// 凭据（"keychain:服务名" 语法）
// ---------------------------------------------------------------------------
#[cfg(target_os = "macos")]
pub fn credential_read(service: &str, account: &str) -> Option<String> {
    let mut cmd = Command::new("security");
    cmd.args(["find-generic-password", "-s", service, "-w"]);
    if !account.is_empty() {
        cmd.args(["-a", account]);
    }
    let out = cmd.output().ok()?;
    if !out.status.success() {
        return None;
    }
    let pwd = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if pwd.is_empty() { None } else { Some(pwd) }
}

#[cfg(target_os = "macos")]
pub fn credential_save(service: &str, account: &str, password: &str) -> Result<(), String> {
    let _ = Command::new("security")
        .args(["delete-generic-password", "-s", service, "-a", account])
        .output();
    let out = Command::new("security")
        .args([
            "add-generic-password",
            "-s",
            service,
            "-a",
            account,
            "-w",
            password,
            "-U",
        ])
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

#[cfg(target_os = "linux")]
pub fn credential_read(service: &str, account: &str) -> Option<String> {
    // libsecret / GNOME Keyring
    let out = Command::new("secret-tool")
        .args(["lookup", "service", service, "account", account])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let pwd = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if pwd.is_empty() { None } else { Some(pwd) }
}

#[cfg(target_os = "linux")]
pub fn credential_save(service: &str, account: &str, password: &str) -> Result<(), String> {
    use std::io::Write;
    let mut child = Command::new("secret-tool")
        .args([
            "store",
            "--label",
            service,
            "service",
            service,
            "account",
            account,
        ])
        .stdin(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("secret-tool 不可用: {e}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(password.as_bytes())
            .map_err(|e| e.to_string())?;
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err("secret-tool 返回失败".into())
    }
}

#[cfg(target_os = "windows")]
pub fn credential_read(service: &str, account: &str) -> Option<String> {
    // Windows 凭据管理器：cmdkey 只能存不能读，这里不做支持
    let _ = (service, account);
    None
}

#[cfg(target_os = "windows")]
pub fn credential_save(service: &str, account: &str, password: &str) -> Result<(), String> {
    let _ = (service, account, password);
    Err("Windows 端暂不支持写入系统凭据，请把密码写进 config.json 或用环境变量 SRUN_PASS".into())
}
