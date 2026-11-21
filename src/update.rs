use std::{io::IsTerminal, path::PathBuf, time::Duration};

use anyhow::{Context, Result, anyhow, bail};
use serde_json::Value;

const RELEASES_API: &str = "https://api.github.com/repos/DorskFR/shop/releases?per_page=20";
const UA: &str = concat!("shop/", env!("CARGO_PKG_VERSION"));
const CURRENT: &str = env!("CARGO_PKG_VERSION");
const DAY: u64 = 86_400;

fn agent(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(timeout))
        .build()
        .into()
}

fn asset_name() -> Result<String> {
    let os = match std::env::consts::OS {
        "linux" => "linux",
        "macos" => "darwin",
        other => bail!("no shop build is published for {other}"),
    };
    let arch = match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        other => bail!("no shop build is published for {other}"),
    };
    if os == "darwin" && arch == "amd64" {
        bail!("no shop build is published for macOS on Intel");
    }
    Ok(format!("shop-{os}-{arch}"))
}

pub fn parse_ver(s: &str) -> Option<(u64, u64, u64)> {
    let s = s.trim().trim_start_matches('v');
    let mut it = s.split(['.', '-', '+']);
    let major = it.next()?.parse::<u64>().ok()?;
    let mut rest = it.map(|p| p.parse::<u64>().unwrap_or(0));
    Some((major, rest.next().unwrap_or(0), rest.next().unwrap_or(0)))
}

pub fn find_sum(sums: &str, asset: &str) -> Option<String> {
    sums.lines().find_map(|l| {
        let (sum, file) = l.split_once("  ").or_else(|| l.split_once(' '))?;
        (file.trim().trim_start_matches('*') == asset).then(|| sum.trim().to_lowercase())
    })
}

pub fn newest_release(releases: &Value) -> Option<&Value> {
    releases
        .as_array()?
        .iter()
        .filter(|r| !r["draft"].as_bool().unwrap_or(false))
        .filter_map(|r| parse_ver(r["tag_name"].as_str()?).map(|v| (v, r)))
        .max_by_key(|(v, _)| *v)
        .map(|(_, r)| r)
}

fn fetch_latest(timeout: Duration) -> Result<Value> {
    let mut resp = agent(timeout)
        .get(RELEASES_API)
        .header("User-Agent", UA)
        .header("Accept", "application/vnd.github+json")
        .call()
        .context("querying GitHub releases")?;
    let status = resp.status().as_u16();
    let body = resp.body_mut().read_to_string()?;
    if status >= 400 {
        bail!("GitHub API HTTP {status}: {body}");
    }
    let releases: Value = serde_json::from_str(&body).context("parsing releases JSON")?;
    newest_release(&releases)
        .cloned()
        .ok_or_else(|| anyhow!("no release found"))
}

fn download(url: &str) -> Result<Vec<u8>> {
    let mut resp = agent(Duration::from_secs(120))
        .get(url)
        .header("User-Agent", UA)
        .call()
        .with_context(|| format!("downloading {url}"))?;
    let status = resp.status().as_u16();
    if status >= 400 {
        bail!("downloading {url}: HTTP {status}");
    }
    resp.body_mut()
        .with_config()
        .limit(200 * 1024 * 1024)
        .read_to_vec()
        .context("reading download")
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

pub fn run(check_only: bool) -> Result<()> {
    let want = asset_name()?;
    let rel = fetch_latest(Duration::from_secs(15))?;
    let tag = rel["tag_name"]
        .as_str()
        .ok_or_else(|| anyhow!("release has no tag_name"))?;
    if parse_ver(tag) <= parse_ver(CURRENT) {
        println!("shop is up to date (v{CURRENT})");
        return Ok(());
    }
    if check_only {
        println!("shop {tag} is available (installed: v{CURRENT}); run `shop update`");
        return Ok(());
    }
    eprintln!("updating shop v{CURRENT} -> {tag} ({want})");

    let assets = rel["assets"].as_array().cloned().unwrap_or_default();
    let url_of = |name: &str| {
        assets
            .iter()
            .find(|a| a["name"].as_str() == Some(name))
            .and_then(|a| a["browser_download_url"].as_str())
            .map(str::to_string)
    };
    let bin_url = url_of(&want).ok_or_else(|| anyhow!("release {tag} has no asset {want}"))?;
    let sums_url =
        url_of("SHA256SUMS").ok_or_else(|| anyhow!("release {tag} has no SHA256SUMS"))?;

    let bytes = download(&bin_url)?;
    let sums = String::from_utf8(download(&sums_url)?).context("SHA256SUMS is not UTF-8")?;
    let expected = find_sum(&sums, &want).ok_or_else(|| anyhow!("no checksum for {want}"))?;
    let actual = sha256_hex(&bytes);
    if actual != expected {
        bail!("checksum mismatch for {want}: expected {expected}, got {actual}");
    }

    let exe = std::env::current_exe().context("locating the running binary")?;
    let exe = std::fs::canonicalize(&exe).unwrap_or(exe);
    let dir = exe
        .parent()
        .ok_or_else(|| anyhow!("binary has no parent directory"))?;
    let tmp = dir.join(format!(".shop-update-{}", std::process::id()));
    std::fs::write(&tmp, &bytes)
        .with_context(|| format!("writing {} (no write access?)", tmp.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))?;
    }
    if let Err(e) = std::fs::rename(&tmp, &exe) {
        std::fs::remove_file(&tmp).ok();
        return Err(e).with_context(|| format!("replacing {}", exe.display()));
    }
    println!("updated to {tag}");
    Ok(())
}

fn cache_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))?;
    Some(base.join("shop").join("update-check.json"))
}

pub fn notice() {
    if std::env::var("SHOP_NO_UPDATE_CHECK").is_ok_and(|v| v == "1")
        || !std::io::stderr().is_terminal()
    {
        return;
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let cache = cache_path();
    let cached: Value = cache
        .as_ref()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(Value::Null);
    let latest = if now.saturating_sub(cached["checked"].as_u64().unwrap_or(0)) < DAY {
        cached["latest"].as_str().map(str::to_string)
    } else {
        let tag = fetch_latest(Duration::from_secs(3))
            .ok()
            .and_then(|r| r["tag_name"].as_str().map(str::to_string));
        if let Some(p) = &cache {
            if let Some(dir) = p.parent() {
                std::fs::create_dir_all(dir).ok();
            }
            let entry = serde_json::json!({ "checked": now, "latest": tag });
            std::fs::write(p, entry.to_string()).ok();
        }
        tag
    };
    if let Some(t) = latest
        && parse_ver(&t) > parse_ver(CURRENT)
    {
        let dim = anstyle::Style::new().dimmed();
        anstream::eprintln!(
            "{dim}shop {t} is available (installed: v{CURRENT}); run `shop update`{dim:#}"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert_eq!(parse_ver("v0.3.0"), Some((0, 3, 0)));
        assert_eq!(parse_ver("1.2"), Some((1, 2, 0)));
        assert_eq!(parse_ver("latest"), None);
        assert!(parse_ver("v0.10.0") > parse_ver("v0.9.9"));
    }

    #[test]
    fn checksums() {
        let sums = "ABC  shop-linux-amd64\ndef *shop-darwin-arm64\n";
        assert_eq!(find_sum(sums, "shop-linux-amd64").as_deref(), Some("abc"));
        assert_eq!(find_sum(sums, "shop-darwin-arm64").as_deref(), Some("def"));
        assert_eq!(find_sum(sums, "shop-linux-arm64"), None);
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn newest_skips_rolling_and_drafts() {
        let releases = serde_json::json!([
            {"tag_name": "latest"},
            {"tag_name": "v0.10.0", "draft": true},
            {"tag_name": "v0.3.0"},
            {"tag_name": "v0.9.1"},
        ]);
        assert_eq!(newest_release(&releases).unwrap()["tag_name"], "v0.9.1");
    }
}
