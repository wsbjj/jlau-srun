//! 自助服务平台（门户 `:8800`）：免验证码 SSO 登录 + 「在线信息」解析
//!
//! 门户页上的「自助服务」按钮走的就是这条链路（`static/js/main.js`）：
//!
//! ```text
//! base64 字母表切成标准表后
//!   data = base64("zh-CN:<账号>:<md5(密码)>")
//!   GET  {selfServiceIp}/site/sso?data=...
//! ```
//!
//! SSO 成功会 302 到 `/home` 并下发新的 `PHPSESSID_*`，随后首页里的
//! 「在线信息」表格就是该账号全部在线会话：用户名 / IP / IPv6 / 上线时间 /
//! 已用流量 / 已用时长 / MAC / 登录类型。账号下多台设备时会有多行。

use std::time::Duration;

use crate::crypt::{base64_standard, md5_hex};
use crate::srun::urlencode;

const UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 \
                  (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";
const SSO_LANG: &str = "zh-CN";
/// 门户 JS 里的 `selfServiceIp = location.hostname + ':8800'`
const SERVICE_PORT: u16 = 8800;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Device {
    pub user_name: String,
    pub ip: String,
    pub ipv6: String,
    /// 上线时间，服务端原样输出：`2026-11-19 17:32:22`
    pub online_at: String,
    pub traffic: String,
    pub duration: String,
    pub mac: String,
    pub login_type: String,
}

impl Device {
    /// 菜单里的一行：IP · 上线时间 · MAC
    pub fn summary(&self) -> String {
        let when = short_time(&self.online_at);
        format!("{} · {} · {}", self.ip, when, self.mac)
    }
}

/// `2026-11-19 17:32:22` -> `11-19 17:32`；格式不认识就原样返回。
fn short_time(raw: &str) -> String {
    let parts: Vec<&str> = raw.split_whitespace().collect();
    if parts.len() != 2 {
        return raw.to_string();
    }
    let (date, clock) = (parts[0], parts[1]);
    let date = date.split_once('-').map(|(_, md)| md).unwrap_or(date);
    let clock = clock.rsplit_once(':').map(|(hm, _)| hm).unwrap_or(clock);
    format!("{date} {clock}")
}

/// 门户地址推导自助服务地址：`http://202.198.1.80` -> `http://202.198.1.80:8800`
pub fn base_url(portal: &str) -> String {
    let portal = portal.trim().trim_end_matches('/');
    let (scheme, rest) = portal.split_once("://").unwrap_or(("http", portal));
    let host_port = rest.split('/').next().unwrap_or(rest);
    let host = match host_port.rsplit_once(':') {
        Some((host, port)) if !host.is_empty() && port.chars().all(|c| c.is_ascii_digit()) => host,
        _ => host_port,
    };
    format!("{scheme}://{host}:{SERVICE_PORT}")
}

pub fn home_url(base: &str) -> String {
    format!("{}/home", base.trim_end_matches('/'))
}

/// 与门户 `$('#self-service')` 点击逻辑等价。
pub fn sso_url(base: &str, user: &str, password: &str) -> String {
    let payload = format!("{SSO_LANG}:{user}:{}", md5_hex(password));
    let data = base64_standard(payload.as_bytes());
    format!(
        "{}/site/sso?data={}",
        base.trim_end_matches('/'),
        urlencode(&data)
    )
}

/// 登录自助服务并解析「在线信息」。
pub fn fetch_devices(base: &str, user: &str, password: &str) -> Result<Vec<Device>, String> {
    if user.trim().is_empty() {
        return Err("未配置账号".into());
    }
    if password.is_empty() {
        return Err("未配置密码".into());
    }

    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(12)))
        .http_status_as_error(false)
        // 手动跟 SSO 的 302，才能拿到新会话 Cookie
        .max_redirects(0)
        .build()
        .into();

    let mut sso = agent
        .get(&sso_url(base, user, password))
        .header("User-Agent", UA)
        .header("Accept", "*/*")
        .call()
        .map_err(|e| format!("连不上自助服务平台: {e}"))?;
    let status = sso.status().as_u16();
    if status >= 400 {
        return Err(format!("自助服务 SSO 返回 HTTP {status}"));
    }
    // 响应可能带多个 PHPSESSID，后下发的那个才是登录后的会话
    let cookie = sso
        .headers()
        .get_all("set-cookie")
        .iter()
        .filter_map(|value| value.to_str().ok())
        .filter_map(|value| value.split(';').next())
        .filter(|pair| pair.to_ascii_lowercase().starts_with("phpsessid"))
        .map(str::to_string)
        .next_back()
        .ok_or_else(|| "SSO 没有下发会话 Cookie（账号或密码可能不对）".to_string())?;
    // 把 302 的空 body 读掉，连接可以复用
    let _ = sso.body_mut().read_to_string();

    let mut resp = agent
        .get(&home_url(base))
        .header("User-Agent", UA)
        .header("Cookie", &cookie)
        .call()
        .map_err(|e| format!("打开自助服务首页失败: {e}"))?;
    let status = resp.status().as_u16();
    if status != 200 {
        return Err(format!("自助服务首页返回 HTTP {status}"));
    }
    let body = resp
        .body_mut()
        .read_to_string()
        .map_err(|e| format!("读取自助服务页面失败: {e}"))?;
    parse_devices(&body)
}

/// 从 `/home` HTML 里抠出「在线信息」表格的每一行。
fn parse_devices(html: &str) -> Result<Vec<Device>, String> {
    let Some(section) = html.find("在线信息") else {
        if html.contains("login-form") {
            return Err("自助服务登录态无效，检查账号/密码".into());
        }
        return Err("页面中没有「在线信息」段落".into());
    };
    let tail = &html[section..];
    let Some(start) = tail.find("<table") else {
        return Err("「在线信息」表格缺失".into());
    };
    let table = &tail[start..];
    let table = match table.find("</table>") {
        Some(end) => &table[..end],
        None => table,
    };

    let mut devices = Vec::new();
    let mut rest = table;
    while let Some(pos) = rest.find("<tr") {
        rest = &rest[pos + 3..];
        let row = match rest.find("</tr>") {
            Some(end) => {
                let row = &rest[..end];
                rest = &rest[end..];
                row
            }
            None => std::mem::take(&mut rest),
        };
        let cells = cells_of(row);
        if cells.len() < 7 {
            // 表头是 <th>，没有 <td>，正好跳过
            continue;
        }
        devices.push(Device {
            user_name: cells[0].clone(),
            ip: cells[1].clone(),
            ipv6: cells[2].clone(),
            online_at: cells[3].clone(),
            traffic: cells[4].clone(),
            duration: cells[5].clone(),
            mac: cells[6].clone(),
            login_type: cells.get(7).cloned().unwrap_or_default(),
        });
    }
    Ok(devices)
}

fn cells_of(row: &str) -> Vec<String> {
    let mut cells = Vec::new();
    let mut rest = row;
    while let Some(pos) = rest.find("<td") {
        rest = &rest[pos + 3..];
        let Some(gt) = rest.find('>') else { break };
        let cell = &rest[gt + 1..];
        let Some(end) = cell.find("</td>") else { break };
        cells.push(clean_text(&cell[..end]));
        rest = &cell[end + 5..];
    }
    cells
}

fn clean_text(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut in_tag = false;
    for ch in raw.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    let out = out
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'");
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<div class="panel panel-default">
    <div class="panel-heading"><span class="glyphicon glyphicon-th-large"></span>
        在线信息</div>
    <div id="w1" class="grid-view"><table class="kv-grid-table table">
    <thead>
    <tr><th data-col-seq="0">用户名</th><th data-col-seq="1">IP地址</th><th data-col-seq="2">IPV6地址</th><th data-col-seq="3">上线时间</th><th data-col-seq="4">已用流量</th><th data-col-seq="5">已用时长</th><th data-col-seq="9">MAC地址</th><th data-col-seq="11">登录类型</th></tr>
    </thead>
    <tbody>
    <tr data-key="35919244"><td data-col-seq="0">20261794</td><td data-col-seq="1">10.7.98.160</td><td data-col-seq="2">::</td><td data-col-seq="3">2026-11-19 17:32:22</td><td data-col-seq="4">171.642G</td><td data-col-seq="5">153时36分40秒</td><td data-col-seq="9">C2-F6-2A-BE-71-91</td><td data-col-seq="11">IPOE</td></tr>
    <tr data-key="35925829"><td data-col-seq="0">20261794</td><td data-col-seq="1">10.7.224.192</td><td data-col-seq="2">::</td><td data-col-seq="3">2026-11-19 19:19:47</td><td data-col-seq="4">171.642G</td><td data-col-seq="5">153时36分40秒</td><td data-col-seq="9">06-D4-86-CE-38-B5</td><td data-col-seq="11">IPOE</td></tr>
    </tbody></table></div>
    <div class="panel panel-default"><table><tr><td>产品信息</td></tr></table></div>
</div>"#;

    #[test]
    fn parses_online_device_table() {
        let devices = parse_devices(SAMPLE).unwrap();
        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0].ip, "10.7.98.160");
        assert_eq!(devices[0].mac, "C2-F6-2A-BE-71-91");
        assert_eq!(devices[0].online_at, "2026-11-19 17:32:22");
        assert_eq!(devices[0].login_type, "IPOE");
        assert_eq!(devices[1].ip, "10.7.224.192");
        assert_eq!(devices[1].mac, "06-D4-86-CE-38-B5");
    }

    #[test]
    fn empty_table_is_zero_devices() {
        let html = "<h4>在线信息</h4><table><tbody></tbody></table>";
        assert!(parse_devices(html).unwrap().is_empty());
    }

    #[test]
    fn login_page_is_reported() {
        let html = r#"<form id="login-form" action="/"></form>"#;
        assert!(parse_devices(html).unwrap_err().contains("登录态无效"));
    }

    #[test]
    fn derives_service_address_from_portal() {
        assert_eq!(base_url("http://202.198.1.80"), "http://202.198.1.80:8800");
        assert_eq!(base_url("http://202.198.1.80/"), "http://202.198.1.80:8800");
        assert_eq!(
            base_url("https://portal.example.edu:8080"),
            "https://portal.example.edu:8800"
        );
        assert_eq!(
            home_url("http://202.198.1.80:8800/"),
            "http://202.198.1.80:8800/home"
        );
    }

    #[test]
    fn sso_payload_matches_portal_script() {
        // md5("password") = 5f4dcc3b5aa765d61d8327deb882cf99
        // base64("zh-CN:alice:5f4dcc3b5aa765d61d8327deb882cf99")
        let url = sso_url("http://host:8800", "alice", "password");
        assert_eq!(
            url,
            "http://host:8800/site/sso?data=emgtQ046YWxpY2U6NWY0ZGNjM2I1YWE3NjVkNjFkODMyN2RlYjg4MmNmOTk%3D"
        );
    }

    #[test]
    fn device_summary_is_compact() {
        let device = Device {
            ip: "10.7.98.160".into(),
            mac: "C2-F6-2A-BE-71-91".into(),
            online_at: "2026-11-19 17:32:22".into(),
            ..Default::default()
        };
        assert_eq!(
            device.summary(),
            "10.7.98.160 · 11-19 17:32 · C2-F6-2A-BE-71-91"
        );
    }
}
