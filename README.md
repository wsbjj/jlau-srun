# 吉林农业大学校园网 · 深澜 SRun 自动登录

对 `http://202.198.1.80` 的登录接口做了完整逆向，产出零依赖 Python 客户端 + 断网自动重连守护。

- 认证系统：**深澜 Srun**（`Srun-Server: SRunCGIAuthIntfSvr V1.18 B20191227`，门户 `Srun portal server golang version`）
- 反向代理：nginx，门户入口 `302 → /index_3.html → /srun_portal_pc?ac_id=4&theme=wifi`
- `ac_id`：**4**（本校唯一 SSID 分组）

---

## 一、接口清单（全部实测）

| 用途 | 方法与路径 | 关键参数 |
|---|---|---|
| 取挑战串 | `GET /cgi-bin/get_challenge` | `username`、`ip`（可空）、`callback=cb` |
| 登录 | `GET /cgi-bin/srun_portal` | `action=login` + 13 个参数，见下 |
| 登出 | `GET /cgi-bin/srun_portal` | `action=logout&ac_id=4&ip=<ip>&username=<可空>` |
| 在线信息 | `GET /cgi-bin/rad_user_info` | `callback=cb` |
| 门户页 | `GET /srun_portal_pc?ac_id=4&theme=wifi` | 返回带 `ac_id`/`user_ip` 的表单 |

`get_challenge` 实测响应：

```json
cb({"challenge":"887362c9468ab1416b8ce6a2d0967e2083fd637ec2c3c774786cf6c84b5bb9e4",
    "client_ip":"10.7.224.192","ecode":0,"error":"ok","error_msg":"","expire":"60",
    "online_ip":"10.7.224.192","res":"ok",
    "srun_ver":"SRunCGIAuthIntfSvr V1.18 B20191227","st":1794726100})
```

`rad_user_info` 实测响应（已登录时）：

```json
cb({"add_time":1794700679,"all_bytes":7728358583,"bytes_in":3123879759,"bytes_out":4363603077,
    "error":"ok","online_ip":"10.7.224.192","products_name":"20元包月-学生",
    "sum_bytes":2233220525,"sum_seconds":17223,"user_balance":80,
    "user_mac":"06-D4-86-CE-38-B5","user_name":"20261794", ...})
```

登录真实参数（14 个）：

```
action=login
username=20261794
password={MD5}<hmd5>
ac_id=4
ip=10.7.224.192
chksum=<sha1>
info={SRBX1}<...>
n=200
type=1
os=Windows 10
name=Windows
double_stack=0
callback=cb
```

## 二、加密链路（来自 `ref/jquery.srun.portal.js`）

```js
token  = challenge;
hmd5   = md5(password, token);              // js-md5 双参数 = HMAC-MD5(key=token, msg=password)
info   = "{SRBX1}" + base64(xEncode(JSON.stringify({
             username, password, ip, acid, enc_ver:"srun_bx1" }), token));
chksum = sha1(token+username + token+hmd5 + token+ac_id + token+ip
              + token+"200" + token+"1" + token+info);
```

三个坑，都已经在本项目里踩平：

1. **`xEncode` 是 XXTEA 的变体**（`delta=0x9E3779B9`，32 位无符号，`q = 6 + 52/(n+1)`），
   且 `s()` 按 **UTF-16 charCode** 组 32 位整数，不是 UTF-8 字节 —— 含中文的密码/字段
   用 UTF-8 会和服务端对不上。
2. **base64 用的是自定义字母表**，不是标准表：
   `LVoJPiCN2R8G90yg+hmFHuacZ1OWMnrsSTXkYpUq/3dlbfKwv6xztjI7DeBE45QA`
3. **`hmd5` 不是 `md5(password+token)` 拼接**，而是 `HMAC-MD5(key=token, msg=password)`
   （用 node 加载门户 `all.min.js` 的 md5 v2.10.0 实测确认：
   `md5('secret','token') === hmac_md5('token','secret')`）。

### 链路正确性怎么验证的

向 `/cgi-bin/srun_portal` 发两组请求对比：

| chksum | 服务端返回 |
|---|---|
| 乱填 `deadbeef…` | `{"error":"sign_error"}` ← 服务端确实在算 chksum |
| 本项目算出来的 | `{"error":"login_error","error_msg":"no_response_data_error"}` ← 通过校验，进入 AC 认证 |

也就是加密链路、参数顺序、info 密文全部被服务端接受。`no_response_data_error` 出现在
**该 IP 已有在线会话**时（AC 不响应重复认证），脚本会回查 `rad_user_info` 判定为已在线。

另外 `xEncode` 的输出与浏览器 JS 做了逐字节比对（含中文字段），完全一致。

## 三、用法

```bash
cp config.example.json config.json     # 填学号；密码可存钥匙串
python3 srun_login.py probe            # 看门户 / ac_id / 本机 IP / 在线状态
python3 srun_login.py login -u 20261794 -p '身份证后六位'
python3 srun_login.py status           # 在线信息 + 外网连通
python3 srun_login.py daemon           # 常驻，断网自动重连（读 config.json）
python3 srun_login.py daemon --once    # 只检查/登录一次
python3 srun_login.py logout           # 下线当前 IP
```

密码有三种给法：

```bash
# 1. 命令行（会进 shell history，不推荐）
python3 srun_login.py login -u 20261794 -p 'xxx'

# 2. 环境变量
export SRUN_USER=20261794 SRUN_PASS='xxx'

# 3. macOS 钥匙串（推荐，config.json 里写 "password": "keychain:jlau-srun"）
security add-generic-password -s jlau-srun -a 20261794 -w 'xxx'
```

`config.json`：

```json
{
  "portal": "http://202.198.1.80",
  "ac_id": "4",
  "user": "20261794",
  "password": "keychain:jlau-srun",
  "interval": 60,
  "hmd5_mode": "hmac"
}
```

不想碰命令行：双击 **`登录.command`**（首次会自动生成 config.json 并提示你填写）。

## 四、部署

| 平台 | 做法 |
|---|---|
| macOS | `bash deploy/install_macos.sh`（LaunchAgent 开机自启，卸载加 `uninstall`） |
| Linux | `cp deploy/jlau-srun.service /etc/systemd/system/ && systemctl enable --now jlau-srun` |
| Windows | `schtasks /create /tn JLAU-Srun /tr "python C:\jlau\srun_login.py daemon" /sc onlogon /rl highest` |
| 路由器 / OpenWrt | 拷 `srun_login.py` + `config.json`，`python3 srun_login.py daemon &`（纯标准库，无依赖） |

日志：`~/Library/Logs/jlau-srun.log`（macOS）或 `--log /path/srun.log`。

## 五点五、桌面客户端（Rust，三端）

`menubar/` 是原生托盘客户端：菜单栏/托盘显示「校」字，断网自动登录，菜单里可改检测间隔。

| 平台 | 产物 |
|---|---|
| macOS (arm64) | `JLAU校园网-1.0.dmg`（含 .app，无 Dock 图标） |
| Windows (x64) | `JLAU校园网-windows-x64.exe` |
| Linux (x64) | `JLAU校园网-linux-x64.tar.gz` |

```bash
cd menubar
cargo build --release
bash deploy/make_dmg.sh          # macOS 打包
cargo test && cargo clippy -- -D warnings
```

CI 见 [.github/workflows/build.yml](.github/workflows/build.yml)：三平台矩阵构建、跑测试与 clippy、
上传产物，打 `v*` tag 时自动发 Release。

协议在 Rust 里重写了一遍（`menubar/src/crypt.rs`、`srun.rs`），`cargo test` 会验证
HMAC 与 info 密文和浏览器逐字节一致。配置与 Python 版共用同一份 `config.json`。
详见 [menubar/README.md](menubar/README.md)。

## 五、参考项目

- [zu1k/srun](https://github.com/zu1k/srun) —— Rust 实现，协议细节的经典参照
- [Tim-Conner/SrunPortaLogin](https://github.com/Tim-Conner/SrunPortaLogin) —— Python 通用版
- [zju-autologin](https://pypi.org/project/zju-autologin/) —— 浙大深澜客户端，含完整的 `docs/srun-protocol.md`
- [深澜登录协议分析（北理工版）](https://zhuanlan.zhihu.com/p/122556315) —— `getJSON` 钩子的原始分析
- [云南财经大学深澜协议](https://blog.csdn.net/zhou_bw/article/details/147927780) —— `{SRBX1}` / xEncode 拆解

## 六、常见返回码

| error / error_msg | 含义 | 处理 |
|---|---|---|
| `ok` | 成功 | — |
| `password_error` / `E2901` | 账号或密码错误 | 检查密码（默认身份证后六位） |
| `sign_error` | chksum 校验失败 | 加密链路问题，换 `--hmd5-mode concat` |
| `info_error` | info 解密失败 | XXTEA / base64 表不匹配 |
| `no_response_data_error` | AC/Radius 无响应 | 多因该 IP 已在线；脚本自动回查会话 |
| `ip_already_online_error` | IP 已在线 | 视为成功 |
| `missing_required_parameters_error` | 少参数（如空 username） | challenge 必须带 username |
| `E2553` | 密码错误次数过多被锁 | 等锁定期过，别刷 |
| `E2610` | 在线设备数超限 | 去自助服务踢设备 |

## 七、注意

- `get_challenge` 有频率限制（60s 有效期，短时间多次会拒），脚本内部已做重试与退避，别手动并发刷。
- 密码错会累计失败次数导致账号锁定 —— 调试时先用 `probe` / `status`，并确认密码。
- 校园网已在线时重复登录一般不会把现有会话踢掉，但 `logout` 会。
- 本目录 `ref/` 保留了门户的 3 个 JS 与页面快照，作为逆向证据，重新核对时可直接比对。

## 文件

```
srun_login.py              主程序（probe/login/logout/status/daemon）
menubar/                   macOS 菜单栏版（Rust：winit + tray-icon + muda）
config.example.json        配置模板
登录.command               双击登录
deploy/install_macos.sh    macOS 开机自启安装/卸载
deploy/com.jlau.srun.plist.tmpl
deploy/jlau-srun.service   Linux systemd
ref/                       门户 JS 与页面快照（逆向证据）
```
