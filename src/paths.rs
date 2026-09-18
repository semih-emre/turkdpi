//! Platforma göre değişen dizinler.
//!
//! Linux'ta FHS, Windows'ta `%ProgramData%`, macOS'ta `/Library/Application
//! Support` yerleşimi kullanılıyor. Her yol bir ortam değişkeniyle
//! geçersiz kılınabiliyor; bu hem testlerin gerçek sistem dizinlerine
//! dokunmadan çalışmasını hem de taşınabilir (kurulumsuz) kullanımı sağlıyor.

use std::path::{Path, PathBuf};

/// Çalışma zamanı durumu: `status.json`, PID dosyaları. Yeniden başlatmada
/// silinmesi beklenen veriler.
pub fn run_dir() -> PathBuf {
    from_env("TURKDPI_RUN_DIR").unwrap_or_else(|| {
        if cfg!(target_os = "linux") {
            PathBuf::from("/run/turkdpi")
        } else if cfg!(target_os = "windows") {
            program_data().join("run")
        } else {
            PathBuf::from("/var/run/turkdpi")
        }
    })
}

/// Kalıcı durum: öğrenilen stratejiler, DNS yedekleri, ağ hafızası.
pub fn state_dir() -> PathBuf {
    from_env("TURKDPI_STATE_DIR").unwrap_or_else(|| {
        if cfg!(target_os = "linux") {
            PathBuf::from("/var/lib/turkdpi")
        } else if cfg!(target_os = "windows") {
            program_data()
        } else {
            PathBuf::from("/Library/Application Support/TurkDPI")
        }
    })
}

/// Dağıtımla gelen profil dosyaları.
pub fn profile_dir() -> PathBuf {
    from_env("TURKDPI_PROFILE_DIR").unwrap_or_else(|| {
        if cfg!(target_os = "linux") {
            PathBuf::from("/usr/share/turkdpi/profiles")
        } else if cfg!(target_os = "windows") {
            install_dir().join("profiles")
        } else {
            PathBuf::from("/usr/local/share/turkdpi/profiles")
        }
    })
}

/// Host listeleri gibi paylaşılan veri dosyaları.
pub fn share_dir() -> PathBuf {
    from_env("TURKDPI_SHARE_DIR").unwrap_or_else(|| {
        if cfg!(target_os = "linux") {
            PathBuf::from("/usr/share/turkdpi")
        } else if cfg!(target_os = "windows") {
            install_dir()
        } else {
            PathBuf::from("/usr/local/share/turkdpi")
        }
    })
}

/// Motor ikilileri: `nfqws`, `winws.exe`, `tpws`, `ciadpi`.
pub fn engine_dir() -> PathBuf {
    from_env("TURKDPI_ENGINE_DIR").unwrap_or_else(|| {
        if cfg!(target_os = "linux") {
            PathBuf::from("/usr/lib/turkdpi")
        } else if cfg!(target_os = "windows") {
            install_dir().join("engine")
        } else {
            PathBuf::from("/usr/local/lib/turkdpi")
        }
    })
}

/// Bir motor ikilisinin tam yolu. Windows'ta `.exe` uzantısı eklenir.
pub fn engine_binary(name: &str) -> PathBuf {
    let file_name = if cfg!(target_os = "windows") {
        format!("{name}.exe")
    } else {
        name.to_owned()
    };
    engine_dir().join(file_name)
}

pub fn status_file() -> PathBuf {
    run_dir().join("status.json")
}

/// Ağ başına öğrenilen stratejilerin tutulduğu dizin.
pub fn networks_dir() -> PathBuf {
    state_dir().join("networks")
}

/// Günlük dosyalarının tutulduğu dizin.
pub fn log_dir() -> PathBuf {
    from_env("TURKDPI_LOG_DIR").unwrap_or_else(|| state_dir().join("logs"))
}

/// Çalıştırılabilir dosyanın bulunduğu dizin. Windows'ta kurulum dizini budur;
/// taşınabilir kullanımda da doğru sonucu verir.
fn install_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn program_data() -> PathBuf {
    std::env::var_os("ProgramData")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"))
        .join("TurkDPI")
}

fn from_env(key: &str) -> Option<PathBuf> {
    std::env::var_os(key)
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_binary_gets_an_exe_suffix_only_on_windows() {
        let path = engine_binary("nfqws");
        let name = path.file_name().expect("dosya adı").to_string_lossy();
        if cfg!(target_os = "windows") {
            assert_eq!(name, "nfqws.exe");
        } else {
            assert_eq!(name, "nfqws");
        }
    }

    #[test]
    fn status_file_lives_under_the_run_directory() {
        assert!(status_file().starts_with(run_dir()));
    }

    #[test]
    fn networks_directory_lives_under_the_state_directory() {
        assert!(networks_dir().starts_with(state_dir()));
    }
}
