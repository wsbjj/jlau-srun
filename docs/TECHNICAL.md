# 技术文档

本文档面向开发者，介绍吉林农业大学校园网深澜 SRun 认证系统的技术实现细节。

---

## 目录

- [认证系统概述](#认证系统概述)
- [接口清单](#接口清单)
- [认证流程](#认证流程)
- [加密算法](#加密算法)
- [协议逆向过程](#协议逆向过程)
- [实现细节](#实现细节)
- [项目结构](#项目结构)

---

## 认证系统概述

### 基本信息

- **认证系统**: 深澜 Srun
- **服务器版本**: `SRunCGIAuthIntfSvr V1.18 B20191227`
- **门户版本**: `Srun portal server golang version`
- **反向代理**: nginx
- **认证地址**: `http://202.198.1.80`
- **接入点 ID** (`ac_id`): `4`（本校唯一 SSID 分组）

### 门户入口

```
302 → /index_3.html → /srun_portal_pc?ac_id=4&theme=wifi
```

---

## 接口清单

所有接口均已实测验证。

| 用途 | 方法 | 路径 | 关键参数 |
|------|------|------|----------|
| 获取挑战串 | GET | `/cgi-bin/get_challenge` | `username`, `ip`（可空）, `callback=cb` |
| 登录 | GET | `/cgi-bin/srun_portal` | `action=login` + 13 个参数 |
| 登出 | GET | `/cgi-bin/srun_portal` | `action=logout&ac_id=4&ip=<ip>&username=<可空>` |
| 在线信息 | GET | `/cgi-bin/rad_user_info` | `callback=cb` |
| 门户页 | GET | `/srun_portal_pc` | `ac_id=4&theme=wifi` |

### 1. 获取挑战串 (get_challenge)

**请求示例**:

```http
GET /cgi-bin/get_challenge?callback=cb&username=2021001234&ip= HTTP/1.1
Host: 202.198.1.80
```

**响应示例**:

```json
cb({
  "challenge": "887362c9468ab1416b8ce6a2d0967e2083fd637ec2c3c774786cf6c84b5bb9e4",
  "client_ip": "10.7.224.192",
  "ecode": 0,
  "error": "ok",
  "error_msg": "",
  "expire": "60",
  "online_ip": "10.7.224.192",
  "res": "ok",
  "srun_ver": "SRunCGIAuthIntfSvr V1.18 B20191227",
  "st": 1731654030
})
```

### 2. 登录 (srun_portal)

**必需参数** (13 个):

| 参数 | 说明 | 示例 |
|------|------|------|
| `action` | 固定 `login` | `login` |
| `ac_id` | 接入点 ID | `4` |
| `username` | 用户名 | `2021001234` |
| `password` | 加密后的密码 | `{MD5}xxxx` |
| `ip` | 客户端 IP | `10.7.224.192` |
| `info` | 加密后的 JSON 信息 | `{SRBX1}xxxx` |
| `chksum` | SHA1 签名 | `abc123...` |
| `n` | 固定 `200` | `200` |
| `type` | 固定 `1` | `1` |
| `double_stack` | 固定 `0` | `0` |
| `callback` | JSONP 回调 | `cb` |

**请求示例**:

```http
GET /cgi-bin/srun_portal?action=login&username=2021001234&password={MD5}...&ac_id=4&ip=10.7.224.192&info={SRBX1}...&chksum=...&n=200&type=1&os=Windows&name=Windows&double_stack=0&callback=cb HTTP/1.1
Host: 202.198.1.80
```

**成功响应**:

```json
cb({
  "error": "ok",
  "res": "ok",
  "srun_ver": "SRunCGIAuthIntfSvr V1.18 B20191227",
  "client_ip": "10.7.224.192",
  "online_ip": "10.7.224.192",
  "...": "..."
})
```

---

## 认证流程

### 完整流程图

```
┌─────────────┐
│   启动程序   │
└──────┬──────┘
       │
       ▼
┌─────────────────────────┐
│ 1. 发送 get_challenge    │
│    获取挑战串 (challenge) │
└──────────┬──────────────┘
           │
           ▼
┌─────────────────────────┐
│ 2. 使用 HMAC-MD5 加密    │
│    password + challenge  │
└──────────┬──────────────┘
           │
           ▼
┌─────────────────────────┐
│ 3. 构造 info JSON        │
│    包含用户名、IP、MAC等  │
└──────────┬──────────────┘
           │
           ▼
┌─────────────────────────┐
│ 4. Base64 编码 info      │
│    前缀 {SRBX1}          │
└──────────┬──────────────┘
           │
           ▼
┌─────────────────────────┐
│ 5. SHA1 签名整个请求     │
│    生成 chksum           │
└──────────┬──────────────┘
           │
           ▼
┌─────────────────────────┐
│ 6. 发送 login 请求       │
│    携带所有参数          │
└──────────┬──────────────┘
           │
           ▼
┌─────────────────────────┐
│ 7. 解析响应              │
│    成功 / 失败 / 错误码   │
└─────────────────────────┘
```

### 详细步骤

#### Step 1: 获取挑战串

```python
challenge_url = f"{portal}/cgi-bin/get_challenge?callback=cb&username={username}&ip="
response = requests.get(challenge_url)
challenge = extract_json(response.text)['challenge']
```

#### Step 2: 加密密码

使用 HMAC-MD5 算法：

```python
import hmac
import hashlib

def encrypt_password(password, challenge):
    h = hmac.new(challenge.encode(), password.encode(), hashlib.md5)
    return "{MD5}" + h.hexdigest()
```

#### Step 3-4: 构造并编码 info

```python
import json
import base64

info = {
    "username": username,
    "password": password,
    "ip": client_ip,
    "acid": ac_id,
    "enc_ver": "srun_bx1"
}
info_json = json.dumps(info, separators=(',', ':'))
info_encoded = "{SRBX1}" + base64.b64encode(info_json.encode()).decode()
```

#### Step 5: 计算 chksum

使用 SHA1 对整个参数字符串签名：

```python
import hashlib

def calc_chksum(challenge, username, password, ac_id, ip, n, type_, info):
    raw = f"{challenge}{username}"
    raw += f"{challenge}{password}"
    raw += f"{challenge}{ac_id}"
    raw += f"{challenge}{ip}"
    raw += f"{challenge}{n}"
    raw += f"{challenge}{type_}"
    raw += f"{challenge}{info}"
    return hashlib.sha1(raw.encode()).hexdigest()
```

#### Step 6: 发送登录请求

```python
login_params = {
    'action': 'login',
    'username': username,
    'password': encrypted_password,
    'ac_id': ac_id,
    'ip': client_ip,
    'info': info_encoded,
    'chksum': chksum,
    'n': '200',
    'type': '1',
    'os': 'Windows',
    'name': 'Windows',
    'double_stack': '0',
    'callback': 'cb'
}
response = requests.get(f"{portal}/cgi-bin/srun_portal", params=login_params)
```

---

## 加密算法

### HMAC-MD5 密码加密

**算法**: HMAC (Hash-based Message Authentication Code) with MD5

**输入**:
- `key`: challenge 字符串
- `message`: 用户密码

**输出**: `{MD5}` + 32 位十六进制字符串

**Python 实现**:

```python
import hmac
import hashlib

def hmac_md5(key, message):
    """
    HMAC-MD5 加密
    """
    h = hmac.new(key.encode('utf-8'), message.encode('utf-8'), hashlib.md5)
    return h.hexdigest()

# 使用示例
challenge = "887362c9468ab1416b8ce6a2d0967e2083fd637ec2c3c774786cf6c84b5bb9e4"
password = "mypassword"
encrypted = "{MD5}" + hmac_md5(challenge, password)
print(encrypted)  # {MD5}abc123...
```

### SHA1 签名算法

**算法**: SHA-1

**用途**: 对整个请求参数字符串进行签名，防止篡改

**拼接顺序**:

```
challenge + username +
challenge + password +
challenge + ac_id +
challenge + ip +
challenge + n +
challenge + type +
challenge + info
```

**Python 实现**:

```python
import hashlib

def sha1_chksum(challenge, username, hmd5, ac_id, ip, n, type_, info):
    """
    计算 chksum 签名
    """
    raw = challenge + username
    raw += challenge + hmd5
    raw += challenge + ac_id
    raw += challenge + ip
    raw += challenge + str(n)
    raw += challenge + str(type_)
    raw += challenge + info
    
    return hashlib.sha1(raw.encode('utf-8')).hexdigest()
```

### Base64 编码 info

**格式**: `{SRBX1}` + Base64(JSON)

**JSON 结构**:

```json
{
  "username": "2021001234",
  "password": "mypassword",
  "ip": "10.7.224.192",
  "acid": "4",
  "enc_ver": "srun_bx1"
}
```

**Python 实现**:

```python
import json
import base64

def encode_info(username, password, ip, ac_id):
    """
    构造并编码 info 参数
    """
    info_dict = {
        "username": username,
        "password": password,
        "ip": ip,
        "acid": ac_id,
        "enc_ver": "srun_bx1"
    }
    # 紧凑 JSON（无空格）
    info_json = json.dumps(info_dict, separators=(',', ':'))
    # Base64 编码
    info_b64 = base64.b64encode(info_json.encode('utf-8')).decode('utf-8')
    return "{SRBX1}" + info_b64
```

---

## 协议逆向过程

### 工具与方法

1. **浏览器开发者工具** (Chrome DevTools)
   - Network 面板捕获请求
   - 分析 URL 参数和响应

2. **JavaScript 源码分析**
   - 查看 `jquery.srun.portal.js`
   - 反混淆加密逻辑

3. **抓包工具** (Wireshark)
   - 捕获完整 HTTP 流量
   - 验证参数顺序和编码

### 逆向发现

#### 1. HMAC-MD5 加密

原始 JS 代码片段：

```javascript
function get_md5(password, challenge) {
    return "{MD5}" + CryptoJS.HmacMD5(password, challenge).toString();
}
```

**关键发现**: 使用 HMAC-MD5，而非简单的 MD5

#### 2. info 参数构造

原始 JS 代码片段：

```javascript
var info = {
    "username": username,
    "password": password,
    "ip": ip,
    "acid": ac_id,
    "enc_ver": "srun_bx1"
};
var info_str = "{SRBX1}" + Base64.encode(JSON.stringify(info));
```

**关键发现**: 
- JSON 需要紧凑格式（无多余空格）
- Base64 编码前添加 `{SRBX1}` 前缀

#### 3. chksum 计算

原始 JS 代码片段：

```javascript
var chkstr = challenge + username +
             challenge + hmd5 +
             challenge + ac_id +
             challenge + ip +
             challenge + n +
             challenge + type +
             challenge + info;
return CryptoJS.SHA1(chkstr).toString();
```

**关键发现**: 
- 每个参数前都要拼接 challenge
- 使用 SHA1 而非 MD5

---

## 实现细节

### Python 客户端 (`srun_login.py`)

**特点**:
- ✅ 零依赖，仅使用 Python 标准库
- ✅ 支持自动重连
- ✅ 完整错误处理
- ✅ 日志记录

**核心函数**:

```python
def do_login(config):
    """
    执行登录流程
    """
    # 1. 获取挑战串
    challenge = get_challenge(config['portal'], config['user'])
    
    # 2. 加密密码
    encrypted_pwd = hmac_md5(challenge, config['password'])
    
    # 3. 构造 info
    info = encode_info(config['user'], config['password'], 
                      get_client_ip(), config['ac_id'])
    
    # 4. 计算 chksum
    chksum = calc_chksum(challenge, config['user'], encrypted_pwd,
                         config['ac_id'], get_client_ip(), 200, 1, info)
    
    # 5. 发送登录请求
    result = send_login_request(config['portal'], {
        'username': config['user'],
        'password': encrypted_pwd,
        'ac_id': config['ac_id'],
        'ip': get_client_ip(),
        'info': info,
        'chksum': chksum,
        # ... 其他参数
    })
    
    return result
```

### Rust 托盘应用 (`menubar/`)

**技术栈**:
- `tray-icon`: 系统托盘图标
- `muda`: 菜单管理
- `ureq`: HTTP 客户端（零依赖）
- `serde`: JSON 序列化

**核心模块**:

```
menubar/src/
├── main.rs       # 主入口，托盘和菜单
├── srun.rs       # 认证逻辑
├── crypt.rs      # 加密算法
├── config.rs     # 配置管理
├── monitor.rs    # 网络监控
└── platform.rs   # 平台相关
```

**加密实现** (`crypt.rs`):

```rust
use hmac::{Hmac, Mac};
use md5::Md5;
use sha1::{Digest, Sha1};

type HmacMd5 = Hmac<Md5>;

pub fn hmac_md5(challenge: &str, password: &str) -> String {
    let mut mac = HmacMd5::new_from_slice(challenge.as_bytes()).unwrap();
    mac.update(password.as_bytes());
    let result = mac.finalize();
    format!("{{MD5}}{}", hex::encode(result.into_bytes()))
}

pub fn sha1_chksum(parts: &[&str]) -> String {
    let mut hasher = Sha1::new();
    for part in parts {
        hasher.update(part.as_bytes());
    }
    hex::encode(hasher.finalize())
}
```

---

## 项目结构

```
jlau-srun/
├── srun_login.py           # Python 客户端（零依赖）
├── config.example.json     # 配置模板
├── 登录.command            # macOS 快捷启动脚本
│
├── menubar/                # Rust 托盘应用
│   ├── Cargo.toml
│   ├── src/
│   │   ├── main.rs        # 托盘主程序
│   │   ├── srun.rs        # 认证逻辑
│   │   ├── crypt.rs       # 加密算法
│   │   ├── config.rs      # 配置管理
│   │   ├── monitor.rs     # 网络监控
│   │   └── platform.rs    # 平台适配
│   └── deploy/
│       ├── make_app.sh    # macOS 打包脚本
│       ├── make_dmg.sh    # DMG 制作脚本
│       └── install_launchagent.sh
│
├── deploy/                # 系统服务部署
│   ├── com.jlau.srun.plist.tmpl     # macOS LaunchAgent
│   ├── install_macos.sh             # macOS 安装脚本
│   └── jlau-srun.service            # Linux systemd
│
├── ref/                   # 参考资料（原始 JS）
│   ├── jquery.srun.portal.js
│   ├── pc.html
│   └── portal.html
│
├── docs/                  # 文档
│   ├── images/           # 截图
│   └── TECHNICAL.md      # 本技术文档
│
└── .github/
    └── workflows/
        └── build.yml      # CI/CD 配置
```

---

## 调试技巧

### 1. 抓包分析

使用 Chrome DevTools 的 Network 面板：

```bash
# 1. 打开浏览器开发者工具 (F12)
# 2. 进入 Network 标签
# 3. 访问 http://202.198.1.80
# 4. 手动登录一次
# 5. 查看 get_challenge 和 srun_portal 请求
```

### 2. 验证加密

使用 Python 交互式环境：

```python
import hmac
import hashlib

challenge = "your_challenge_here"
password = "your_password"

h = hmac.new(challenge.encode(), password.encode(), hashlib.md5)
print("{MD5}" + h.hexdigest())
```

### 3. 日志调试

在 `srun_login.py` 中启用详细日志：

```python
import logging
logging.basicConfig(level=logging.DEBUG)
```

---

## 参考资料

- [深澜 Srun 官方文档](http://www.srun.com/)
- [HMAC-MD5 RFC 2104](https://www.ietf.org/rfc/rfc2104.txt)
- [SHA-1 RFC 3174](https://www.ietf.org/rfc/rfc3174.txt)
- [Base64 RFC 4648](https://www.ietf.org/rfc/rfc4648.txt)

---

## 开发贡献

欢迎提交 PR 改进实现！

### 开发环境搭建

**Python**:
```bash
python3 srun_login.py
```

**Rust**:
```bash
cd menubar
cargo build --release
cargo run
```

### 测试

```bash
# Python
python3 -m pytest tests/

# Rust
cargo test
```

---

## 许可

MIT License

