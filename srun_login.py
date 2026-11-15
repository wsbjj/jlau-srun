#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
吉林农业大学校园网 · 深澜 SRun 认证客户端 (零依赖 / 可部署)
=============================================================
门户: http://202.198.1.80
服务: SRunCGIAuthIntfSvr V1.18 B20191227   (Srun portal golang)

接口（逆向自门户 /static/js/jquery.srun.portal.js + all.min.js）:
    GET /cgi-bin/get_challenge?username=&ip=&callback=cb   -> token / client_ip
    GET /cgi-bin/srun_portal?action=login&...&callback=cb  -> 认证结果
    GET /cgi-bin/srun_portal?action=logout&ac_id=&ip=&callback=cb
    GET /cgi-bin/rad_user_info?callback=cb                 -> 在线会话/流量/余额

加密链路（逐字节与浏览器一致，已用 node 加载门户 JS 交叉验证）:
    token  = challenge
    hmd5   = HMAC-MD5(key=token, msg=password)          # js-md5 的 md5(pwd, token)
    info   = "{SRBX1}" + srun_b64( xEncode(JSON, token) )
             xEncode = XXTEA(delta=0x9E3779B9, 32bit)
             srun_b64 字母表 LVoJPiCN2R8G90yg+hmFHuacZ1OWMnrsSTXkYpUq/3dlbfKwv6xztjI7DeBE45QA
    chksum = SHA1( token+username + token+hmd5 + token+ac_id + token+ip
                   + token+200 + token+1 + token+info )
    password 字段 = "{MD5}" + hmd5

用法:
    python3 srun_login.py probe                 # 探测门户/本机 IP/在线状态
    python3 srun_login.py login -u 学号 -p 密码
    python3 srun_login.py status                # 在线信息 + 外网连通
    python3 srun_login.py daemon                # 断网自动重连（读 config.json）
    python3 srun_login.py logout                # 下线当前 IP
"""

from __future__ import annotations

import argparse
import base64
import getpass
import hashlib
import hmac
import json
import logging
import os
import random
import re
import socket
import struct
import subprocess
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from dataclasses import dataclass, field
from typing import Any, Dict, List, Optional, Tuple

# ----------------------------------------------------------------------------
# 常量
# ----------------------------------------------------------------------------
DEFAULT_PORTAL = "http://202.198.1.80"
DEFAULT_AC_ID = "4"
CGI = "/cgi-bin"

SRUN_BASE64_ALPHA = "LVoJPiCN2R8G90yg+hmFHuacZ1OWMnrsSTXkYpUq/3dlbfKwv6xztjI7DeBE45QA"
STD_BASE64_ALPHA = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"
_B64_TRANS = str.maketrans(STD_BASE64_ALPHA, SRUN_BASE64_ALPHA)

ENC_VER = "srun_bx1"
N_PARAM = "200"
TYPE_PARAM = "1"

DEFAULT_UA = (
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 "
    "(KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36"
)

CONFIG_NAME = "config.json"
LOG_NAME = "srun.log"

ECODE_MSG = {
    0: "成功",
    1: "账号或密码错误",
    2: "账号已欠费",
    3: "账号已停机",
    4: "账号被禁用",
    5: "密码错误次数过多",
    6: "账号已在线",
    7: "AC 认证失败",
    8: "系统繁忙",
    9: "IP 不在允许范围",
    10: "MAC 地址不匹配",
    11: "账号不在线",
    12: "参数错误",
    13: "不在认证时间段",
    14: "流量/时长超限",
}

ERROR_MSG = {
    "ok": "成功",
    "": "成功",
    "login_error": "登录失败",
    "logout_error": "下线失败",
    "password_error": "账号或密码错误",
    "chksum_error": "校验和错误（加密链路不匹配）",
    "sign_error": "签名校验失败（chksum 不匹配）",
    "info_error": "info 解密失败（XXTEA/base64 不匹配）",
    "ip_already_online_error": "该 IP 已在线",
    "not_online_error": "该 IP 当前不在线",
    "user_not_found": "用户不存在",
    "no_response_data_error": "AC/Radius 无响应（常见于该 IP 已在线，或需在校园网内发起）",
    "E2901": "账号或密码错误",
    "E2902": "账号已欠费",
    "E2903": "账号已停机",
    "E2904": "账号被禁用",
    "E2606": "IP 已被其他账号绑定",
    "E2610": "在线设备数超限",
    "E2553": "密码错误次数过多，账号被锁定",
}

SUCCESS_ERRORS = ("ok", "")
ALREADY_ONLINE = ("ip_already_online_error", "already_online", "E2620")

log = logging.getLogger("srun")


# ----------------------------------------------------------------------------
# 加密原语
# ----------------------------------------------------------------------------
def _str2ints(s: str, include_len: bool) -> List[int]:
    """JS s(a,b)：每 4 个 charCode 组成 32bit 小端 int（精确复刻 charCodeAt）。"""
    codes = [ord(c) for c in s]
    v = []
    for i in range(0, len(codes), 4):
        chunk = codes[i:i + 4] + [0] * (4 - len(codes[i:i + 4]))
        v.append((chunk[0] | chunk[1] << 8 | chunk[2] << 16 | chunk[3] << 24) & 0xFFFFFFFF)
    if include_len:
        v.append(len(codes))
    return v


def _ints2bytes(v: List[int]) -> bytes:
    return b"".join(struct.pack("<I", x & 0xFFFFFFFF) for x in v)


def xencode(data: str, key: str) -> bytes:
    """深澜 xEncode = XXTEA（32 位无符号，delta=0x9E3779B9）。"""
    if data == "":
        return b""
    v = _str2ints(data, True)
    k = _str2ints(key, False)
    while len(k) < 4:
        k.append(0)

    n = len(v) - 1
    z, y, d = v[n], v[0], 0
    c = 0x9E3779B9
    q = 6 + 52 // (n + 1)

    while q > 0:
        q -= 1
        d = (d + c) & 0xFFFFFFFF
        e = (d >> 2) & 3
        for p in range(n):
            y = v[p + 1]
            m = ((z >> 5) ^ ((y << 2) & 0xFFFFFFFF)) & 0xFFFFFFFF
            m = (m + ((((y >> 3) ^ ((z << 4) & 0xFFFFFFFF)) ^ (d ^ y)) & 0xFFFFFFFF)) & 0xFFFFFFFF
            m = (m + ((k[(p & 3) ^ e] ^ z) & 0xFFFFFFFF)) & 0xFFFFFFFF
            z = (v[p] + m) & 0xFFFFFFFF
            v[p] = z
        y = v[0]
        m = ((z >> 5) ^ ((y << 2) & 0xFFFFFFFF)) & 0xFFFFFFFF
        m = (m + ((((y >> 3) ^ ((z << 4) & 0xFFFFFFFF)) ^ (d ^ y)) & 0xFFFFFFFF)) & 0xFFFFFFFF
        m = (m + ((k[(n & 3) ^ e] ^ z) & 0xFFFFFFFF)) & 0xFFFFFFFF
        z = (v[n] + m) & 0xFFFFFFFF
        v[n] = z

    return _ints2bytes(v)


def srun_base64(data: bytes) -> str:
    """深澜自定义字母表 base64（仅换表，填充符仍为 =）。"""
    return base64.b64encode(data).decode("ascii").translate(_B64_TRANS)


def build_info(username: str, password: str, ip: str, acid: str, token: str) -> str:
    payload = {
        "username": username,
        "password": password,
        "ip": ip,
        "acid": acid,
        "enc_ver": ENC_VER,
    }
    js = json.dumps(payload, separators=(",", ":"), ensure_ascii=False)
    return "{SRBX1}" + srun_base64(xencode(js, token))


def hmd5_password(password: str, token: str) -> str:
    """js-md5 的 md5(password, token) —— 实测为 HMAC-MD5(key=token, msg=password)。"""
    return hmac.new(token.encode("utf-8"), password.encode("utf-8"), hashlib.md5).hexdigest()


def hmd5_concat(password: str, token: str) -> str:
    """兼容部分深澜版本的 md5(password + token)。"""
    return hashlib.md5((password + token).encode("utf-8")).hexdigest()


def build_chksum(token: str, username: str, hmd5: str, ac_id: str, ip: str,
                 n: str, type_: str, info: str) -> str:
    raw = (token + username + token + hmd5 + token + ac_id + token + ip +
           token + n + token + type_ + token + info)
    return hashlib.sha1(raw.encode("utf-8")).hexdigest()


def parse_jsonp(text: str) -> Dict[str, Any]:
    """解析 cb({...}) 形式的 JSONP。"""
    text = text.strip()
    start, end = text.find("{"), text.rfind("}")
    if start == -1 or end < start:
        raise ValueError(f"无法解析响应: {text[:200]!r}")
    return json.loads(text[start:end + 1])


# ----------------------------------------------------------------------------
# 结果对象
# ----------------------------------------------------------------------------
@dataclass
class LoginResult:
    ok: bool
    error: str = ""
    ecode: Any = None
    message: str = ""
    raw: Dict[str, Any] = field(default_factory=dict)
    ip: str = ""
    elapsed: float = 0.0
    note: str = ""

    def __str__(self) -> str:
        head = "✅ 登录成功" if self.ok else "❌ 登录失败"
        detail = self.message or ERROR_MSG.get(self.error, self.error)
        extra = f" {self.note}" if self.note else ""
        return f"{head}（{self.elapsed:.2f}s）ip={self.ip or '-'} error={self.error!r} {detail}{extra}"


# ----------------------------------------------------------------------------
# 客户端
# ----------------------------------------------------------------------------
class SrunClient:
    def __init__(self, portal: str = DEFAULT_PORTAL, ac_id: str = DEFAULT_AC_ID,
                 timeout: float = 12.0, ua: str = DEFAULT_UA, insecure_tls: bool = True):
        self.portal = portal.rstrip("/")
        self.ac_id = str(ac_id)
        self.timeout = timeout
        self.ua = ua
        self._ssl_ctx = None
        if self.portal.startswith("https") and insecure_tls:
            import ssl
            ctx = ssl.create_default_context()
            ctx.check_hostname = False
            ctx.verify_mode = ssl.CERT_NONE
            self._ssl_ctx = ctx

    # ---------------- HTTP ----------------
    def _get(self, path: str, params: Optional[Dict[str, Any]] = None,
             callback: Optional[str] = "cb") -> str:
        url = self.portal + path
        if params is not None:
            q = dict(params)
            if callback:
                q["callback"] = callback
            url += "?" + urllib.parse.urlencode(q)
        log.debug("GET %s", url)
        req = urllib.request.Request(url, headers={
            "User-Agent": self.ua,
            "Accept": "*/*",
            "Accept-Language": "zh-CN,zh;q=0.9",
        })
        try:
            with urllib.request.urlopen(req, timeout=self.timeout, context=self._ssl_ctx) as r:
                return r.read().decode("utf-8", "replace")
        except urllib.error.HTTPError as e:
            body = e.read().decode("utf-8", "replace")
            raise RuntimeError(f"HTTP {e.code} {path}: {body[:200]}") from None
        except urllib.error.URLError as e:
            raise RuntimeError(f"无法连接门户 {self.portal}{path}: {e.reason}") from None

    # ---------------- 接口 ----------------
    def get_challenge(self, username: str = "", ip: str = "") -> Dict[str, Any]:
        return parse_jsonp(self._get(f"{CGI}/get_challenge", {"username": username, "ip": ip}))

    def online_info(self) -> Dict[str, Any]:
        try:
            return parse_jsonp(self._get(f"{CGI}/rad_user_info", {}))
        except Exception:
            return {}

    def is_online(self) -> bool:
        info = self.online_info()
        return info.get("error") == "ok" and bool(info.get("online_ip"))

    def portal_ac_id(self) -> str:
        body = self._get("/srun_portal_pc", {"theme": "wifi"}, callback=None)
        m = re.search(r'id="ac_id"\s+value="(\d+)"', body) or re.search(r"ac_id=(\d+)", body)
        return m.group(1) if m else self.ac_id

    def login(self, username: str, password: str, ip: str = "",
              hmd5_mode: str = "hmac", os_name: str = "Windows 10",
              name: str = "Windows", double_stack: int = 0,
              retries: int = 2, trust_online: bool = True) -> LoginResult:
        t0 = time.time()
        last: Optional[LoginResult] = None
        for attempt in range(retries + 1):
            try:
                ch = self.get_challenge(username, ip)
            except Exception as e:
                last = LoginResult(False, error="network", message=str(e), elapsed=time.time() - t0)
                time.sleep(1.0 + attempt)
                continue

            if ch.get("error") not in SUCCESS_ERRORS and ch.get("res") != "ok":
                msg = str(ch.get("error_msg") or ch.get("error") or "")
                return LoginResult(False, error=ch.get("error", ""), ecode=ch.get("ecode"),
                                   message=ERROR_MSG.get(msg, msg) or msg, raw=ch,
                                   elapsed=time.time() - t0)

            token = ch.get("challenge", "")
            client_ip = ip or ch.get("client_ip") or ch.get("online_ip") or ""
            if not token or not client_ip:
                last = LoginResult(False, error="challenge",
                                   message=f"challenge 响应异常: {ch}", raw=ch,
                                   elapsed=time.time() - t0)
                time.sleep(1.0 + attempt)
                continue

            hmd5 = (hmd5_password if hmd5_mode == "hmac" else hmd5_concat)(password, token)
            info = build_info(username, password, client_ip, self.ac_id, token)
            params = {
                "action": "login",
                "username": username,
                "password": "{MD5}" + hmd5,
                "ac_id": self.ac_id,
                "ip": client_ip,
                "chksum": build_chksum(token, username, hmd5, self.ac_id, client_ip,
                                       N_PARAM, TYPE_PARAM, info),
                "info": info,
                "n": N_PARAM,
                "type": TYPE_PARAM,
                "os": os_name,
                "name": name,
                "double_stack": double_stack,
            }
            try:
                resp = parse_jsonp(self._get(f"{CGI}/srun_portal", params))
            except Exception as e:
                last = LoginResult(False, error="network", message=str(e), ip=client_ip,
                                   elapsed=time.time() - t0)
                time.sleep(1.0 + attempt)
                continue

            err = str(resp.get("error", "")).strip()
            suc = str(resp.get("suc_msg", "")).strip()
            ploy = str(resp.get("ploy_msg", "")).strip()
            errmsg = str(resp.get("error_msg", "")).strip()
            elapsed = time.time() - t0

            if err in SUCCESS_ERRORS:
                if suc in ALREADY_ONLINE or "already" in suc.lower():
                    return LoginResult(True, error=suc or "already_online",
                                       message="该 IP 已在线，无需重复登录", raw=resp,
                                       ip=client_ip, elapsed=elapsed)
                msg = ploy if ploy and not ploy.startswith("E0000") else "登录成功"
                return LoginResult(True, error=err or "ok", ecode=resp.get("ecode"),
                                   message=msg, raw=resp, ip=client_ip, elapsed=elapsed)

            human = (ERROR_MSG.get(errmsg) or ERROR_MSG.get(err) or
                     ECODE_MSG.get(resp.get("ecode") if isinstance(resp.get("ecode"), int) else -1) or
                     ploy or errmsg or err)

            # 已在线场景：no_response_data_error / 直接查会话
            if trust_online and (errmsg == "no_response_data_error" or err == "login_error"):
                info_now = self.online_info()
                if info_now.get("error") == "ok" and info_now.get("online_ip") == client_ip:
                    return LoginResult(True, error=err, message="该 IP 已有在线会话",
                                       raw=resp, ip=client_ip, elapsed=elapsed,
                                       note=f"(账号 {info_now.get('user_name')})")

            retryable = err in ("sign_error", "chksum_error", "info_error") and attempt < retries
            last = LoginResult(False, error=err, ecode=resp.get("ecode"), message=human,
                               raw=resp, ip=client_ip, elapsed=elapsed)
            if retryable:
                if hmd5_mode == "hmac":
                    hmd5_mode = "concat"
                time.sleep(1.5 + attempt)
                continue
            return last

        return last or LoginResult(False, error="unknown", message="登录未完成",
                                   elapsed=time.time() - t0)

    def logout(self, username: str = "", ip: str = "") -> Dict[str, Any]:
        params = {"action": "logout", "ac_id": self.ac_id, "ip": ip}
        if username:
            params["username"] = username
        return parse_jsonp(self._get(f"{CGI}/srun_portal", params))


# ----------------------------------------------------------------------------
# 工具函数
# ----------------------------------------------------------------------------
def setup_logging(verbose: bool, logfile: Optional[str]) -> None:
    handlers: List[logging.Handler] = [logging.StreamHandler(sys.stdout)]
    if logfile:
        handlers.append(logging.FileHandler(logfile, encoding="utf-8"))
    logging.basicConfig(level=logging.DEBUG if verbose else logging.INFO,
                        format="%(asctime)s %(levelname)s %(message)s",
                        handlers=handlers, force=True)


def load_config(path: str) -> Dict[str, Any]:
    for p in (path, os.path.join(os.path.dirname(os.path.abspath(__file__)), CONFIG_NAME)):
        if p and os.path.exists(p):
            with open(p, "r", encoding="utf-8") as f:
                cfg = json.load(f)
            cfg["_path"] = p
            return cfg
    return {}


def read_keychain(service: str, account: str = "") -> str:
    """macOS: security find-generic-password -s SERVICE [-a ACCOUNT] -w"""
    cmd = ["security", "find-generic-password", "-s", service, "-w"]
    if account:
        cmd[3:3] = ["-a", account]
    try:
        return subprocess.run(cmd, capture_output=True, text=True, timeout=5).stdout.strip()
    except Exception:
        return ""


def resolve_password(value: str, cfg: Dict[str, Any]) -> str:
    if not value:
        value = str(cfg.get("password", "") or "")
    if value.startswith("keychain:"):
        svc = value.split(":", 1)[1]
        return read_keychain(svc, str(cfg.get("user", "")))
    if value == "-" or value == "stdin":
        return getpass.getpass("校园网密码: ")
    return value


def local_ip_udp() -> str:
    try:
        s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        s.settimeout(2)
        s.connect(("223.5.5.5", 53))
        ip = s.getsockname()[0]
        s.close()
        return ip
    except Exception:
        return ""


def internet_ok(timeout: float = 5.0) -> bool:
    """多目标探测，任一 2xx/3xx 即认为在线。"""
    probes = (
        "http://connect.rom.miui.com/generate_204",
        "http://www.baidu.com/favicon.ico",
        "http://captive.apple.com/hotspot-detect.html",
        "http://nmcheck.gnome.org/check_network_status.txt",
    )
    for url in probes:
        try:
            req = urllib.request.Request(url, headers={"User-Agent": DEFAULT_UA})
            with urllib.request.urlopen(req, timeout=timeout) as r:
                if r.status < 400:
                    body = r.read(4096)
                    # 门户劫持常返回 200 + 登录页，尝试识别
                    if b"<html" in body.lower() and (b"login" in body.lower() or b"\xe8\xae\xa4\xe8\xaf\x81" in body):
                        continue
                    return True
        except Exception:
            continue
    return False


def human_bytes(n: Optional[float]) -> str:
    if not n:
        return "0 B"
    units = ["B", "KB", "MB", "GB", "TB"]
    f, i = float(n), 0
    while f >= 1024 and i < len(units) - 1:
        f /= 1024
        i += 1
    return f"{f:.2f} {units[i]}"


def print_online(info: Dict[str, Any]) -> None:
    if info.get("error") != "ok":
        print("   在线状态 : 未登录 / 已离线")
        return
    print("   在线状态 : 已登录")
    print(f"   账号     : {info.get('user_name')}")
    print(f"   本机 IP  : {info.get('online_ip')}")
    print(f"   套餐     : {info.get('products_name')}")
    print(f"   会话时长 : {int(info.get('sum_seconds') or 0) // 60} 分钟")
    print(f"   本次流量 : ↑{human_bytes(info.get('bytes_out'))}  ↓{human_bytes(info.get('bytes_in'))}")
    print(f"   累计流量 : {human_bytes(info.get('all_bytes'))}")
    print(f"   余额     : {info.get('user_balance')} 元")
    add = int(info.get("add_time") or 0)
    if add:
        print(f"   上线时间 : {time.strftime('%Y-%m-%d %H:%M:%S', time.localtime(add))}")


# ----------------------------------------------------------------------------
# 子命令
# ----------------------------------------------------------------------------
def cmd_probe(args, cfg) -> int:
    c = SrunClient(args.portal, args.ac_id, args.timeout)
    print(f"== 门户探测: {c.portal} ==")
    print(f"   网卡 IP     : {local_ip_udp() or '未知'}")
    try:
        print(f"   门户 ac_id  : {c.portal_ac_id()} (配置使用 {c.ac_id})")
    except Exception as e:
        print(f"   门户 ac_id  : 解析失败 {e}")
    try:
        probe_user = str(cfg.get("user", "") or args.user or "test")
        ch = c.get_challenge(probe_user)
        print(f"   服务器视角IP: {ch.get('client_ip')}")
        print(f"   challenge   : {ch.get('challenge')}")
        print(f"   srun_ver    : {ch.get('srun_ver')}")
        print(f"   有效期      : {ch.get('expire')} s")
    except Exception as e:
        print(f"   challenge 获取失败: {e}")
    print("-- 在线信息 --")
    print_online(c.online_info())
    print(f"-- 外网连通 : {'✅ 通' if internet_ok() else '❌ 不通'} --")
    return 0


def _credentials(args, cfg) -> Tuple[str, str]:
    user = args.user or str(cfg.get("user", "") or "")
    pwd = resolve_password(args.password, cfg)
    return user, pwd


def cmd_login(args, cfg) -> int:
    user, pwd = _credentials(args, cfg)
    if not user or not pwd:
        print("缺少账号/密码：--user/--password，config.json，或环境变量 SRUN_USER/SRUN_PASS")
        return 2
    c = SrunClient(args.portal, args.ac_id, args.timeout)
    res = c.login(user, pwd, ip=args.ip, hmd5_mode=args.hmd5_mode,
                  os_name=args.os_name, name=args.name_platform,
                  double_stack=args.double_stack, trust_online=not args.strict)
    print(res)
    if args.show_raw and res.raw:
        print(json.dumps(res.raw, ensure_ascii=False, indent=2))
    if res.ok:
        time.sleep(0.5)
        print_online(c.online_info())
    return 0 if res.ok else 1


def cmd_logout(args, cfg) -> int:
    c = SrunClient(args.portal, args.ac_id, args.timeout)
    ip = args.ip
    if not ip:
        ip = c.online_info().get("online_ip", "")
    resp = c.logout(args.user or str(cfg.get("user", "") or ""), ip)
    err = str(resp.get("error", ""))
    ok = err in SUCCESS_ERRORS
    print("✅ 已下线" if ok else f"❌ 下线失败: {err} {ERROR_MSG.get(err, '')}")
    if args.show_raw:
        print(json.dumps(resp, ensure_ascii=False, indent=2))
    return 0 if ok else 1


def cmd_status(args, cfg) -> int:
    c = SrunClient(args.portal, args.ac_id, args.timeout)
    print_online(c.online_info())
    ok = internet_ok()
    print(f"   外网连通 : {'✅' if ok else '❌'}")
    return 0 if ok else 1


def cmd_daemon(args, cfg) -> int:
    user, pwd = _credentials(args, cfg)
    if not user or not pwd:
        print("缺少账号/密码：--user/--password，config.json，或环境变量 SRUN_USER/SRUN_PASS")
        return 2
    interval = max(10, int(args.interval or cfg.get("interval", 60)))
    c = SrunClient(args.portal, args.ac_id, args.timeout)
    log.info("守护启动 账号=%s 门户=%s ac_id=%s 间隔=%ss hmd5=%s",
             user, c.portal, c.ac_id, interval, args.hmd5_mode)
    fails = 0
    while True:
        try:
            if internet_ok():
                fails = 0
                log.debug("网络正常")
            else:
                res = c.login(user, pwd, hmd5_mode=args.hmd5_mode, trust_online=True)
                log.info("检测到断网 → %s", res)
                fails = 0 if res.ok else fails + 1
                if fails >= 3:
                    log.warning("连续失败 %d 次，退避 120s 后重试", fails)
                    time.sleep(120)
                if args.once:
                    return 0 if res.ok else 1
        except KeyboardInterrupt:
            log.info("已手动停止")
            return 0
        except Exception as e:
            log.warning("异常: %s", e)
        if args.once:
            return 0
        time.sleep(interval + random.uniform(0, 3))


# ----------------------------------------------------------------------------
# CLI
# ----------------------------------------------------------------------------
def build_parser() -> argparse.ArgumentParser:
    common = argparse.ArgumentParser(add_help=False)
    common.add_argument("--portal", default=None, help=f"门户地址（默认 {DEFAULT_PORTAL}）")
    common.add_argument("--ac-id", default=None, help=f"ac_id（默认 {DEFAULT_AC_ID}）")
    common.add_argument("--timeout", type=float, default=12.0)
    common.add_argument("-v", "--verbose", action="store_true")
    common.add_argument("--log", default=None, help="日志文件路径")
    common.add_argument("--show-raw", action="store_true", help="打印服务端原始响应")

    p = argparse.ArgumentParser(
        description="吉林农业大学 深澜 SRun 校园网认证客户端",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""示例:
  python3 srun_login.py probe
  python3 srun_login.py login -u 20261794 -p '身份证后六位'
  python3 srun_login.py status
  python3 srun_login.py daemon --once -u 20261794 -p ***
  python3 srun_login.py daemon            # 常驻，读 config.json
  python3 srun_login.py logout
""")
    p.add_argument("--config", default=None, help="配置文件路径（默认同目录 config.json）")
    sub = p.add_subparsers(dest="cmd", required=True)

    sp = sub.add_parser("probe", parents=[common], help="探测门户 / ac_id / 本机 IP / 在线状态")
    sp.set_defaults(func=cmd_probe)

    sl = sub.add_parser("login", parents=[common], help="登录")
    sl.add_argument("-u", "--user", default="")
    sl.add_argument("-p", "--password", default="", help="密码，'-' 表示交互输入，keychain:服务名 表示读钥匙串")
    sl.add_argument("--ip", default="", help="指定 IP（默认由 challenge 返回）")
    sl.add_argument("--hmd5-mode", choices=["hmac", "concat"], default="hmac")
    sl.add_argument("--os-name", default="Windows 10")
    sl.add_argument("--name-platform", default="Windows")
    sl.add_argument("--double-stack", type=int, default=0)
    sl.add_argument("--strict", action="store_true", help="不把'已在线'当作成功")
    sl.set_defaults(func=cmd_login)

    so = sub.add_parser("logout", parents=[common], help="下线")
    so.add_argument("-u", "--user", default="")
    so.add_argument("--ip", default="")
    so.set_defaults(func=cmd_logout)

    ss = sub.add_parser("status", parents=[common], help="在线信息与外网连通")
    ss.set_defaults(func=cmd_status)

    sd = sub.add_parser("daemon", parents=[common], help="守护：断网自动重连")
    sd.add_argument("-u", "--user", default="")
    sd.add_argument("-p", "--password", default="")
    sd.add_argument("--interval", type=int, default=0)
    sd.add_argument("--hmd5-mode", choices=["hmac", "concat"], default="hmac")
    sd.add_argument("--once", action="store_true", help="只检查/登录一次后退出")
    sd.set_defaults(func=cmd_daemon)
    return p


def main(argv=None) -> int:
    args = build_parser().parse_args(argv)
    cfg = load_config(args.config) if args.config else load_config(CONFIG_NAME)
    setup_logging(getattr(args, "verbose", False), getattr(args, "log", None))
    if args.portal is None:
        args.portal = str(cfg.get("portal", DEFAULT_PORTAL) or DEFAULT_PORTAL)
    if args.ac_id is None:
        args.ac_id = str(cfg.get("ac_id", DEFAULT_AC_ID) or DEFAULT_AC_ID)
    if not getattr(args, "user", None):
        args.user = os.environ.get("SRUN_USER", "")
    if not getattr(args, "password", None):
        args.password = os.environ.get("SRUN_PASS", "")
    try:
        return args.func(args, cfg)
    except RuntimeError as e:
        print(f"[!] {e}")
        return 3
    except KeyboardInterrupt:
        return 130


if __name__ == "__main__":
    sys.exit(main())
