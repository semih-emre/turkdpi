use anyhow::{bail, Context, Result};
use chrono::Utc;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::process::{Command, Stdio};

const RULESET: &str = r#"
table inet turkdpi {
  chain output {
    type filter hook output priority mangle; policy accept;
    meta l4proto tcp tcp dport { 80, 443 } queue num 200 bypass
    meta l4proto udp udp dport { 443, 50000-65535 } queue num 201 bypass
  }
}
"#;

fn run_nft(args: &[&str], stdin: Option<&[u8]>) -> Result<std::process::Output> {
    let mut child = Command::new("/usr/bin/nft")
        .args(args)
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("nft çalıştırılamadı")?;
    if let Some(data) = stdin {
        child
            .stdin
            .take()
            .context("nft stdin açılamadı")?
            .write_all(data)?;
    }
    Ok(child.wait_with_output()?)
}

pub fn backup_ruleset(state_dir: &Path) -> Result<()> {
    let backup_dir = state_dir.join("backups");
    fs::create_dir_all(&backup_dir)?;
    let out = run_nft(&["-j", "list", "ruleset"], None)?;
    if !out.status.success() {
        bail!(
            "nftables yedeği alınamadı: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let path = backup_dir.join(format!(
        "ruleset-{}.json",
        Utc::now().format("%Y%m%dT%H%M%S-%9fZ")
    ));
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(&out.stdout)?;
    Ok(())
}

pub fn apply() -> Result<()> {
    let check = run_nft(&["-c", "-f", "-"], Some(RULESET.as_bytes()))?;
    if !check.status.success() {
        bail!(
            "nft kural doğrulaması başarısız: {}",
            String::from_utf8_lossy(&check.stderr)
        );
    }
    let out = run_nft(&["-f", "-"], Some(RULESET.as_bytes()))?;
    if !out.status.success() {
        bail!(
            "nft kuralları uygulanamadı: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Ok(())
}

pub fn cleanup() -> Result<()> {
    let exists = run_nft(&["list", "table", "inet", "turkdpi"], None)?;
    if !exists.status.success() {
        return Ok(());
    }
    let out = run_nft(&["delete", "table", "inet", "turkdpi"], None)?;
    if !out.status.success() {
        bail!(
            "turkdpi tablosu silinemedi: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Ok(())
}
