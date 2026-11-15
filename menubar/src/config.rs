//! 配置读取 / 钥匙串密码

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::platform;

fn default_portal() -> String {
    "http://202.198.1.80".into()
}
fn default_ac_id() -> String {
    "4".into()
}
fn default_interval() -> u64 {
    30
}
fn default_true() -> bool {
    true
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Config {
    #[serde(default = "default_portal")]
    pub portal: String,
    #[serde(default = "default_ac_id")]
    pub ac_id: String,
    #[serde(default)]
    pub user: String,
    /// 明文密码，或 "keychain:服务名"
    #[serde(default)]
    pub password: String,
    /// 断网检测间隔（秒）
    #[serde(default = "default_interval")]
    pub interval: u64,
    /// "hmac"（默认）或 "concat"
    #[serde(default)]
    pub hmd5_mode: String,
    #[serde(default = "default_true")]
    pub auto_login: bool,
}

impl Default for Config {
    fn default() -> Self {
        serde_json::from_str("{}").unwrap()
    }
}

impl Config {
    pub fn compat_md5(&self) -> bool {
        self.hmd5_mode.eq_ignore_ascii_case("concat")
    }

    /// 依次尝试若干候选路径
    pub fn locate() -> Option<PathBuf> {
        let mut candidates: Vec<PathBuf> = Vec::new();
        if let Ok(p) = std::env::var("JLAU_CONFIG") {
            candidates.push(PathBuf::from(p));
        }
        if let Some(home) = platform::home_dir() {
            candidates.push(home.join("Library/Application Support/JLAUSrun/config.json"));
            candidates.push(home.join(".config/jlau-srun/config.json"));
            candidates.push(home.join(".config/JLAUSrun/config.json"));
        }
        if let Some(appdata) = std::env::var_os("APPDATA") {
            candidates.push(PathBuf::from(appdata).join("JLAUSrun/config.json"));
        }
        if let Ok(cwd) = std::env::current_dir() {
            candidates.push(cwd.join("config.json"));
            candidates.push(cwd.join("../config.json"));
        }
        if let Ok(exe) = std::env::current_exe()
            && let Some(dir) = exe.parent()
        {
            candidates.push(dir.join("config.json"));
            candidates.push(dir.join("../../../config.json"));
        }
        candidates.into_iter().find(|p| p.exists())
    }

    pub fn load(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("读取 {} 失败: {e}", path.display()))?;
        serde_json::from_str(&text).map_err(|e| format!("解析 {} 失败: {e}", path.display()))
    }

    /// 写回配置文件（保留其余字段，2 空格缩进）
    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(dir) = path.parent()
            && !dir.as_os_str().is_empty()
        {
            std::fs::create_dir_all(dir).map_err(|e| format!("创建目录失败: {e}"))?;
        }
        let text = serde_json::to_string_pretty(self).map_err(|e| format!("序列化失败: {e}"))?;
        std::fs::write(path, format!("{text}\n")).map_err(|e| format!("写入失败: {e}"))
    }

    /// 没有配置文件时，返回默认落盘位置
    pub fn default_path() -> PathBuf {
        #[cfg(target_os = "macos")]
        {
            if let Some(h) = platform::home_dir() {
                return h.join("Library/Application Support/JLAUSrun/config.json");
            }
        }
        #[cfg(target_os = "windows")]
        {
            if let Some(d) = std::env::var_os("APPDATA") {
                return PathBuf::from(d).join("JLAUSrun/config.json");
            }
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            if let Some(h) = platform::home_dir() {
                return h.join(".config/jlau-srun/config.json");
            }
        }
        PathBuf::from("config.json")
    }

    pub fn load_or_default() -> (Self, Option<PathBuf>) {
        match Self::locate() {
            Some(p) => match Self::load(&p) {
                Ok(cfg) => (cfg, Some(p)),
                Err(e) => {
                    eprintln!("[config] {e}");
                    (Self::default(), Some(p))
                }
            },
            None => (Self::default(), None),
        }
    }

    /// 解析密码：支持 "keychain:服务名"
    pub fn resolve_password(&self) -> Option<String> {
        if self.password.is_empty() {
            return None;
        }
        if let Some(svc) = self.password.strip_prefix("keychain:") {
            return keychain_read(svc, &self.user);
        }
        Some(self.password.clone())
    }
}

/// 读系统凭据（macOS 钥匙串 / Linux secret-tool）
pub fn keychain_read(service: &str, account: &str) -> Option<String> {
    platform::credential_read(service, account)
}

/// 写系统凭据
pub fn keychain_save(service: &str, account: &str, password: &str) -> Result<(), String> {
    platform::credential_save(service, account, password)
}
