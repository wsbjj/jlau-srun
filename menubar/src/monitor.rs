//! 后台监控线程：断网检测 + 自动登录

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use crate::config::Config;
use crate::srun::{internet_ok, OnlineInfo, SrunClient};

#[derive(Debug, Clone)]
pub enum Command {
    CheckNow,
    LoginNow,
    Logout,
    Quit,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Status {
    /// 正在检测
    Checking,
    /// 在线
    Online,
    /// 离线（含原因）
    Offline,
    /// 检测到离线，正在登录
    LoggingIn,
    /// 登录成功
    LoggedIn,
    /// 出错
    Error,
    /// 没配置账号
    Unconfigured,
}

#[derive(Debug, Clone)]
pub struct Update {
    pub status: Status,
    pub detail: String,
    pub info: Option<OnlineInfo>,
    pub checked_at: Option<Instant>,
    pub config_path: Option<PathBuf>,
}

impl Update {
    pub fn new(status: Status, detail: impl Into<String>) -> Self {
        Self {
            status,
            detail: detail.into(),
            info: None,
            checked_at: Some(Instant::now()),
            config_path: None,
        }
    }
}

pub fn spawn(cfg: Config, cfg_path: Option<PathBuf>, tx: Sender<Update>, rx: Receiver<Command>) {
    std::thread::Builder::new()
        .name("jlau-monitor".into())
        .spawn(move || run(cfg, cfg_path, tx, rx))
        .expect("启动监控线程失败");
}

fn run(mut cfg: Config, cfg_path: Option<PathBuf>, tx: Sender<Update>, rx: Receiver<Command>) {
    let mut client = SrunClient::new(&cfg.portal, &cfg.ac_id, 12);
    let mut next_check = Instant::now();

    loop {
        // 间隔每轮重算，菜单里改完立即生效
        let interval = Duration::from_secs(cfg.interval.clamp(5, 3600));
        // 等待到下一次检测，其间响应菜单命令
        let now = Instant::now();
        let wait = next_check.saturating_duration_since(now);

        match rx.recv_timeout(wait) {
            Ok(Command::Quit) => return,
            Ok(Command::CheckNow) => {
                next_check = Instant::now();
                continue;
            }
            Ok(Command::LoginNow) => {
                if !configured(&cfg) {
                    let _ = tx.send(Update::new(Status::Unconfigured, "未配置账号，见 config.json"));
                    next_check = Instant::now() + interval;
                    continue;
                }
                do_login(&client, &cfg, &tx);
                next_check = Instant::now() + interval;
                continue;
            }
            Ok(Command::Logout) => {
                let msg = match client.logout(&cfg.user) {
                    Ok(m) => Update::new(Status::Offline, m),
                    Err(e) => Update::new(Status::Error, format!("下线失败: {e}")),
                };
                let _ = tx.send(msg);
                next_check = Instant::now() + interval;
                continue;
            }
            Err(RecvTimeoutError::Timeout) => { /* 到点检测 */ }
            Err(RecvTimeoutError::Disconnected) => return,
        }

        next_check = Instant::now() + interval;

        if !configured(&cfg) {
            let mut u = Update::new(Status::Unconfigured, "未配置账号，见 config.json");
            u.config_path = cfg_path.clone();
            let _ = tx.send(u);
            continue;
        }

        // 配置热重载：优先用启动时的路径，找不到就重新定位
        let reload_from = cfg_path.clone().filter(|p| p.exists()).or_else(Config::locate);
        if let Some(p) = reload_from
            && let Ok(fresh) = Config::load(&p)
        {
            if fresh.portal != cfg.portal || fresh.ac_id != cfg.ac_id {
                client = SrunClient::new(&fresh.portal, &fresh.ac_id, 12);
            }
            cfg = fresh;
        }

        let _ = tx.send(Update::new(Status::Checking, "正在检测网络…"));

        if internet_ok(5) {
            let info = client.online_info();
            let mut u = Update::new(Status::Online, "在线");
            u.info = info;
            let _ = tx.send(u);
            continue;
        }

        let _ = tx.send(Update::new(Status::Offline, "已断网"));
        if cfg.auto_login {
            do_login(&client, &cfg, &tx);
        }
    }
}

fn configured(cfg: &Config) -> bool {
    !cfg.user.is_empty() && cfg.resolve_password().is_some()
}

fn do_login(client: &SrunClient, cfg: &Config, tx: &Sender<Update>) {
    let _ = tx.send(Update::new(Status::LoggingIn, "正在登录…"));
    let Some(pwd) = cfg.resolve_password() else {
        let _ = tx.send(Update::new(Status::Error, "取不到密码（检查 config.json / 钥匙串）"));
        return;
    };
    match client.login(&cfg.user, &pwd, cfg.compat_md5()) {
        Ok(outcome) => {
            if outcome.ok {
                let mut u = Update::new(Status::LoggedIn, format!("✅ {} · {}", cfg.user, outcome.message));
                u.info = client.online_info();
                let _ = tx.send(u);
            } else {
                let _ = tx.send(Update::new(
                    Status::Error,
                    format!("❌ {} 登录失败: {}", outcome.ip, outcome.describe()),
                ));
            }
        }
        Err(e) => {
            let _ = tx.send(Update::new(Status::Error, format!("❌ 登录失败: {e}")));
        }
    }
}
