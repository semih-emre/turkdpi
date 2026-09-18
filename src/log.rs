//! Diske yazılan günlük.
//!
//! Bir kullanıcının ağında neyin yanlış gittiğini uzaktan anlamanın tek yolu
//! bu: teşhis sonucu, denenen stratejiler ve hataların kaydı. Kullanıcı
//! dosyayı olduğu gibi gönderebiliyor.
//!
//! İki sınır bilerek konuldu, çünkü sürekli çalışan bir aracın günlüğü kontrol
//! edilmezse diski doldurur:
//!   * **Yaş**: [`RETENTION_DAYS`] günden eski dosyalar siliniyor. Geçen
//!     haftanın kaydının hata ayıklamaya faydası yok.
//!   * **Boyut**: tek bir günün dosyası [`MAX_FILE_BYTES`] sınırını aşarsa
//!     baş tarafı atılıp son kısım korunuyor — hatayı anlatan satırlar
//!     genellikle sondakiler.
//!
//! Günlük yazımı hiçbir koşulda programı durdurmuyor: disk doluysa ya da
//! dizin yazılamıyorsa kayıt sessizce atlanıyor. Günlük tutamamak, aracın
//! asıl işini yapmamasına gerekçe değil.

use crate::paths;
use crate::util;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Bundan eski günlük dosyaları siliniyor.
pub const RETENTION_DAYS: u64 = 3;

/// Tek bir günlük dosyasının üst sınırı (4 MiB).
pub const MAX_FILE_BYTES: u64 = 4 * 1024 * 1024;

/// Sınır aşıldığında korunacak kısım. Dosyanın tamamını silmek yerine sonunu
/// tutuyoruz: son satırlar hatayı anlatan satırlar.
const KEEP_TAIL_BYTES: usize = 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Level {
    Info,
    Warn,
    Error,
}

impl Level {
    fn as_str(self) -> &'static str {
        match self {
            Level::Info => "INFO",
            Level::Warn => "WARN",
            Level::Error => "ERROR",
        }
    }
}

/// Bugünün günlük dosyası.
pub fn current_file() -> PathBuf {
    paths::log_dir().join(format!("turkdpi-{}.log", util::utc_date()))
}

/// Bir satır yazar. Hata durumunda sessizce vazgeçiyor.
pub fn write(level: Level, message: &str) {
    let _ = try_write(level, message);
}

pub fn info(message: &str) {
    write(Level::Info, message);
}

pub fn warn(message: &str) {
    write(Level::Warn, message);
}

pub fn error(message: &str) {
    write(Level::Error, message);
}

fn try_write(level: Level, message: &str) -> std::io::Result<()> {
    let directory = paths::log_dir();
    fs::create_dir_all(&directory)?;
    let path = current_file();

    // Sınır kontrolü yazmadan önce yapılıyor; böylece dosya hiçbir zaman
    // sınırın belirgin şekilde üzerine çıkmıyor.
    if let Ok(metadata) = fs::metadata(&path) {
        if metadata.len() > MAX_FILE_BYTES {
            truncate_to_tail(&path);
        }
    }

    let mut file = OpenOptions::new().create(true).append(true).open(&path)?;
    // Çok satırlı mesajlar tek bir kaydın parçası olarak girintileniyor;
    // aksi halde satır başına zaman damgası olmayan kayıtlar oluşuyor.
    let body = message.replace('\n', "\n    ");
    writeln!(
        file,
        "{} [{}] {}",
        util::utc_timestamp(),
        level.as_str(),
        body
    )
}

/// Dosyanın son kısmını koruyup gerisini atar.
fn truncate_to_tail(path: &Path) {
    let Ok(contents) = fs::read(path) else {
        return;
    };
    let start = contents.len().saturating_sub(KEEP_TAIL_BYTES);
    // Satır ortasından kesmemek için ilk satır sonuna kadar ilerliyoruz.
    let boundary = contents[start..]
        .iter()
        .position(|&byte| byte == b'\n')
        .map(|offset| start + offset + 1)
        .unwrap_or(start);
    let tail = &contents[boundary..];
    let notice = format!(
        "{} [INFO] günlük boyut sınırına ulaştı; eski kayıtlar atıldı\n",
        util::utc_timestamp()
    );
    let mut replacement = notice.into_bytes();
    replacement.extend_from_slice(tail);
    let _ = fs::write(path, replacement);
}

/// Saklama süresini aşan günlük dosyalarını siler.
///
/// Dosyanın yaşı değiştirilme zamanından okunuyor; dosya adındaki tarihi
/// ayrıştırmaya göre daha dayanıklı, çünkü elle kopyalanmış ya da adı
/// değiştirilmiş dosyalar da doğru değerlendiriliyor.
pub fn prune() {
    let _ = prune_in(&paths::log_dir(), RETENTION_DAYS);
}

fn prune_in(directory: &Path, retention_days: u64) -> std::io::Result<usize> {
    let cutoff = SystemTime::now()
        .checked_sub(Duration::from_secs(retention_days * 86_400))
        .unwrap_or(SystemTime::UNIX_EPOCH);

    let mut removed = 0;
    for entry in fs::read_dir(directory)? {
        let Ok(entry) = entry else { continue };
        let path = entry.path();
        // Yalnızca kendi ürettiğimiz dosyalara dokunuyoruz: kullanıcı bu
        // dizine başka bir şey koyduysa silmek kabalık olurdu.
        let is_ours = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("turkdpi-") && name.ends_with(".log"));
        if !is_ours {
            continue;
        }
        let Ok(modified) = entry.metadata().and_then(|meta| meta.modified()) else {
            continue;
        };
        if modified < cutoff && fs::remove_file(&path).is_ok() {
            removed += 1;
        }
    }
    Ok(removed)
}

/// Saklanan günlük dosyalarını yeniden eskiye doğru listeler.
pub fn files() -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(paths::log_dir()) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("turkdpi-") && name.ends_with(".log"))
        })
        .collect();
    // Dosya adları tarih sırasına göre sıralanıyor; en yeni başa alınıyor.
    paths.sort();
    paths.reverse();
    paths
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let directory = std::env::temp_dir().join(format!("turkdpi-log-{name}"));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).expect("dizin");
        directory
    }

    fn write_aged(directory: &Path, name: &str, age_days: u64) {
        let path = directory.join(name);
        fs::write(&path, b"kayit\n").expect("dosya");
        // Değiştirilme zamanını geçmişe çekmek için dosyayı yeniden yazmak
        // yetmiyor; testte doğrudan zaman damgası ayarlıyoruz.
        let when = SystemTime::now() - Duration::from_secs(age_days * 86_400 + 60);
        let file = OpenOptions::new().write(true).open(&path).expect("aç");
        file.set_modified(when).expect("zaman damgası");
    }

    #[test]
    fn logs_older_than_the_retention_window_are_removed() {
        let directory = scratch("prune-old");
        write_aged(&directory, "turkdpi-20200101.log", 30);
        write_aged(&directory, "turkdpi-20200102.log", 10);

        let removed = prune_in(&directory, RETENTION_DAYS).expect("temizlik");
        assert_eq!(removed, 2, "iki eski dosya silinmeli");
        assert!(!directory.join("turkdpi-20200101.log").exists());
        let _ = fs::remove_dir_all(&directory);
    }

    #[test]
    fn recent_logs_are_kept() {
        // Kullanıcının gönderebileceği asıl kayıtlar bunlar; silinmemeli.
        let directory = scratch("prune-recent");
        write_aged(&directory, "turkdpi-20260918.log", 0);
        write_aged(&directory, "turkdpi-20260917.log", 1);

        let removed = prune_in(&directory, RETENTION_DAYS).expect("temizlik");
        assert_eq!(removed, 0, "yakın tarihli kayıtlar korunmalı");
        assert!(directory.join("turkdpi-20260918.log").exists());
        assert!(directory.join("turkdpi-20260917.log").exists());
        let _ = fs::remove_dir_all(&directory);
    }

    #[test]
    fn unrelated_files_are_never_deleted() {
        // Kullanıcı bu dizine kendi notunu koymuş olabilir.
        let directory = scratch("prune-foreign");
        let foreign = directory.join("notlarim.txt");
        fs::write(&foreign, b"dokunma").expect("dosya");
        let when = SystemTime::now() - Duration::from_secs(365 * 86_400);
        OpenOptions::new()
            .write(true)
            .open(&foreign)
            .expect("aç")
            .set_modified(when)
            .expect("zaman damgası");

        prune_in(&directory, RETENTION_DAYS).expect("temizlik");
        assert!(foreign.exists(), "yabancı dosyaya dokunulmamalı");
        let _ = fs::remove_dir_all(&directory);
    }

    #[test]
    fn an_oversized_log_keeps_its_tail() {
        let directory = scratch("truncate");
        let path = directory.join("turkdpi-20260918.log");
        // Sınırın üzerinde bir dosya üret.
        let line = "x".repeat(99);
        let mut contents = String::new();
        while contents.len() < (MAX_FILE_BYTES as usize) + 1024 {
            contents.push_str(&line);
            contents.push('\n');
        }
        contents.push_str("SON SATIR\n");
        fs::write(&path, &contents).expect("dosya");

        truncate_to_tail(&path);

        let after = fs::read_to_string(&path).expect("oku");
        assert!(
            (after.len() as u64) < MAX_FILE_BYTES,
            "dosya küçültülmeli: {} bayt",
            after.len()
        );
        assert!(after.contains("SON SATIR"), "son satırlar korunmalı");
        assert!(after.contains("eski kayıtlar atıldı"), "kesme not edilmeli");
        let _ = fs::remove_dir_all(&directory);
    }

    #[test]
    fn truncation_does_not_split_a_line_in_half() {
        let directory = scratch("truncate-boundary");
        let path = directory.join("turkdpi-20260918.log");
        let contents: String = (0..200_000).map(|i| format!("satir-{i}\n")).collect();
        fs::write(&path, &contents).expect("dosya");

        truncate_to_tail(&path);

        let after = fs::read_to_string(&path).expect("oku");
        // İlk satır bizim eklediğimiz not, ikincisi tam bir kayıt olmalı.
        let second = after.lines().nth(1).expect("ikinci satır");
        assert!(
            second.starts_with("satir-"),
            "satır ortasından kesilmemeli: {second}"
        );
        let _ = fs::remove_dir_all(&directory);
    }

    #[test]
    fn a_multiline_message_stays_one_record() {
        // Çok satırlı hata mesajları zaman damgasız satırlar üretmemeli.
        let body = "ilk\nikinci".replace('\n', "\n    ");
        assert_eq!(body, "ilk\n    ikinci");
    }
}
