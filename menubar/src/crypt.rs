//! 深澜 SRun 加密原语：XXTEA(xEncode)、自定义字母表 base64、HMAC-MD5、SHA1。
//!
//! 与门户 `/static/js/jquery.srun.portal.js` 逐字节等价（含中文按 UTF-16 charCode 处理）。

use hmac::{Hmac, KeyInit, Mac};
use md5::Md5;
use sha1::{Digest, Sha1};

type HmacMd5 = Hmac<Md5>;

/// 深澜自定义 base64 字母表
pub const SRUN_ALPHA: &[u8; 64] =
    b"LVoJPiCN2R8G90yg+hmFHuacZ1OWMnrsSTXkYpUq/3dlbfKwv6xztjI7DeBE45QA";

/// JS 的 `s(a, b)`：每 4 个 charCode 组成一个 32bit 小端整数。
fn str2ints(s: &str, include_len: bool) -> Vec<u32> {
    // JS charCodeAt 是 UTF-16 code unit，用 encode_utf16 精确对应
    let units: Vec<u16> = s.encode_utf16().collect();
    let mut v = Vec::with_capacity(units.len() / 4 + 2);
    let mut i = 0;
    while i < units.len() {
        let mut val: u32 = 0;
        for j in 0..4 {
            if let Some(u) = units.get(i + j) {
                val |= (*u as u32) << (8 * j);
            }
        }
        v.push(val);
        i += 4;
    }
    if include_len {
        v.push(units.len() as u32);
    }
    v
}

fn ints2bytes(v: &[u32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 4);
    for x in v {
        out.extend_from_slice(&x.to_le_bytes());
    }
    out
}

/// 深澜 xEncode = XXTEA（delta 0x9E3779B9，32 位无符号回绕）。
#[allow(unused_assignments)]
pub fn xencode(data: &str, key: &str) -> Vec<u8> {
    if data.is_empty() {
        return Vec::new();
    }
    let mut v = str2ints(data, true);
    let mut k = str2ints(key, false);
    while k.len() < 4 {
        k.push(0);
    }

    let n = v.len() - 1;
    let mut z = v[n];
    let mut y = v[0];
    let mut d: u32 = 0;
    const C: u32 = 0x9E37_79B9;
    let mut q: u32 = 6 + 52 / (n as u32 + 1);

    while q > 0 {
        q -= 1;
        d = d.wrapping_add(C);
        let e = (d >> 2) & 3;
        let mut p = 0usize;
        while p < n {
            y = v[p + 1];
            let mut m = (z >> 5) ^ y.wrapping_shl(2);
            m = m.wrapping_add(((y >> 3) ^ z.wrapping_shl(4)) ^ d ^ y);
            m = m.wrapping_add(k[(p & 3) ^ e as usize] ^ z);
            z = v[p].wrapping_add(m);
            v[p] = z;
            p += 1;
        }
        y = v[0];
        let mut m = (z >> 5) ^ y.wrapping_shl(2);
        m = m.wrapping_add(((y >> 3) ^ z.wrapping_shl(4)) ^ d ^ y);
        m = m.wrapping_add(k[(n & 3) ^ e as usize] ^ z);
        z = v[n].wrapping_add(m);
        v[n] = z;
    }

    ints2bytes(&v)
}

/// 标准字母表 base64（自助服务平台 SSO 的 `data` 参数用这个）。
pub fn base64_standard(data: &[u8]) -> String {
    const ALPHA: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let n = ((chunk[0] as u32) << 16)
            | ((*chunk.get(1).unwrap_or(&0) as u32) << 8)
            | (*chunk.get(2).unwrap_or(&0) as u32);
        out.push(ALPHA[((n >> 18) & 63) as usize] as char);
        out.push(ALPHA[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            ALPHA[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHA[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

/// 明文 MD5（小写十六进制），自助服务 SSO 的密码摘要。
pub fn md5_hex(input: &str) -> String {
    let mut hasher = Md5::new();
    hasher.update(input.as_bytes());
    hex(&hasher.finalize())
}

/// 深澜自定义字母表 base64（仅换表，填充符仍是 `=`）。
pub fn srun_base64(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(SRUN_ALPHA[((n >> 18) & 63) as usize] as char);
        out.push(SRUN_ALPHA[((n >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            out.push(SRUN_ALPHA[((n >> 6) & 63) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(SRUN_ALPHA[(n & 63) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

fn hex(bytes: &[u8]) -> String {
    const H: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(H[(b >> 4) as usize] as char);
        s.push(H[(b & 0x0f) as usize] as char);
    }
    s
}

/// js-md5 的 `md5(password, token)` 语义 —— 实测为 HMAC-MD5(key=token, msg=password)。
pub fn hmac_md5_hex(key: &str, msg: &str) -> String {
    let mut mac = HmacMd5::new_from_slice(key.as_bytes()).expect("HMAC 接受任意长度密钥");
    mac.update(msg.as_bytes());
    hex(&mac.finalize().into_bytes())
}

/// 部分深澜版本的兼容写法：md5(password + token)
pub fn md5_concat_hex(password: &str, token: &str) -> String {
    let mut hasher = Md5::new();
    hasher.update(password.as_bytes());
    hasher.update(token.as_bytes());
    hex(&hasher.finalize())
}

pub fn sha1_hex(input: &str) -> String {
    let mut hasher = Sha1::new();
    hasher.update(input.as_bytes());
    hex(&hasher.finalize())
}

/// `info` 字段：`{SRBX1}` + srun_base64(xEncode(JSON, token))
pub fn build_info(username: &str, password: &str, ip: &str, acid: &str, token: &str) -> String {
    let payload = serde_json::json!({
        "username": username,
        "password": password,
        "ip": ip,
        "acid": acid,
        "enc_ver": "srun_bx1",
    });
    // JS 的 JSON.stringify 无多余空格
    let js = serde_json::to_string(&payload).unwrap_or_default();
    format!("{{SRBX1}}{}", srun_base64(&xencode(&js, token)))
}

/// `chksum` 字段
#[allow(clippy::too_many_arguments)]
pub fn build_chksum(
    token: &str,
    username: &str,
    hmd5: &str,
    acid: &str,
    ip: &str,
    n: &str,
    ty: &str,
    info: &str,
) -> String {
    let raw = format!(
        "{t}{u}{t}{h}{t}{a}{t}{ip}{t}{n}{t}{ty}{t}{info}",
        t = token,
        u = username,
        h = hmd5,
        a = acid,
        ip = ip,
        n = n,
        ty = ty,
        info = info
    );
    sha1_hex(&raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hmac_matches_portal_js() {
        // node 加载门户 all.min.js 得到: md5('secret','token') = 1c618aed...
        assert_eq!(
            hmac_md5_hex("token", "secret"),
            "1c618aedf7ce9649a7f3c059475fb320"
        );
    }

    #[test]
    fn info_matches_browser_bytes() {
        // 与浏览器 xEncode + 自定义 base64 逐字节一致（ASCII 载荷）
        let payload = r#"{"username":"20261794","password":"p@ss1234","ip":"10.7.224.192","acid":"4","enc_ver":"srun_bx1"}"#;
        let got = srun_base64(&xencode(payload, "testtoken0123456789abcdef"));
        assert_eq!(
            got,
            "Yi+Pp9tKgrTSCH4Hsse0kDYzJ1sCRF8y1r/xTI2FqqpX5m54i5sUGOVSrm7ddfY1lCDauCCvjJHfPrcsVukv4PXE51K+ufUGAEu14Xd+BRBi+Eqxmv2evPj8FplgJbg7k4HnmntXYt9="
        );
    }

    #[test]
    fn sha1_hex_is_lowercase() {
        assert_eq!(sha1_hex("abc"), "a9993e364706816aba3e25717850c26c9cd0d89d");
    }
}
