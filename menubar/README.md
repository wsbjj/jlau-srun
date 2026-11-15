# JLAU 校园网 · 菜单栏/托盘客户端（Rust，三端）

常驻菜单栏/托盘的校园网小工具：检测外网连通性，断网自动执行深澜 SRun 认证。
菜单栏显示一个「校」字，点开可看状态、改设置、手动登录/下线。

- macOS 无 Dock 图标（`LSUIElement` / Accessory 策略）
- 协议层纯 Rust 实现，与门户 JS 逐字节一致（有单元测试）
- 设置里直接改「检测间隔」，写入配置文件后立即生效，无需重启

## 三端支持

| 平台 | 托盘 | 打包产物 | 系统弹窗 | 凭据存储 |
|---|---|---|---|---|
| macOS (arm64/x86_64) | NSStatusItem | `.app` + `.dmg` | `osascript` | 钥匙串 |
| Windows (x64) | 通知区域 | `.exe` | PowerShell InputBox | 需写进 config.json |
| Linux (x64) | AppIndicator | `.tar.gz` | `zenity` / `kdialog` | `secret-tool` |

## 编译

```bash
cd menubar
cargo build --release
cargo test            # 加密链路与浏览器对拍
cargo clippy -- -D warnings
```

最低 Rust 1.88（edition 2024 + let-chains）。

### 打包

```bash
# macOS：生成 dist/JLAU校园网.app 与 dist/JLAU校园网-<版本>.dmg
bash deploy/make_dmg.sh

# macOS：只生成 .app
bash deploy/make_app.sh

# Linux：手动打包（或直接用 CI 产物）。文件名带 Cargo.toml 版本。
cargo build --release
version="$(sed -n 's/^version = "\([^"]*\)".*/\1/p' Cargo.toml | head -n 1)"
mkdir -p pkg && cp target/release/jlau-srun-menubar pkg/
tar -czf "JLAU校园网-${version}-linux-x64.tar.gz" -C pkg .
```

Windows 用 `cargo build --release` 后取 `target\release\jlau-srun-menubar.exe`。

### dmg 首次打开

未做公证的 app 会被 Gatekeeper 拦一次，任选其一：

```bash
xattr -dr com.apple.quarantine "/Applications/JLAU校园网.app"   # 终端
```
或右键 → 打开 → 再点"打开"。

## CI（GitHub Actions）

`.github/workflows/build.yml`：

- 三个 job 矩阵：`macos-14`（arm64）、`windows-latest`（x64）、`ubuntu-22.04`（x64）
- 每个平台跑 `cargo clippy -D warnings` + `cargo test --release` + `cargo build --release`
- 产物：`.dmg` / `.exe` / `.tar.gz`，上传为 artifact
- 打 `v*` tag 时自动汇总成一个 GitHub Release

Linux job 会先装系统依赖（`libgtk-3-dev`、`libayatana-appindicator3-dev`、`librsvg2-dev`、`libxkbcommon-dev`），
这是 `tray-icon` + `muda` 的 AppIndicator 后端需要的。

发布：

```bash
git tag v1.0.0 && git push origin v1.0.0
```

## 配置

自动查找（先命中先用）：`$JLAU_CONFIG` → `~/Library/Application Support/JLAUSrun/config.json`
→ `~/.config/jlau-srun/config.json` → `%APPDATA%\JLAUSrun\config.json` → 当前目录 / 上一级的 `config.json`。

```json
{
  "portal": "http://202.198.1.80",
  "ac_id": "4",
  "user": "20261794",
  "password": "keychain:jlau-srun",
  "interval": 30,
  "hmd5_mode": "hmac",
  "auto_login": true
}
```

- `password` 写 `keychain:服务名` 时从系统凭据读取（macOS 钥匙串 / Linux secret-tool）
- `interval` 检测间隔秒数，范围 5–3600（菜单里可改）
- `auto_login` 是否断网自动登录
- `hmd5_mode`：`hmac`（默认）或 `concat`（个别学校变体）

存密码：

```bash
jlau-srun-menubar save-password jlau-srun
```

## 菜单

```
在线 · 20261794
   在线
   套餐 20元包月-学生
   会话 706 分钟 · ↑55.2MB ↓98.3MB
──────────────
立即登录 / 立即检测 / 下线
──────────────
设置 ▸
    检测间隔（30 秒） ▸  10 秒 / 30 秒 / 60 秒 / 120 秒 / 300 秒 / 自定义…
    断网自动登录 ✓
打开配置文件 / 打开日志
──────────────
退出
```

改「检测间隔」会直接写回 config.json，并让监控线程立刻按新间隔轮询。

## 命令行

```bash
jlau-srun-menubar check                  # 门户/账号/间隔/外网/会话/流量
jlau-srun-menubar login                  # 立刻登录一次
jlau-srun-menubar set-interval 45        # 改检测间隔并写盘
jlau-srun-menubar params                 # 打印 token/hmd5/info/chksum，可与 Python 版对拍
jlau-srun-menubar save-password jlau-srun
```

> Windows 的 release 版以 GUI 子系统启动（双击不出现黑框），CLI 输出请用 debug 构建查看。

## 实现要点

- **监控线程**（`monitor.rs`）：`interval` 秒一轮，多目标 204 探测；识别"返回 200 但其实是门户登录页"的劫持。
- **自动登录**：断网 → challenge → 计算 → `srun_portal`；`no_response_data_error` 时回查
  `rad_user_info` 判为在线，避免误报。
- **间隔热更新**：每轮循环重新从 `cfg.interval` 计算等待时间，改完立即生效。
- **跨线程**：菜单事件经 `muda::MenuEvent` → `EventLoopProxy`；状态经 `mpsc` 转发到 winit 事件循环，
  不在工作线程碰 UI。
- **平台抽象**（`platform.rs`）：主目录、打开文件、数值输入框、日志路径、凭据读写各自落地。
- **零图片资源**：App 图标由 `tools/make_icon.swift` 用 AppKit 渲染「校」字生成 icns；
  菜单栏直接用系统文字「校」。

## 协议

与上级目录 Python 版相同（详见 `../README.md`）：

```
hmd5   = HMAC-MD5(key=token, msg=password)
info   = "{SRBX1}" + srun_b64(XXTEA(JSON, token))
chksum = SHA1(token+username+token+hmd5+token+ac_id+token+ip+token+200+token+1+token+info)
```

`cargo test` 钉住了两个最容易错的地方：HMAC 语义、info 密文逐字节一致。

## 文件

```
src/main.rs       事件循环 / 菜单栏 / 菜单与设置 / CLI
src/monitor.rs    后台线程：检测 + 自动登录 + 命令处理
src/srun.rs       深澜接口（challenge / login / logout / rad_user_info）
src/crypt.rs      XXTEA / 深澜 base64 / HMAC-MD5 / SHA1（含测试）
src/config.rs     配置定位、读写、凭据解析
src/platform.rs   平台差异：主目录 / 打开文件 / 输入框 / 日志 / 凭据
tools/            「校」字图标渲染（Swift → icns）
deploy/           make_app.sh / make_dmg.sh / LaunchAgent
```

## 已知限制

- 未做代码公证，macOS 首次打开需去隔离属性（见上）。
- Windows 端不支持写入系统凭据，密码需写在 config.json。
- Linux 托盘依赖 AppIndicator，没有该服务的桌面环境（如纯 i3 无 tray）下不会显示。
- 校园网已在线时重复登录不会踢掉现有会话，但"下线"会。
