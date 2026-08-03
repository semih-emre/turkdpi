use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::net::UdpSocket;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

const NMCLI: &str = "/usr/bin/nmcli";
const DNS_V4: &str = "1.1.1.1,1.0.0.1";
const DNS_V6: &str = "2606:4700:4700::1111,2606:4700:4700::1001";
const DNS_QUERY: &[u8] = &[
    0x54, 0x44, 0x01, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x07, b'e',
    b'x', b'a', b'm', b'p', b'l', b'e', 0x03, b'c', b'o', b'm', 0x00, 0x00, 0x01, 0x00, 0x01,
];

#[derive(Debug)]
pub struct ActiveConnection {
    pub uuid: String,
    pub device: String,
}

#[derive(Deserialize, Serialize)]
struct DnsBackup {
    ipv4_dns: String,
    ipv4_ignore_auto_dns: String,
    ipv6_dns: String,
    ipv6_ignore_auto_dns: String,
}

fn validate_uuid(uuid: &str) -> Result<()> {
    if uuid.len() != 36 || !uuid.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
        bail!("NetworkManager bağlantı UUID'si geçersiz");
    }
    Ok(())
}

fn nmcli(args: &[&str]) -> Result<String> {
    let output = Command::new(NMCLI)
        .args(args)
        .output()
        .context("nmcli çalıştırılamadı")?;
    if !output.status.success() {
        bail!(
            "nmcli başarısız: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    String::from_utf8(output.stdout).context("nmcli UTF-8 olmayan çıktı döndürdü")
}

pub fn active_connection() -> Result<ActiveConnection> {
    let output = nmcli(&[
        "-t",
        "--escape",
        "no",
        "-f",
        "UUID,DEVICE,TYPE",
        "connection",
        "show",
        "--active",
    ])?;
    parse_active_connection(&output)
}

fn parse_active_connection(output: &str) -> Result<ActiveConnection> {
    for line in output.lines() {
        let mut fields = line.splitn(3, ':');
        let (Some(uuid), Some(device), Some(connection_type)) =
            (fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        if !matches!(
            connection_type,
            "802-3-ethernet" | "802-11-wireless" | "ethernet" | "wifi"
        ) {
            continue;
        }
        if device.is_empty() || device == "--" || device.len() > 15 || device.contains('\0') {
            continue;
        }
        if validate_uuid(uuid).is_ok() {
            return Ok(ActiveConnection {
                uuid: uuid.to_owned(),
                device: device.to_owned(),
            });
        }
    }
    bail!("etkin NetworkManager bağlantısı bulunamadı")
}

fn backup_path(state_dir: &Path, uuid: &str) -> PathBuf {
    state_dir.join("dns-backups").join(format!("{uuid}.json"))
}

fn marker_path(state_dir: &Path) -> PathBuf {
    state_dir.join("dns-connection")
}

fn cloudflare_dns_responds() -> bool {
    for server in ["1.1.1.1:53", "1.0.0.1:53"] {
        let Ok(socket) = UdpSocket::bind("0.0.0.0:0") else {
            continue;
        };
        let timeout = Some(Duration::from_millis(1500));
        if socket.set_read_timeout(timeout).is_err()
            || socket.set_write_timeout(timeout).is_err()
            || socket.connect(server).is_err()
            || socket.send(DNS_QUERY).is_err()
        {
            continue;
        }
        let mut response = [0_u8; 512];
        if let Ok(length) = socket.recv(&mut response) {
            if length >= 12
                && response[0..2] == DNS_QUERY[0..2]
                && response[2] & 0x80 != 0
            {
                return true;
            }
        }
    }
    false
}

fn read_current(uuid: &str) -> Result<DnsBackup> {
    validate_uuid(uuid)?;
    let property = |name: &str| -> Result<String> {
        let raw = nmcli(&["-g", name, "connection", "show", "uuid", uuid])?;
        Ok(raw
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && *line != "--")
            .collect::<Vec<_>>()
            .join(","))
    };
    let normalize_bool = |value: String| {
        if matches!(value.as_str(), "yes" | "true" | "1") {
            "yes".to_owned()
        } else {
            "no".to_owned()
        }
    };
    Ok(DnsBackup {
        ipv4_dns: property("ipv4.dns")?,
        ipv4_ignore_auto_dns: normalize_bool(property("ipv4.ignore-auto-dns")?),
        ipv6_dns: property("ipv6.dns")?,
        ipv6_ignore_auto_dns: normalize_bool(property("ipv6.ignore-auto-dns")?),
    })
}

fn modify(uuid: &str, values: &DnsBackup) -> Result<()> {
    validate_uuid(uuid)?;
    nmcli(&[
        "connection",
        "modify",
        "uuid",
        uuid,
        "ipv4.dns",
        &values.ipv4_dns,
        "ipv4.ignore-auto-dns",
        &values.ipv4_ignore_auto_dns,
    ])?;
    let ipv6_method = nmcli(&["-g", "ipv6.method", "connection", "show", "uuid", uuid])?;
    if !matches!(ipv6_method.trim(), "ignore" | "disabled") {
        nmcli(&[
            "connection",
            "modify",
            "uuid",
            uuid,
            "ipv6.dns",
            &values.ipv6_dns,
            "ipv6.ignore-auto-dns",
            &values.ipv6_ignore_auto_dns,
        ])?;
    }
    Ok(())
}

fn reapply_if_active(uuid: &str) -> Result<()> {
    if let Ok(active) = active_connection() {
        if active.uuid == uuid {
            nmcli(&["device", "reapply", &active.device])?;
        }
    }
    Ok(())
}

pub fn apply_cloudflare(state_dir: &Path) -> Result<String> {
    if !cloudflare_dns_responds() {
        return Ok(
            "Cloudflare DNS bu ağda yanıt vermedi; mevcut otomatik DNS korundu".into(),
        );
    }
    let active = active_connection()?;
    let path = backup_path(state_dir, &active.uuid);
    if !path.exists() {
        let original = read_current(&active.uuid)?;
        fs::create_dir_all(path.parent().context("DNS yedek dizini yok")?)?;
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&path)?;
        serde_json::to_writer_pretty(&mut file, &original)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
    }

    let cloudflare = DnsBackup {
        ipv4_dns: DNS_V4.into(),
        ipv4_ignore_auto_dns: "yes".into(),
        ipv6_dns: DNS_V6.into(),
        ipv6_ignore_auto_dns: "yes".into(),
    };
    fs::create_dir_all(state_dir)?;
    fs::write(marker_path(state_dir), format!("{}\n", active.uuid))?;
    if let Err(error) =
        modify(&active.uuid, &cloudflare).and_then(|_| reapply_if_active(&active.uuid))
    {
        let _ = restore(state_dir);
        return Err(error);
    }
    Ok(format!("Cloudflare DNS etkin: {DNS_V4}"))
}

pub fn restore(state_dir: &Path) -> Result<String> {
    let marker = marker_path(state_dir);
    let Ok(raw_uuid) = fs::read_to_string(&marker) else {
        return Ok("DNS değişikliği yok".into());
    };
    let uuid = raw_uuid.trim();
    validate_uuid(uuid)?;
    let path = backup_path(state_dir, uuid);
    if !path.exists() {
        let _ = fs::remove_file(marker);
        return Ok("DNS yedeği bulunamadı; bağlantı değiştirilmedi".into());
    }
    let backup: DnsBackup = serde_json::from_slice(&fs::read(&path)?)?;
    modify(uuid, &backup)?;
    reapply_if_active(uuid)?;
    fs::remove_file(path)?;
    fs::remove_file(marker)?;
    Ok("Önceki DNS ayarları geri yüklendi".into())
}

pub fn is_cloudflare_active(state_dir: &Path) -> bool {
    marker_path(state_dir).exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ethernet_or_wifi_is_selected_instead_of_loopback() {
        let output = "11111111-1111-1111-1111-111111111111:lo:loopback\n22222222-2222-2222-2222-222222222222:wlan0:802-11-wireless\n";
        let active = parse_active_connection(output).expect("wifi connection");
        assert_eq!(active.uuid, "22222222-2222-2222-2222-222222222222");
        assert_eq!(active.device, "wlan0");
    }

    #[test]
    fn malformed_connection_is_rejected() {
        assert!(parse_active_connection("not-a-uuid:eth0:802-3-ethernet\n").is_err());
    }
}
