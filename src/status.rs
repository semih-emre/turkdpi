//! GUI ile servis arasındaki sözleşme: `status.json`.
//!
//! Dosya atomik yazılıyor (geçici dosya + `rename`). GUI dosyayı her an
//! okuyabildiği için, yarım yazılmış bir JSON'u ayrıştırmaya çalışması hâlinde
//! arayüz boş görünürdü.

use crate::paths;
use crate::strategy::Strategy;
use crate::util;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::Write;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Status {
    pub active: bool,
    /// Uygulanan stratejinin kimliği ya da profil adı.
    pub profile: String,
    /// Kullanılan arka ucun okunabilir açıklaması.
    pub method: String,
    pub message: String,
    pub dns_cloudflare: bool,
    /// Arka uç kimliği: `nfqws`, `winws`, `ciadpi`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend: Option<String>,
    /// Kazanan strateji; GUI'nin ayrıntı göstermesi için.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strategy: Option<Strategy>,
    /// Teşhisin okunabilir özeti.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagnosis: Option<String>,
    /// Proxy modunda uygulamaların yönlendirilmesi gereken adres.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proxy: Option<String>,
    pub updated: String,
}

impl Status {
    pub fn idle(message: impl Into<String>) -> Self {
        Self {
            active: false,
            profile: "none".into(),
            method: "none".into(),
            message: message.into(),
            dns_cloudflare: false,
            backend: None,
            strategy: None,
            diagnosis: None,
            proxy: None,
            updated: util::utc_timestamp(),
        }
    }
}

/// Durumu atomik olarak yazar.
pub fn write(status: &Status) -> Result<()> {
    let directory = paths::run_dir();
    fs::create_dir_all(&directory)
        .with_context(|| format!("durum dizini oluşturulamadı: {}", directory.display()))?;
    let target = paths::status_file();
    let temporary = directory.join("status.json.tmp");

    let mut file = File::create(&temporary)?;
    serde_json::to_writer_pretty(&mut file, status)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    drop(file);

    // Windows'ta hedef varsa `rename` başarısız olur; bu yüzden önce siliyoruz.
    // Kısa bir an dosya yok oluyor, ama yarım yazılmış bir dosyadan iyi.
    #[cfg(windows)]
    let _ = fs::remove_file(&target);

    fs::rename(&temporary, &target)
        .with_context(|| format!("durum dosyası yazılamadı: {}", target.display()))?;
    Ok(())
}

/// Durumu okur. Dosya yoksa "henüz çalıştırılmadı" durumu döner.
pub fn read() -> Status {
    fs::read_to_string(paths::status_file())
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_else(|| Status::idle("henüz çalıştırılmadı"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_idle_status_is_not_active() {
        let status = Status::idle("test");
        assert!(!status.active);
        assert_eq!(status.profile, "none");
        assert!(!status.updated.is_empty(), "zaman damgası her zaman olmalı");
    }

    #[test]
    fn optional_fields_are_omitted_when_empty() {
        // GUI eski sürümlerle de çalışabilsin diye boş alanlar yazılmıyor.
        let encoded = serde_json::to_string(&Status::idle("test")).expect("serialise");
        assert!(!encoded.contains("backend"));
        assert!(!encoded.contains("proxy"));
        assert!(encoded.contains("\"active\""), "zorunlu alanlar yazılmalı");
    }

    #[test]
    fn a_status_survives_a_json_round_trip() {
        let mut status = Status::idle("çalışıyor");
        status.active = true;
        status.backend = Some("ciadpi".into());
        status.proxy = Some("127.0.0.1:1080".into());
        let encoded = serde_json::to_string(&status).expect("serialise");
        let decoded: Status = serde_json::from_str(&encoded).expect("deserialise");
        assert!(decoded.active);
        assert_eq!(decoded.proxy.as_deref(), Some("127.0.0.1:1080"));
    }

    #[test]
    fn a_status_without_the_new_fields_still_parses() {
        // 0.4 sürümünün yazdığı dosya biçimi.
        let legacy = r#"{"active":true,"profile":"discord","method":"x",
                          "message":"y","dns_cloudflare":false,"updated":"-"}"#;
        let status: Status = serde_json::from_str(legacy).expect("eski biçim okunmalı");
        assert_eq!(status.profile, "discord");
        assert!(status.backend.is_none());
    }
}
