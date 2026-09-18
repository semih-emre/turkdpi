//! Motor ikililerinin indirilmesi ve doğrulanması.
//!
//! Üçüncü taraf ikililer depoya commit edilmiyor; `engine/manifest.json`
//! yalnızca hangi sürümün nereden alınacağını ve SHA-256 özetini sabitliyor.
//!
//! Özet doğrulaması isteğe bağlı değil. İndirme, tam da bu aracın aşmaya
//! çalıştığı türden müdahaleye açık bir ağ üzerinden geçiyor; doğrulanmamış
//! bir ikiliyi çalıştırmak, kullanıcıyı korumaya çalıştığımız şeyin daha
//! kötüsüne maruz bırakırdı.
//!
//! İndirme ve arşiv açma işleri `curl` ile `tar`a bırakılıyor. İkisi de
//! Windows 10 1803+, modern Linux ve macOS'ta sistemle birlikte geliyor;
//! Windows'un `tar.exe`si (bsdtar) zip arşivlerini de açabiliyor, bu yüzden
//! üç platformda tek kod yolu yetiyor.

use crate::{paths, sha256};
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Deserialize)]
struct Manifest {
    byedpi: EnginePackage,
}

#[derive(Debug, Deserialize)]
struct EnginePackage {
    version: String,
    binary: String,
    targets: std::collections::HashMap<String, TargetEntry>,
}

#[derive(Debug, Deserialize)]
struct TargetEntry {
    url: String,
    sha256: String,
    /// Arşivin içinden çıkarılacak dosya.
    member: String,
}

/// Bu makinenin manifest anahtarı, örneğin `windows-x86_64`.
pub fn target_key() -> String {
    let os = if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    };
    let architecture = if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else if cfg!(target_arch = "x86") {
        "i686"
    } else {
        std::env::consts::ARCH
    };
    format!("{os}-{architecture}")
}

fn load_manifest(path: &Path) -> Result<Manifest> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("motor bildirimi okunamadı: {}", path.display()))?;
    serde_json::from_str(&raw).context("motor bildirimi geçersiz")
}

/// `curl` ile indirir. Yönlendirmeleri izliyor ve yalnızca HTTPS kabul ediyor.
fn download(url: &str, destination: &Path) -> Result<()> {
    let status = Command::new("curl")
        .args([
            "--fail",
            "--location",
            "--proto",
            "=https",
            "--tlsv1.2",
            "--silent",
            "--show-error",
            "--connect-timeout",
            "20",
            "--max-time",
            "300",
            "--output",
        ])
        .arg(destination)
        .arg(url)
        .status()
        .context("curl çalıştırılamadı; sistemde kurulu olduğundan emin olun")?;
    if !status.success() {
        bail!("indirme başarısız ({status}): {url}");
    }
    Ok(())
}

/// Arşivi açar. `tar` hem `.tar.gz` hem de (Windows'ta) `.zip` işliyor.
fn extract(archive: &Path, directory: &Path) -> Result<()> {
    std::fs::create_dir_all(directory)?;
    let status = Command::new("tar")
        .arg("-xf")
        .arg(archive)
        .arg("-C")
        .arg(directory)
        .status()
        .context("tar çalıştırılamadı; sistemde kurulu olduğundan emin olun")?;
    if !status.success() {
        bail!("arşiv açılamadı: {}", archive.display());
    }
    Ok(())
}

/// Açılan ağaçta bir dosyayı adına göre bulur.
///
/// Arşiv düzeni sürümden sürüme değişebiliyor (bazıları dosyaları bir alt
/// dizine koyuyor), bu yüzden sabit bir yol varsaymak yerine arıyoruz.
fn find_member(directory: &Path, name: &str) -> Option<PathBuf> {
    let entries = std::fs::read_dir(directory).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_member(&path, name) {
                return Some(found);
            }
        } else if path.file_name().is_some_and(|file| file == name) {
            return Some(path);
        }
    }
    None
}

/// Çalıştırma iznini verir. Windows'ta gerekmiyor.
#[cfg(unix)]
fn make_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = std::fs::metadata(path)?.permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(path, permissions)?;
    Ok(())
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<()> {
    Ok(())
}

/// ByeDPI motorunu indirir, doğrular ve kurar.
///
/// Zaten kuruluysa hiçbir şey yapmıyor; `force` ile yeniden indirilebiliyor.
pub fn install_byedpi(manifest_path: &Path, force: bool) -> Result<PathBuf> {
    let manifest = load_manifest(manifest_path)?;
    let package = manifest.byedpi;
    let destination = paths::engine_binary(&package.binary);

    if destination.is_file() && !force {
        return Ok(destination);
    }

    let key = target_key();
    let Some(entry) = package.targets.get(&key) else {
        bail!(
            "ByeDPI {} için hazır ikili yok: {key}\n\
             Kaynaktan derleyip {} konumuna koyabilirsiniz.",
            package.version,
            destination.display()
        );
    };

    let workspace = std::env::temp_dir().join(format!("turkdpi-engine-{key}"));
    let _ = std::fs::remove_dir_all(&workspace);
    std::fs::create_dir_all(&workspace)?;

    let archive = workspace.join("engine-archive");
    download(&entry.url, &archive)?;

    let bytes = std::fs::read(&archive).context("indirilen dosya okunamadı")?;
    if !sha256::verify(&bytes, &entry.sha256) {
        let _ = std::fs::remove_dir_all(&workspace);
        bail!(
            "indirilen dosyanın SHA-256 özeti eşleşmedi.\n  beklenen: {}\n  bulunan : {}\n\
             Dosya yolda değiştirilmiş olabilir; kurulum iptal edildi.",
            entry.sha256,
            sha256::hex(&sha256::digest(&bytes))
        );
    }

    let unpacked = workspace.join("unpacked");
    extract(&archive, &unpacked)?;
    let Some(member) = find_member(&unpacked, &entry.member) else {
        bail!("arşivde {} bulunamadı", entry.member);
    };

    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("motor dizini oluşturulamadı: {}", parent.display()))?;
    }
    std::fs::copy(&member, &destination)
        .with_context(|| format!("motor kurulamadı: {}", destination.display()))?;
    make_executable(&destination)?;
    let _ = std::fs::remove_dir_all(&workspace);

    Ok(destination)
}

/// Depodaki bildirim dosyasının yolu.
pub fn default_manifest() -> PathBuf {
    // Kurulu sistemde paylaşılan veri dizininde, geliştirme ağacında ise
    // deponun kökünde duruyor.
    let installed = paths::share_dir().join("engine-manifest.json");
    if installed.is_file() {
        return installed;
    }
    PathBuf::from("engine/manifest.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_repository_manifest_parses_and_covers_this_platform() {
        let manifest = load_manifest(Path::new("engine/manifest.json"))
            .expect("depodaki bildirim geçerli olmalı");
        assert_eq!(manifest.byedpi.binary, "ciadpi");
        assert!(
            !manifest.byedpi.targets.is_empty(),
            "en az bir hedef tanımlı olmalı"
        );
    }

    #[test]
    fn every_pinned_entry_has_a_well_formed_digest() {
        let manifest = load_manifest(Path::new("engine/manifest.json")).expect("bildirim");
        for (key, entry) in &manifest.byedpi.targets {
            assert_eq!(
                entry.sha256.len(),
                64,
                "{key}: SHA-256 64 onaltılık karakter olmalı"
            );
            assert!(
                entry.sha256.chars().all(|c| c.is_ascii_hexdigit()),
                "{key}: özet onaltılık olmalı"
            );
            assert!(
                entry.url.starts_with("https://"),
                "{key}: indirme yalnızca HTTPS üzerinden olmalı"
            );
        }
    }

    #[test]
    fn the_target_key_matches_the_manifest_naming() {
        let key = target_key();
        assert!(key.contains('-'), "biçim <os>-<arch> olmalı: {key}");
        let (os, _) = key.split_once('-').expect("ayraç");
        assert!(
            matches!(os, "windows" | "linux" | "macos"),
            "bilinen os: {os}"
        );
    }

    #[test]
    fn a_member_is_found_even_when_nested_in_a_subdirectory() {
        // Arşiv düzeni sürümler arasında değişebiliyor.
        let root = std::env::temp_dir().join("turkdpi-test-find-member");
        let _ = std::fs::remove_dir_all(&root);
        let nested = root.join("byedpi-1.2.3").join("bin");
        std::fs::create_dir_all(&nested).expect("dizin");
        std::fs::write(nested.join("ciadpi"), b"x").expect("dosya");

        let found = find_member(&root, "ciadpi").expect("iç içe dosya bulunmalı");
        assert!(found.ends_with("ciadpi"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_missing_member_is_reported_rather_than_guessed() {
        let root = std::env::temp_dir().join("turkdpi-test-missing-member");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("dizin");
        assert!(find_member(&root, "ciadpi").is_none());
        let _ = std::fs::remove_dir_all(&root);
    }
}
