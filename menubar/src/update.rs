//! 对照 GitHub Release 检查是否有更新。

use std::cmp::Ordering;
use std::time::Duration;

const REPO: &str = "wsbjj/jlau-srun";
const USER_AGENT: &str = concat!("jlau-srun/", env!("CARGO_PKG_VERSION"));

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateReport {
    pub summary: String,
    pub latest: Option<String>,
    pub page: Option<String>,
    pub newer: bool,
}

pub fn check(current: &str) -> UpdateReport {
    match fetch_latest() {
        Ok(release) => {
            let newer = compare_versions(current, &release.version) == Ordering::Less;
            let summary = if newer {
                format!("发现新版本 {}，当前版本 {current}", release.version)
            } else {
                format!("已是最新版本 {current}")
            };
            UpdateReport {
                summary,
                latest: Some(release.version),
                page: Some(release.page),
                newer,
            }
        }
        Err(err) => UpdateReport {
            summary: format!("检查更新失败: {err}"),
            latest: None,
            page: None,
            newer: false,
        },
    }
}

struct Release {
    version: String,
    page: String,
}

fn fetch_latest() -> Result<Release, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(12)))
        .build()
        .into();
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let mut response = agent
        .get(&url)
        .header("User-Agent", USER_AGENT)
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|err| format!("请求失败: {err}"))?;
    let body = response
        .body_mut()
        .read_to_string()
        .map_err(|err| format!("读取响应失败: {err}"))?;
    let value: serde_json::Value = serde_json::from_str(&body).map_err(|err| format!("解析失败: {err}"))?;
    let tag = value
        .get("tag_name")
        .and_then(|item| item.as_str())
        .ok_or("响应缺少版本号")?;
    let page = value
        .get("html_url")
        .and_then(|item| item.as_str())
        .unwrap_or("https://github.com/wsbjj/jlau-srun/releases/latest")
        .to_string();
    Ok(Release {
        version: tag.trim().trim_start_matches('v').to_string(),
        page,
    })
}

pub fn compare_versions(current: &str, latest: &str) -> Ordering {
    let current = version_parts(current);
    let latest = version_parts(latest);
    let len = current.len().max(latest.len());
    for index in 0..len {
        let left = current.get(index).copied().unwrap_or(0);
        let right = latest.get(index).copied().unwrap_or(0);
        match left.cmp(&right) {
            Ordering::Equal => {}
            other => return other,
        }
    }
    Ordering::Equal
}

fn version_parts(raw: &str) -> Vec<u64> {
    raw.trim()
        .trim_start_matches('v')
        .split('.')
        .map(|part| {
            part.chars()
                .take_while(|ch| ch.is_ascii_digit())
                .collect::<String>()
                .parse()
                .unwrap_or(0)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::cmp::Ordering;

    use super::compare_versions;

    #[test]
    fn newer_patch_is_greater() {
        assert_eq!(compare_versions("1.0.1", "1.0.2"), Ordering::Less);
    }

    #[test]
    fn same_version_ignores_v_prefix() {
        assert_eq!(compare_versions("1.0.1", "v1.0.1"), Ordering::Equal);
    }

    #[test]
    fn older_release_is_not_an_update() {
        assert_eq!(compare_versions("1.0.1", "1.0.0"), Ordering::Greater);
    }

    #[test]
    fn missing_component_counts_as_zero() {
        assert_eq!(compare_versions("1.0", "1.0.1"), Ordering::Less);
    }
}
