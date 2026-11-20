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
mod selfservice;
mod srun;
mod update;

use std::io::Write;
use std::path::PathBuf;
use std::sync::mpsc::{self, Sender};
use std::time::Instant;

use muda::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{TrayIcon, TrayIconBuilder};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
use winit::window::WindowId;

use config::Config;
use monitor::{Command, Status, Update};
use selfservice::Device;
use srun::{internet_ok, OnlineInfo, SrunClient};

const BADGE: &str = "校";

const ID_LOGIN: &str = "login";
const ID_CHECK: &str = "check";
const ID_LOGOUT: &str = "logout";
const ID_CONFIG: &str = "config";
const ID_LOG: &str = "log";
const ID_UPDATE: &str = "update";
const ID_QUIT: &str = "quit";
const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
const ID_TOGGLE_AUTO: &str = "toggle_auto";
const ID_DEVICES_REFRESH: &str = "devices_refresh";
const ID_SELF_SERVICE: &str = "self_service";
const ID_INTERVAL_CUSTOM: &str = "interval_custom";
const INTERVAL_PREFIX: &str = "interval_";
const PRESETS: [u64; 5] = [10, 30, 60, 120, 300];
/// 在线设备列表的自动刷新间隔
const DEVICES_TTL: std::time::Duration = std::time::Duration::from_secs(300);

// ---------------------------------------------------------------------------
// 事件
// ---------------------------------------------------------------------------
#[derive(Debug, Clone)]
enum UserEvent {
    // Update 含配置和在线信息，直接放进枚举会把每个事件都撑到数百字节。
    Update(Box<Update>),
    Menu(String),
    UpdateChecked(Box<update::UpdateReport>),
    Devices(Result<Vec<Device>, String>),
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
    proxy: EventLoopProxy<UserEvent>,
    /// 已发现、比当前安装更新的版本号。
    newer_version: Option<String>,
    checking_update: bool,
    /// 自助服务「在线信息」里的设备列表
    devices: Option<Vec<Device>>,
    devices_error: Option<String>,
    /// 上次发起设备查询的时间，用来限流
    devices_attempt: Option<Instant>,
    fetching_devices: bool,
    /// 上次渲染的指纹，避免菜单无谓重建
    rendered: String,
}

impl App {
    fn new(cfg: Config, cfg_path: Option<PathBuf>, cmd_tx: Sender<Command>, proxy: EventLoopProxy<UserEvent>) -> Self {
        Self {
            tray: None,
            cfg,
            cfg_path,
            status: Status::Checking,
            detail: "启动中…".into(),
            info: None,
            last_check: None,
            cmd_tx,
            proxy,
            newer_version: None,
            checking_update: false,
            devices: None,
            devices_error: None,
            devices_attempt: None,
            fetching_devices: false,
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
        format!("JLAU 校园网 {APP_VERSION} · {}\n{}", self.status_line(), self.detail)
    }

    /// 「在线设备」子菜单：标题带台数，每行是 IP · 上线时间 · MAC。
    fn build_devices_menu(&self) -> Submenu {
        let title = match (&self.devices, &self.devices_error) {
            (Some(list), _) => format!("在线设备（{} 台）", list.len()),
            (None, Some(_)) => "在线设备（获取失败）".to_string(),
            (None, None) => "在线设备（获取中…）".to_string(),
        };
        let submenu = Submenu::with_id("devices", title, true);
        match &self.devices {
            Some(list) if list.is_empty() => {
                let _ = submenu.append(&MenuItem::new("当前没有在线设备", false, None));
            }
            Some(list) => {
                for device in list {
                    let _ = submenu.append(&MenuItem::new(device.summary(), false, None));
                }
            }
            None => {
                if let Some(err) = &self.devices_error {
                    let _ = submenu.append(&MenuItem::new(format!("  {err}"), false, None));
                }
            }
        }
        let _ = submenu.append(&PredefinedMenuItem::separator());
        let label = if self.fetching_devices { "正在刷新…" } else { "刷新设备" };
        let refresh = MenuItem::with_id(ID_DEVICES_REFRESH, label, !self.fetching_devices, None);
        let _ = submenu.append(&refresh);
        submenu
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

        let _ = menu.append(&self.build_devices_menu());

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
        let self_service = MenuItem::with_id(ID_SELF_SERVICE, "自助服务平台", true, None);
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&self_service);
        let _ = menu.append_items(&[&cfg_item, &log_item]);

        let _ = menu.append(&PredefinedMenuItem::separator());
        let version_label = match &self.newer_version {
            Some(version) => format!("版本 {APP_VERSION} · 可更新到 {version}"),
            None => format!("版本 {APP_VERSION}"),
        };
        let version = MenuItem::new(version_label, false, None);
        let update_label = if self.checking_update { "正在检查更新…" } else { "检查更新" };
        let update = MenuItem::with_id(ID_UPDATE, update_label, !self.checking_update, None);
        let _ = menu.append_items(&[&version, &update]);

        let _ = menu.append(&PredefinedMenuItem::separator());
        let quit = MenuItem::with_id(ID_QUIT, "退出", true, None);
        let _ = menu.append(&quit);

        menu
    }

    fn refresh(&mut self, force: bool) {
        let devices_key = match &self.devices {
            Some(list) => list
                .iter()
                .map(Device::summary)
                .collect::<Vec<_>>()
                .join(","),
            None => format!("err:{}", self.devices_error.as_deref().unwrap_or("")),
        };
        let fingerprint = format!(
            "{:?}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
            self.status,
            self.detail,
            self.cfg.user,
            self.cfg.interval,
            self.cfg.auto_login,
            self.newer_version.as_deref().unwrap_or(""),
            self.checking_update,
            self.fetching_devices,
            devices_key,
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

    /// 查一次自助服务「在线信息」。`force=false` 时按 TTL 限流。
    fn refresh_devices(&mut self, force: bool) {
        if self.fetching_devices {
            return;
        }
        if !force
            && let Some(at) = self.devices_attempt
            && at.elapsed() < DEVICES_TTL
        {
            return;
        }
        if self.cfg.user.is_empty() {
            self.devices = None;
            self.devices_error = Some("未配置账号".into());
            self.devices_attempt = Some(Instant::now());
            self.refresh(true);
            return;
        }
        let Some(password) = self.cfg.resolve_password() else {
            self.devices = None;
            self.devices_error = Some("未配置密码".into());
            self.devices_attempt = Some(Instant::now());
            self.refresh(true);
            return;
        };

        let base = self.cfg.self_service_base();
        let user = self.cfg.user.clone();
        let proxy = self.proxy.clone();
        self.fetching_devices = true;
        self.devices_attempt = Some(Instant::now());
        self.refresh(true);

        let spawned = std::thread::Builder::new()
            .name("jlau-devices".into())
            .spawn(move || {
                let result = selfservice::fetch_devices(&base, &user, &password);
                let _ = proxy.send_event(UserEvent::Devices(result));
            });
        if spawned.is_err() {
            self.fetching_devices = false;
            self.devices_error = Some("无法启动设备查询线程".into());
            self.refresh(true);
        }
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
            ID_DEVICES_REFRESH => self.refresh_devices(true),
            ID_SELF_SERVICE => {
                let base = self.cfg.self_service_base();
                // 带 SSO 参数打开浏览器，省掉一次验证码登录；没密码就退回首页
                let url = match self.cfg.resolve_password() {
                    Some(password) if !self.cfg.user.is_empty() => {
                        selfservice::sso_url(&base, &self.cfg.user, &password)
                    }
                    _ => selfservice::home_url(&base),
                };
                match platform::open_url(&url) {
                    Ok(()) => self.detail = "已在浏览器打开自助服务平台".into(),
                    Err(e) => self.detail = format!("打开自助服务平台失败: {e}"),
                }
                self.refresh(true);
            }
            ID_UPDATE => self.check_for_update(),
            ID_QUIT => {
                let _ = self.cmd_tx.send(Command::Quit);
                std::process::exit(0);
            }
            _ => {}
        }
    }

    fn check_for_update(&mut self) {
        if self.checking_update {
            return;
        }
        self.checking_update = true;
        self.detail = "正在检查更新…".into();
        self.refresh(true);
        let proxy = self.proxy.clone();
        if std::thread::Builder::new()
            .name("jlau-update".into())
            .spawn(move || {
                let report = update::check(APP_VERSION);
                let _ = proxy.send_event(UserEvent::UpdateChecked(Box::new(report)));
            })
            .is_err()
        {
            self.checking_update = false;
            self.detail = "无法启动更新检查".into();
            self.refresh(true);
        }
    }

    fn show_update(&mut self, report: update::UpdateReport) {
        self.checking_update = false;
        self.newer_version = if report.newer { report.latest.clone() } else { None };
        self.detail = report.summary.clone();
        let _ = platform::log_status(&report.summary);
        self.refresh(true);
        if report.newer {
            let message = format!("{}\n是否打开下载页面？", report.summary);
            if platform::confirm("JLAU 校园网", &message)
                && let Some(page) = report.page
                && let Err(err) = platform::open_url(&page)
            {
                self.detail = format!("打开下载页面失败: {err}");
                self.refresh(true);
            }
        } else {
            platform::alert("JLAU 校园网", &report.summary);
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
                    self.refresh_devices(true);
                }
                Err(e) => eprintln!("创建菜单栏图标失败: {e}"),
            }
        }
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::Update(u) => {
                let label = match u.status {
                    Status::Online => "在线".to_string(),
                    Status::LoggedIn => "登录成功".to_string(),
                    Status::Offline => "离线".to_string(),
                    Status::Checking => "检测中".to_string(),
                    Status::LoggingIn => "登录中".to_string(),
                    Status::Error => u.detail.clone(),
                    Status::Unconfigured => "未配置账号或密码".to_string(),
                };
                let _ = platform::log_status(&label);
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
                if matches!(self.status, Status::Online | Status::LoggedIn) {
                    self.refresh_devices(false);
                }
            }
            UserEvent::Devices(result) => {
                self.fetching_devices = false;
                match result {
                    Ok(list) => {
                        let _ = platform::log_status(&format!("在线设备 {} 台", list.len()));
                        self.devices = Some(list);
                        self.devices_error = None;
                    }
                    Err(e) => {
                        let _ = platform::log_status(&format!("设备查询失败: {e}"));
                        self.devices = None;
                        self.devices_error = Some(e);
                    }
                }
                self.refresh(true);
            }
            UserEvent::Menu(id) => self.handle_menu(&id),
            UserEvent::UpdateChecked(report) => self.show_update(*report),
        }
    }

    /// winit 每轮事件循环都会经过这里。Linux 上没有独立的 GTK 主循环，
    /// 托盘项注册和菜单点击都靠这里把 GTK/GDBus 事件派发出去，
    /// 顺便把等待超时压到几十毫秒，避免空闲时长时间没人泵。
    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        platform::pump_gui();
        #[cfg(target_os = "linux")]
        {
            const GTK_PUMP: std::time::Duration = std::time::Duration::from_millis(50);
            _event_loop.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(
                Instant::now() + GTK_PUMP,
            ));
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
    if args.iter().any(|arg| arg == "version") {
        println!("{APP_VERSION}");
        return true;
    }
    if args.iter().any(|arg| arg == "check-update") {
        let report = update::check(APP_VERSION);
        println!("{}", report.summary);
        if let Some(page) = report.page {
            println!("{page}");
        }
        return true;
    }

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

    if args.iter().any(|a| a == "devices") {
        let Some(pwd) = cfg.resolve_password() else {
            eprintln!("取不到密码：config.json 的 password 为空，或钥匙串没有该条目");
            return true;
        };
        match selfservice::fetch_devices(&cfg.self_service_base(), &cfg.user, &pwd) {
            Ok(list) => {
                println!("在线设备 {} 台", list.len());
                for d in list {
                    println!("{}  {}  {}", d.ip, d.mac, d.online_at);
                }
            }
            Err(e) => println!("查询失败: {e}"),
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

    // Linux：muda / tray-icon 的 GTK 后端必须在建菜单前由主线程自己 init，
    // 否则 resumed() 里 build_menu() 会 panic「GTK has not been initialized」。
    if let Err(e) = platform::init_gui() {
        eprintln!("{e}");
        let _ = platform::log_status(&e);
        // 没有可用的 GTK/显示时托盘必然建不起来，直接退出，
        // 别让 muda 在 resumed() 里带着 panic 收场。
        std::process::exit(1);
    }

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

    let mut app = App::new(cfg, cfg_path, cmd_tx, proxy);
    if let Err(e) = event_loop.run_app(&mut app) {
        eprintln!("事件循环异常: {e}");
    }
}
