// Windows release 下以 GUI 子系统启动，避免双击出现控制台黑框。
// 需要 CLI 输出时请用 debug 构建，或把输出重定向到日志。
#![cfg_attr(all(target_os = "windows", not(debug_assertions)), windows_subsystem = "windows")]

//! JLAU 校园网菜单栏常驻工具
//!
//! 常驻 macOS 菜单栏（文字「校」），监控外网连通性，断网自动执行深澜 SRun 认证。

mod config;
mod crypt;
mod monitor;
mod platform;
mod srun;

use std::io::Write;
use std::path::PathBuf;
use std::sync::mpsc::{self, Sender};
use std::time::Instant;

use muda::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{TrayIcon, TrayIconBuilder};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::WindowId;

use config::Config;
use monitor::{Command, Status, Update};
use srun::{internet_ok, OnlineInfo, SrunClient};

const BADGE: &str = "校";

const ID_LOGIN: &str = "login";
const ID_CHECK: &str = "check";
const ID_LOGOUT: &str = "logout";
const ID_CONFIG: &str = "config";
const ID_LOG: &str = "log";
const ID_QUIT: &str = "quit";
const ID_TOGGLE_AUTO: &str = "toggle_auto";
const ID_INTERVAL_CUSTOM: &str = "interval_custom";
const INTERVAL_PREFIX: &str = "interval_";
const PRESETS: [u64; 5] = [10, 30, 60, 120, 300];

// ---------------------------------------------------------------------------
// 事件
// ---------------------------------------------------------------------------
#[derive(Debug, Clone)]
enum UserEvent {
    // Update 含配置和在线信息，直接放进枚举会把每个事件都撑到数百字节。
    Update(Box<Update>),
    Menu(String),
}

// ---------------------------------------------------------------------------
// 应用状态
// ---------------------------------------------------------------------------
struct App {
    tray: Option<TrayIcon>,
    cfg: Config,
    cfg_path: Option<PathBuf>,
    status: Status,
    detail: String,
    info: Option<OnlineInfo>,
    last_check: Option<Instant>,
    cmd_tx: Sender<Command>,
    /// 上次渲染的指纹，避免菜单无谓重建
    rendered: String,
}

impl App {
    fn new(cfg: Config, cfg_path: Option<PathBuf>, cmd_tx: Sender<Command>) -> Self {
        Self {
            tray: None,
            cfg,
            cfg_path,
            status: Status::Checking,
            detail: "启动中…".into(),
            info: None,
            last_check: None,
            cmd_tx,
            rendered: String::new(),
        }
    }

    fn status_line(&self) -> String {
        let label = match self.status {
            Status::Online => "在线",
            Status::LoggedIn => "已登录",
            Status::Offline => "离线",
            Status::Checking => "检测中",
            Status::LoggingIn => "登录中",
            Status::Error => "异常",
            Status::Unconfigured => "未配置",
        };
        if self.cfg.user.is_empty() {
            label.to_string()
        } else {
            format!("{label} · {}", self.cfg.user)
        }
    }

    fn tooltip(&self) -> String {
        format!("JLAU 校园网 · {}\n{}", self.status_line(), self.detail)
    }

    fn build_menu(&self) -> Menu {
        let menu = Menu::new();

        let _ = menu.append(&MenuItem::new(self.status_line(), false, None));
        let _ = menu.append(&MenuItem::new(format!("   {}", self.detail), false, None));

        if let Some(info) = &self.info
            && info.online
        {
            {
                if !info.product.is_empty() {
                    let _ = menu.append(&MenuItem::new(
                        format!("   套餐 {}", info.product),
                        false,
                        None,
                    ));
                }
                if info.sum_seconds > 0 {
                    let _ = menu.append(&MenuItem::new(
                        format!(
                            "   会话 {} 分钟 · ↑{} ↓{}",
                            info.sum_seconds / 60,
                            human_bytes(info.bytes_out),
                            human_bytes(info.bytes_in)
                        ),
                        false,
                        None,
                    ));
                }
            }
        }

        let _ = menu.append(&PredefinedMenuItem::separator());

        let login = MenuItem::with_id(ID_LOGIN, "立即登录", true, None);
        let check = MenuItem::with_id(ID_CHECK, "立即检测", true, None);
        let logout = MenuItem::with_id(ID_LOGOUT, "下线", true, None);
        let _ = menu.append_items(&[&login, &check, &logout]);

        let _ = menu.append(&PredefinedMenuItem::separator());

        // ---- 设置 ----
        let settings = Submenu::with_id("settings", "设置", true);

        let intervals = Submenu::with_id(
            "settings_interval",
            format!("检测间隔（{} 秒）", self.cfg.interval),
            true,
        );
        for secs in PRESETS {
            let item = CheckMenuItem::with_id(
                format!("{INTERVAL_PREFIX}{secs}"),
                format!("{secs} 秒"),
                true,
                self.cfg.interval == secs,
                None,
            );
            let _ = intervals.append(&item);
        }
        let _ = intervals.append(&PredefinedMenuItem::separator());
        let custom = MenuItem::with_id(ID_INTERVAL_CUSTOM, "自定义…", true, None);
        let _ = intervals.append(&custom);

        let auto = CheckMenuItem::with_id(
            ID_TOGGLE_AUTO,
            "断网自动登录",
            true,
            self.cfg.auto_login,
            None,
        );

        let _ = settings.append(&intervals);
        let _ = settings.append(&auto);
        let _ = menu.append(&settings);

        // ---- 其他 ----
        let cfg_item = MenuItem::with_id(ID_CONFIG, "打开配置文件", true, None);
        let log_item = MenuItem::with_id(ID_LOG, "打开日志", true, None);
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append_items(&[&cfg_item, &log_item]);

        let _ = menu.append(&PredefinedMenuItem::separator());
        let quit = MenuItem::with_id(ID_QUIT, "退出", true, None);
        let _ = menu.append(&quit);

        menu
    }

    fn refresh(&mut self, force: bool) {
        let fingerprint = format!(
            "{:?}|{}|{}|{}|{}|{}",
            self.status,
            self.detail,
            self.cfg.user,
            self.cfg.interval,
            self.cfg.auto_login,
            self.info
                .as_ref()
                .map(|i| format!("{}|{}|{}", i.online, i.sum_seconds, i.product))
                .unwrap_or_default()
        );
        if !force && fingerprint == self.rendered {
            return;
        }
        self.rendered = fingerprint;

        let Some(tray) = &self.tray else { return };
        let _ = tray.set_icon(None);
        tray.set_title(Some(BADGE));
        let _ = tray.set_tooltip(Some(self.tooltip()));
        tray.set_menu(Some(Box::new(self.build_menu())));
    }

    fn persist(&mut self) -> Result<(), String> {
        let path = self
            .cfg_path
            .clone()
            .unwrap_or_else(Config::default_path);
        self.cfg.save(&path)?;
        self.cfg_path = Some(path);
        Ok(())
    }

    fn set_interval(&mut self, secs: u64) {
        let secs = secs.clamp(5, 3600);
        self.cfg.interval = secs;
        match self.persist() {
            Ok(()) => self.detail = format!("检测间隔已设为 {secs} 秒"),
            Err(e) => self.detail = format!("保存失败: {e}"),
        }
        // 让监控线程立刻重读配置
        let _ = self.cmd_tx.send(Command::CheckNow);
        self.refresh(true);
    }

    fn toggle_auto_login(&mut self) {
        self.cfg.auto_login = !self.cfg.auto_login;
        match self.persist() {
            Ok(()) => {
                self.detail = if self.cfg.auto_login {
                    "已开启断网自动登录".into()
                } else {
                    "已关闭断网自动登录".into()
                }
            }
            Err(e) => self.detail = format!("保存失败: {e}"),
        }
        let _ = self.cmd_tx.send(Command::CheckNow);
        self.refresh(true);
    }

    fn handle_menu(&mut self, id: &str) {
        if let Some(rest) = id.strip_prefix(INTERVAL_PREFIX) {
            if rest == "custom" {
                if let Some(secs) = platform::prompt_number("JLAU 校园网", "检测间隔（秒，5–3600）：", self.cfg.interval) {
                    self.set_interval(secs);
                }
            } else if let Ok(secs) = rest.parse::<u64>() {
                self.set_interval(secs);
            }
            return;
        }

        match id {
            ID_LOGIN => {
                let _ = self.cmd_tx.send(Command::LoginNow);
            }
            ID_CHECK => {
                let _ = self.cmd_tx.send(Command::CheckNow);
            }
            ID_LOGOUT => {
                let _ = self.cmd_tx.send(Command::Logout);
            }
            ID_TOGGLE_AUTO => self.toggle_auto_login(),
            ID_CONFIG => {
                let path = self.cfg_path.clone().unwrap_or_else(Config::default_path);
                if !path.exists() {
                    if let Err(e) = self.cfg.save(&path) {
                        self.detail = format!("创建配置失败: {e}");
                        self.refresh(true);
                        return;
                    }
                    self.cfg_path = Some(path.clone());
                }
                if let Err(e) = platform::open_text_file(&path) {
                    self.detail = format!("打开配置失败: {e}");
                    self.refresh(true);
                }
            }
            ID_LOG => {
                if let Err(e) = platform::log_status("打开日志") {
                    self.detail = format!("日志文件不可用: {e}");
                    self.refresh(true);
                    return;
                }
                if let Err(e) = platform::open_text_file(&platform::log_path()) {
                    self.detail = format!("打开日志失败: {e}");
                    self.refresh(true);
                }
            }
            ID_QUIT => {
                let _ = self.cmd_tx.send(Command::Quit);
                std::process::exit(0);
            }
            _ => {}
        }
    }
}

impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {
        if self.tray.is_none() {
            let menu = self.build_menu();
            match TrayIconBuilder::new()
                .with_menu(Box::new(menu))
                .with_tooltip(self.tooltip())
                .with_title(BADGE)
                .with_menu_on_left_click(true)
                .build()
            {
                Ok(tray) => {
                    self.tray = Some(tray);
                    self.refresh(true);
                    let _ = self.cmd_tx.send(Command::CheckNow);
                }
                Err(e) => eprintln!("创建菜单栏图标失败: {e}"),
            }
        }
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::Update(u) => {
                let label = match u.status {
                    Status::Online => "在线",
                    Status::LoggedIn => "登录成功",
                    Status::Offline => "离线",
                    Status::Checking => "检测中",
                    Status::LoggingIn => "登录中",
                    Status::Error => "操作失败，请查看菜单中的提示",
                    Status::Unconfigured => "未配置账号或密码",
                };
                let _ = platform::log_status(label);
                if let Some(cfg) = u.config {
                    self.cfg = cfg;
                }
                if let Some(path) = u.config_path {
                    self.cfg_path = Some(path);
                }
                self.status = u.status;
                self.detail = u.detail;
                self.info = u.info;
                self.last_check = u.checked_at;
                self.refresh(false);
            }
            UserEvent::Menu(id) => self.handle_menu(&id),
        }
    }

    fn window_event(&mut self, _e: &ActiveEventLoop, _id: WindowId, _event: WindowEvent) {}
}

fn human_bytes(n: i64) -> String {
    let n = n.max(0) as f64;
    let units = ["B", "KB", "MB", "GB", "TB"];
    let mut v = n;
    let mut i = 0;
    while v >= 1024.0 && i < units.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    format!("{v:.1}{}", units[i])
}

// ---------------------------------------------------------------------------
// 无界面工具模式
// ---------------------------------------------------------------------------
fn cli_mode(args: &[String]) -> bool {
    let (cfg, path) = Config::load_or_default();
    let client = SrunClient::new(&cfg.portal, &cfg.ac_id, 12);

    if args.iter().any(|a| a == "check") {
        println!(
            "配置: {}",
            path.map(|p| p.display().to_string())
                .unwrap_or_else(|| "未找到".into())
        );
        println!("门户: {} ac_id={}", cfg.portal, cfg.ac_id);
        println!(
            "账号: {}",
            if cfg.user.is_empty() { "(未配置)" } else { &cfg.user }
        );
        println!("间隔: {} 秒 · 自动登录 {}", cfg.interval, cfg.auto_login);
        println!("外网: {}", if internet_ok(5) { "✅ 通" } else { "❌ 不通" });
        if let Some(i) = client.online_info() {
            if i.online {
                println!(
                    "会话: {user} @{ip} · {product} · {min} 分钟",
                    user = i.user_name,
                    ip = i.online_ip,
                    product = i.product,
                    min = i.sum_seconds / 60
                );
                println!(
                    "流量: ↑{} ↓{} · 余额 {} 元",
                    human_bytes(i.bytes_out),
                    human_bytes(i.bytes_in),
                    i.balance
                );
            } else {
                println!("会话: 未登录");
            }
        }
        return true;
    }

    if args.iter().any(|a| a == "login") {
        let Some(pwd) = cfg.resolve_password() else {
            eprintln!("取不到密码：config.json 的 password 为空，或钥匙串没有该条目");
            return true;
        };
        match client.login(&cfg.user, &pwd, cfg.compat_md5()) {
            Ok(o) if o.ok => println!("✅ {}", o.message),
            Ok(o) => println!("❌ {}", o.describe()),
            Err(e) => println!("❌ {e}"),
        }
        return true;
    }

    if let Some(pos) = args.iter().position(|a| a == "set-interval") {
        let Some(raw) = args.get(pos + 1) else {
            eprintln!("用法: set-interval <秒>");
            return true;
        };
        let Ok(secs) = raw.parse::<u64>() else {
            eprintln!("不是合法数字: {raw}");
            return true;
        };
        let mut cfg = cfg;
        cfg.interval = secs.clamp(5, 3600);
        let target = path.unwrap_or_else(Config::default_path);
        match cfg.save(&target) {
            Ok(()) => println!("已设置检测间隔 {} 秒 -> {}", cfg.interval, target.display()),
            Err(e) => println!("失败: {e}"),
        }
        return true;
    }

    if args.iter().any(|a| a == "params") {
        let Some(pwd) = cfg.resolve_password() else {
            eprintln!("取不到密码");
            return true;
        };
        match client.get_challenge(&cfg.user) {
            Ok(ch) => {
                let hmd5 = if cfg.compat_md5() {
                    crypt::md5_concat_hex(&pwd, &ch.token)
                } else {
                    crypt::hmac_md5_hex(&ch.token, &pwd)
                };
                let info = crypt::build_info(&cfg.user, &pwd, &ch.client_ip, &cfg.ac_id, &ch.token);
                let chksum = crypt::build_chksum(
                    &ch.token,
                    &cfg.user,
                    &hmd5,
                    &cfg.ac_id,
                    &ch.client_ip,
                    "200",
                    "1",
                    &info,
                );
                println!("token  {}", ch.token);
                println!("ip     {}", ch.client_ip);
                println!("hmd5   {hmd5}");
                println!("info   {info}");
                println!("chksum {chksum}");
            }
            Err(e) => println!("challenge 失败: {e}"),
        }
        return true;
    }

    if let Some(pos) = args.iter().position(|a| a == "save-password") {
        let svc = args
            .get(pos + 1)
            .cloned()
            .unwrap_or_else(|| "jlau-srun".into());
        let mut buf = String::new();
        eprint!("校园网密码: ");
        let _ = std::io::stderr().flush();
        if std::io::stdin().read_line(&mut buf).is_err() {
            eprintln!("读取密码失败");
            return true;
        }
        let pwd = buf.trim_end_matches(['\n', '\r']).to_string();
        match config::keychain_save(&svc, &cfg.user, &pwd) {
            Ok(()) => println!("已写入钥匙串: service={svc} account={}", cfg.user),
            Err(e) => println!("写入失败: {e}"),
        }
        return true;
    }

    false
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if cli_mode(&args) {
        return;
    }
    let _ = platform::log_status("程序启动");

    let (cfg, cfg_path) = Config::load_or_default();

    #[allow(unused_mut)]
    let mut builder = EventLoop::<UserEvent>::with_user_event();
    #[cfg(target_os = "macos")]
    {
        use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};
        builder.with_activation_policy(ActivationPolicy::Accessory);
    }
    let event_loop = builder.build().expect("创建事件循环失败");

    let proxy = event_loop.create_proxy();

    let menu_proxy = proxy.clone();
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        let _ = menu_proxy.send_event(UserEvent::Menu(event.id().0.clone()));
    }));

    let (update_tx, update_rx) = mpsc::channel::<Update>();
    let fwd_proxy = proxy.clone();
    std::thread::spawn(move || {
        while let Ok(u) = update_rx.recv() {
            if fwd_proxy.send_event(UserEvent::Update(Box::new(u))).is_err() {
                break;
            }
        }
    });

    let (cmd_tx, cmd_rx) = mpsc::channel::<Command>();
    monitor::spawn(cfg.clone(), cfg_path.clone(), update_tx, cmd_rx);

    let mut app = App::new(cfg, cfg_path, cmd_tx);
    if let Err(e) = event_loop.run_app(&mut app) {
        eprintln!("事件循环异常: {e}");
    }
}
