//! 深澜 SRun 认证客户端（吉林农业大学 http://202.198.1.80）

use std::time::Duration;

use serde_json::Value;

use crate::crypt::{build_chksum, build_info, hmac_md5_hex, md5_concat_hex};

const CGI: &str = "/cgi-bin";
const UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 \
                  (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";
const N_PARAM: &str = "200";
const TYPE_PARAM: &str = "1";

#[derive(Debug, Clone)]
pub struct Challenge {
    pub token: String,
    pub client_ip: String,
}

#[derive(Debug, Clone, Default)]
pub struct OnlineInfo {
    pub online: bool,
    pub user_name: String,
    pub online_ip: String,
    pub product: String,
    pub sum_seconds: i64,
    pub bytes_in: i64,
    pub bytes_out: i64,
    pub balance: f64,
}

#[derive(Debug, Clone, Default)]
pub struct LoginOutcome {
    pub ok: bool,
    pub error: String,
    pub error_msg: String,
    pub message: String,
    pub ip: String,
}

impl LoginOutcome {
    pub fn describe(&self) -> String {
        if self.ok {
            return self.message.clone();
        }
        let key = if !self.error_msg.is_empty() {
            self.error_msg.as_str()
        } else {
            self.error.as_str()
        };
        error_text(key)
    }
}

/// 服务端错误码 -> 中文说明
pub fn error_text(key: &str) -> String {
    let zh = match key {
        "ok" | "" => "成功",
        "login_error" => "登录失败",
        "logout_error" => "下线失败",
        "password_error" | "E2901" => "账号或密码错误",
        "sign_error" | "chksum_error" => "校验失败（加密链路不匹配）",
        "info_error" => "info 解密失败",
        "ip_already_online_error" | "E2620" => "该 IP 已在线",
        "not_online_error" => "该 IP 当前不在线",
        "user_not_found" => "用户不存在",
        "no_response_data_error" => "AC/Radius 无响应（多为该 IP 已在线）",
        "missing_required_parameters_error" => "缺少必要参数",
        "E2902" => "账号已欠费",
        "E2903" => "账号已停机",
        "E2904" => "账号被禁用",
        "E2553" => "密码错误次数过多，账号被锁",
        "E2610" => "在线设备数超限",
        "E2606" => "IP 已被其他账号绑定",
        other => return format!("未知错误: {other}"),
    };
    zh.to_string()
}

fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// 解析 `cb({...})` 形式的 JSONP
fn parse_jsonp(text: &str) -> Result<Value, String> {
    let start = text.find('{').ok_or_else(|| format!("响应不是 JSONP: {text:.120}"))?;
    let end = text.rfind('}').ok_or_else(|| format!("响应不是 JSONP: {text:.120}"))?;
    if end < start {
        return Err(format!("响应不是 JSONP: {text:.120}"));
    }
    serde_json::from_str(&text[start..=end]).map_err(|e| format!("JSON 解析失败: {e}"))
}

pub struct SrunClient {
    pub portal: String,
    pub ac_id: String,
    agent: ureq::Agent,
}

impl SrunClient {
    pub fn new(portal: &str, ac_id: &str, timeout_secs: u64) -> Self {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(timeout_secs)))
            .build()
            .into();
        Self {
            portal: portal.trim_end_matches('/').to_string(),
            ac_id: ac_id.to_string(),
            agent,
        }
    }

    fn get(&self, path: &str, query: &[(&str, String)]) -> Result<String, String> {
        let mut url = format!("{}{}", self.portal, path);
        let mut first = true;
        for (k, v) in query {
            url.push(if first { '?' } else { '&' });
            first = false;
            url.push_str(k);
            url.push('=');
            url.push_str(&urlencode(v));
        }
        let mut resp = self
            .agent
            .get(&url)
            .header("User-Agent", UA)
            .header("Accept", "*/*")
            .call()
            .map_err(|e| format!("请求失败: {e}"))?;
        resp.body_mut()
            .read_to_string()
            .map_err(|e| format!("读取响应失败: {e}"))
    }

    pub fn get_challenge(&self, username: &str) -> Result<Challenge, String> {
        let body = self.get(
            &format!("{CGI}/get_challenge"),
            &[("username", username.to_string()), ("ip", String::new()), ("callback", "cb".into())],
        )?;
        let v = parse_jsonp(&body)?;
        let token = v.get("challenge").and_then(|x| x.as_str()).unwrap_or("").to_string();
        let client_ip = v
            .get("client_ip")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        if token.is_empty() {
            let key = v.get("error").and_then(|x| x.as_str()).unwrap_or("challenge_error");
            return Err(error_text(key));
        }
        Ok(Challenge { token, client_ip })
    }

    pub fn online_info(&self) -> Option<OnlineInfo> {
        let body = self
            .get(&format!("{CGI}/rad_user_info"), &[("callback", "cb".into())])
            .ok()?;
        let v = parse_jsonp(&body).ok()?;
        if v.get("error").and_then(|x| x.as_str()) != Some("ok") {
            return Some(OnlineInfo::default());
        }
        Some(OnlineInfo {
            online: true,
            user_name: v.get("user_name").and_then(|x| x.as_str()).unwrap_or("").into(),
            online_ip: v.get("online_ip").and_then(|x| x.as_str()).unwrap_or("").into(),
            product: v.get("products_name").and_then(|x| x.as_str()).unwrap_or("").into(),
            sum_seconds: v.get("sum_seconds").and_then(|x| x.as_i64()).unwrap_or(0),
            bytes_in: v.get("bytes_in").and_then(|x| x.as_i64()).unwrap_or(0),
            bytes_out: v.get("bytes_out").and_then(|x| x.as_i64()).unwrap_or(0),
            balance: v.get("user_balance").and_then(|x| x.as_f64()).unwrap_or(0.0),
        })
    }

    #[allow(dead_code)]
    pub fn is_online(&self) -> bool {
        self.online_info().map(|i| i.online).unwrap_or(false)
    }

    /// 登录；`compat_md5` 为 true 时使用 md5(password+token) 变体。
    pub fn login(&self, username: &str, password: &str, compat_md5: bool) -> Result<LoginOutcome, String> {
        let ch = self.get_challenge(username)?;
        let token = ch.token;
        let ip = ch.client_ip;
        if ip.is_empty() {
            return Err("未取到本机校园网 IP（是否已连上校园网？）".into());
        }

        let hmd5 = if compat_md5 {
            md5_concat_hex(password, &token)
        } else {
            hmac_md5_hex(&token, password)
        };
        let info = build_info(username, password, &ip, &self.ac_id, &token);
        let chksum = build_chksum(&token, username, &hmd5, &self.ac_id, &ip, N_PARAM, TYPE_PARAM, &info);

        let body = self.get(
            &format!("{CGI}/srun_portal"),
            &[
                ("action", "login".into()),
                ("username", username.to_string()),
                ("password", format!("{{MD5}}{hmd5}")),
                ("ac_id", self.ac_id.clone()),
                ("ip", ip.clone()),
                ("chksum", chksum),
                ("info", info),
                ("n", N_PARAM.into()),
                ("type", TYPE_PARAM.into()),
                ("os", "Windows 10".into()),
                ("name", "Windows".into()),
                ("double_stack", "0".into()),
                ("callback", "cb".into()),
            ],
        )?;

        let v = parse_jsonp(&body)?;
        let error = v.get("error").and_then(|x| x.as_str()).unwrap_or("").to_string();
        let error_msg = v.get("error_msg").and_then(|x| x.as_str()).unwrap_or("").to_string();
        let suc_msg = v.get("suc_msg").and_then(|x| x.as_str()).unwrap_or("").to_string();
        let ploy_msg = v.get("ploy_msg").and_then(|x| x.as_str()).unwrap_or("").to_string();

        if error.is_empty() || error == "ok" {
            if suc_msg.contains("already") {
                return Ok(LoginOutcome {
                    ok: true,
                    error: suc_msg.clone(),
                    message: "该 IP 已在线".into(),
                    ip,
                    ..Default::default()
                });
            }
            let message = if ploy_msg.is_empty() || ploy_msg.starts_with("E0000") {
                "登录成功".to_string()
            } else {
                ploy_msg
            };
            return Ok(LoginOutcome {
                ok: true,
                error,
                message,
                ip,
                ..Default::default()
            });
        }

        // 该 IP 已有会话时，AC 可能不回数据；回查在线状态判定
        if (error_msg == "no_response_data_error" || error == "login_error")
            && let Some(info) = self.online_info()
            && info.online
            && (info.online_ip == ip || info.online_ip.is_empty())
        {
            return Ok(LoginOutcome {
                ok: true,
                error,
                error_msg,
                message: format!("该 IP 已有在线会话（{}）", info.user_name),
                ip,
            });
        }

        Ok(LoginOutcome {
            ok: false,
            error,
            error_msg,
            message: String::new(),
            ip,
        })
    }

    pub fn logout(&self, username: &str) -> Result<String, String> {
        let ip = self.online_info().map(|i| i.online_ip).unwrap_or_default();
        let body = self.get(
            &format!("{CGI}/srun_portal"),
            &[
                ("action", "logout".into()),
                ("ac_id", self.ac_id.clone()),
                ("ip", ip),
                ("username", username.to_string()),
                ("callback", "cb".into()),
            ],
        )?;
        let v = parse_jsonp(&body)?;
        let error = v.get("error").and_then(|x| x.as_str()).unwrap_or("").to_string();
        if error.is_empty() || error == "ok" {
            Ok("已下线".into())
        } else {
            Err(error_text(&error))
        }
    }
}

/// 外网连通性探测（多目标，任一成功即在线）
pub fn internet_ok(timeout_secs: u64) -> bool {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(timeout_secs)))
        .build()
        .into();
    const PROBES: [&str; 3] = [
        "http://connect.rom.miui.com/generate_204",
        "http://www.baidu.com/favicon.ico",
        "http://captive.apple.com/hotspot-detect.html",
    ];
    for url in PROBES {
        match agent.get(url).header("User-Agent", UA).call() {
            Ok(mut resp) => {
                let status = resp.status().as_u16();
                if status >= 400 {
                    continue;
                }
                // 门户劫持常返回 200 + 登录页
                if let Ok(body) = resp.body_mut().read_to_string() {
                    let lower = body.to_lowercase();
                    if lower.contains("<html") && (lower.contains("login") || body.contains("认证")) {
                        continue;
                    }
                }
                return true;
            }
            Err(_) => continue,
        }
    }
    false
}
