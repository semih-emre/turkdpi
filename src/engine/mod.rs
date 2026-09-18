//! Motor katmanı: stratejiyi gerçekten uygulayan arka uçlar.
//!
//! Üç aile var ve hepsi aynı stratejiyi farklı yollarla uyguluyor:
//!
//! | Arka uç | Platform | Yöntem | Yetki |
//! |---|---|---|---|
//! | `nfqws` | Linux | nftables NFQUEUE | root |
//! | `winws` | Windows | WinDivert sürücüsü | yönetici |
//! | `ciadpi` | hepsi | yerel SOCKS5 proxy | gerekmez |
//!
//! Şeffaf arka uçlar tüm trafiği etkiler, bu yüzden uygulamaları ayarlamak
//! gerekmez; ama çekirdek seviyesinde yetki isterler. Proxy arka ucu yetki
//! istemez ama yalnızca kendisine yönlendirilen trafiği etkiler.
//!
//! Seçim otomatik: yetki varsa ve yerel motor kuruluysa şeffaf mod, yoksa
//! proxy moduna düşülüyor. Böylece hiçbir kullanıcı "hiç çalışmadı" durumunda
//! kalmıyor.

#[cfg(target_os = "linux")]
mod nfqws;
#[cfg(target_os = "windows")]
mod winws;

mod byedpi;

use crate::paths;
use crate::privilege;
use crate::strategy::Strategy;
use crate::verify::Channel;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Child;

/// Kullanılabilir arka uçlar.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Backend {
    /// Linux, NFQUEUE tabanlı şeffaf motor.
    Nfqws,
    /// Windows, WinDivert tabanlı şeffaf motor.
    Winws,
    /// Her platformda çalışan yerel SOCKS5 proxy.
    ByeDpi,
}

impl Backend {
    pub fn as_str(self) -> &'static str {
        match self {
            Backend::Nfqws => "nfqws",
            Backend::Winws => "winws",
            Backend::ByeDpi => "ciadpi",
        }
    }

    /// Kullanıcıya gösterilecek açıklama.
    pub fn description(self) -> &'static str {
        match self {
            Backend::Nfqws => "şeffaf mod (nftables + nfqws)",
            Backend::Winws => "şeffaf mod (WinDivert + winws)",
            Backend::ByeDpi => "proxy modu (yerel SOCKS5)",
        }
    }

    pub fn requires_elevation(self) -> bool {
        !matches!(self, Backend::ByeDpi)
    }

    /// Şeffaf arka uçlar tüm trafiği etkiler; proxy yalnızca kendi üzerinden
    /// geçeni. Doğrulamanın hangi yoldan yapılacağını bu belirliyor.
    pub fn is_transparent(self) -> bool {
        !matches!(self, Backend::ByeDpi)
    }

    /// Motor ikilisi bu sistemde kurulu mu?
    pub fn is_installed(self) -> bool {
        paths::engine_binary(self.as_str()).is_file()
    }

    /// Bu platformda bu arka uç anlamlı mı?
    fn supported_here(self) -> bool {
        match self {
            Backend::Nfqws => cfg!(target_os = "linux"),
            Backend::Winws => cfg!(target_os = "windows"),
            Backend::ByeDpi => true,
        }
    }
}

/// Bu platformdaki tercih sırası. Şeffaf mod önce geliyor: uygulama ayarı
/// gerektirmediği ve UDP dahil tüm trafiği kapsadığı için Discord sesi gibi
/// kullanımlarda tek çalışan seçenek o.
pub fn preference_order() -> Vec<Backend> {
    let mut order = Vec::new();
    if cfg!(target_os = "linux") {
        order.push(Backend::Nfqws);
    }
    if cfg!(target_os = "windows") {
        order.push(Backend::Winws);
    }
    order.push(Backend::ByeDpi);
    order
}

/// Bu sistemde kullanılabilecek en iyi arka ucu seçer.
///
/// `elevated` parametresi test edilebilirlik için dışarıdan veriliyor.
pub fn select_with(elevated: bool, installed: impl Fn(Backend) -> bool) -> Result<Backend> {
    let candidates = preference_order();
    for backend in &candidates {
        if !backend.supported_here() || !installed(*backend) {
            continue;
        }
        if backend.requires_elevation() && !elevated {
            continue;
        }
        return Ok(*backend);
    }
    // Neden seçilemediğini ayırt edip kullanıcıya doğru yönlendirmeyi veriyoruz.
    let any_installed = candidates.iter().any(|backend| installed(*backend));
    if !any_installed {
        bail!(
            "hiçbir motor bulunamadı; beklenen konum: {}",
            paths::engine_dir().display()
        );
    }
    bail!(
        "kurulu motorlar yetki gerektiriyor: {}",
        privilege::elevation_hint()
    )
}

/// Gerçek sistemde arka uç seçer.
pub fn select() -> Result<Backend> {
    select_with(privilege::is_elevated(), Backend::is_installed)
}

/// Çalışan bir motor oturumu.
///
/// `Drop` uygulaması kasıtlı: strateji araması sırasında onlarca motor arka
/// arkaya başlatılıp durduruluyor. Erken çıkışta ya da hata durumunda süreçlerin
/// ve güvenlik duvarı kurallarının arkada kalması, kullanıcının internetini
/// bozuk bırakırdı.
pub struct Session {
    pub backend: Backend,
    /// Proxy modunda doğrulamanın yapılacağı yerel adres.
    pub channel: Channel,
    children: Vec<Child>,
    /// Şeffaf modda güvenlik duvarı kuralları yüklendi mi?
    firewall_installed: bool,
    /// Ayrılmış oturum: süreçler CLI çıktıktan sonra da yaşamalı, bu yüzden
    /// `Drop` onlara dokunmuyor.
    detached: bool,
}

/// Ayrılmış bir oturumun diske yazılan künyesi.
#[derive(Serialize, Deserialize)]
struct DetachedRecord {
    backend: Backend,
    pids: Vec<u32>,
    /// Süreç kimliği geri dönüştürülmüş olabilir; öldürmeden önce sürecin
    /// gerçekten bizim motorumuz olduğunu bu adla doğruluyoruz.
    binary: String,
    firewall_installed: bool,
}

fn record_path() -> std::path::PathBuf {
    paths::run_dir().join("engine.json")
}

impl Session {
    /// Motoru durdurur ve kurulan tüm kuralları geri alır.
    pub fn stop(&mut self) -> Result<()> {
        for child in &mut self.children {
            // Süreç zaten ölmüş olabilir; bu bir hata değil.
            let _ = child.kill();
            let _ = child.wait();
        }
        self.children.clear();
        if self.firewall_installed {
            self.firewall_installed = false;
            remove_firewall_rules()?;
        }
        Ok(())
    }

    /// Motoru arka planda bırakır.
    ///
    /// Strateji araması sırasında onlarca oturum açılıp kapanıyor ve `Drop`
    /// hepsini temizliyor. Kazanan strateji bulunduğunda ise motorun yaşamaya
    /// devam etmesi gerekiyor: bu çağrıdan sonra süreçler CLI'dan bağımsız.
    pub fn detach(mut self) -> Result<()> {
        let record = DetachedRecord {
            backend: self.backend,
            pids: self.children.iter().map(Child::id).collect(),
            binary: paths::engine_binary(self.backend.as_str())
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
            firewall_installed: self.firewall_installed,
        };
        let directory = paths::run_dir();
        std::fs::create_dir_all(&directory)?;
        std::fs::write(record_path(), serde_json::to_vec_pretty(&record)?)?;
        self.detached = true;
        Ok(())
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        if self.detached {
            return;
        }
        let _ = self.stop();
    }
}

/// Daha önce ayrılmış bir motoru durdurur ve kuralları geri alır.
///
/// Çalışan motor yoksa sessizce başarılı sayılıyor: temizlik her durumda
/// güvenle çağrılabilmeli.
pub fn stop_detached() -> Result<()> {
    let path = record_path();
    let Ok(raw) = std::fs::read(&path) else {
        // Künye yok; yine de arkada kalmış kural olabilir.
        return remove_firewall_rules();
    };
    let record: DetachedRecord =
        serde_json::from_slice(&raw).context("çalışan motor künyesi okunamadı")?;
    for pid in record.pids {
        kill_if_matches(pid, &record.binary);
    }
    let _ = std::fs::remove_file(&path);
    if record.firewall_installed {
        remove_firewall_rules()?;
    }
    Ok(())
}

/// Bir motorun çalışıp çalışmadığını söyler.
pub fn detached_backend() -> Option<Backend> {
    let raw = std::fs::read(record_path()).ok()?;
    let record: DetachedRecord = serde_json::from_slice(&raw).ok()?;
    Some(record.backend)
}

/// Süreci yalnızca beklenen ikiliye aitse sonlandırır.
///
/// Süreç kimlikleri işletim sistemi tarafından geri dönüştürülüyor. Doğrulama
/// yapmadan öldürmek, aradan geçen sürede aynı kimliği almış tamamen alakasız
/// bir kullanıcı programını kapatabilirdi.
#[cfg(unix)]
fn kill_if_matches(pid: u32, binary: &str) {
    let Ok(target) = std::fs::read_link(format!("/proc/{pid}/exe")) else {
        return;
    };
    let matches = target
        .file_name()
        .map(|name| name.to_string_lossy() == binary)
        .unwrap_or(false);
    if !matches {
        return;
    }
    // SAFETY: kill yalnızca sinyal gönderir; pid doğrulandı.
    unsafe {
        libc::kill(pid as i32, libc::SIGTERM);
    }
    std::thread::sleep(std::time::Duration::from_millis(250));
    if std::path::Path::new(&format!("/proc/{pid}")).exists() {
        unsafe {
            libc::kill(pid as i32, libc::SIGKILL);
        }
    }
}

#[cfg(windows)]
fn kill_if_matches(pid: u32, binary: &str) {
    use std::process::Command;
    // `taskkill` süreç adına göre filtreleyebiliyor; hem kimliği hem adı
    // eşleştirerek yanlış süreci kapatma riskini ortadan kaldırıyoruz.
    let _ = Command::new("taskkill")
        .args([
            "/PID",
            &pid.to_string(),
            "/FI",
            &format!("IMAGENAME eq {binary}"),
            "/F",
            "/T",
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

#[cfg(target_os = "linux")]
fn remove_firewall_rules() -> Result<()> {
    nfqws::remove_rules()
}

#[cfg(not(target_os = "linux"))]
fn remove_firewall_rules() -> Result<()> {
    // Windows'ta WinDivert kuralları sürecin ömrüne bağlı; süreç ölünce
    // kendiliğinden kalkıyor. Proxy modunda kural yok.
    Ok(())
}

/// Seçilen arka uçla bir stratejiyi başlatır.
pub fn start(backend: Backend, strategy: &Strategy, hostlist: Option<&Path>) -> Result<Session> {
    match backend {
        #[cfg(target_os = "linux")]
        Backend::Nfqws => nfqws::start(strategy, hostlist),
        #[cfg(target_os = "windows")]
        Backend::Winws => winws::start(strategy, hostlist),
        Backend::ByeDpi => byedpi::start(strategy, hostlist),
        #[allow(unreachable_patterns)]
        other => bail!("{} bu platformda desteklenmiyor", other.as_str()),
    }
}

/// Arka uçların ortak kullandığı süreç başlatma yardımcıları.
mod spawn {
    use anyhow::{bail, Context, Result};
    use std::path::Path;
    use std::process::{Child, Command, Stdio};
    use std::time::Duration;

    /// Bir motoru başlatır ve hemen ölmediğini doğrular.
    ///
    /// Geçersiz argüman verildiğinde motorlar anında çıkıyor. Bunu yakalamazsak
    /// strateji araması "başlatıldı ama çalışmadı" diye yanlış sonuç üretir.
    pub fn spawn_checked(binary: &Path, args: &[String], label: &str) -> Result<Child> {
        if !binary.is_file() {
            bail!("{label} bulunamadı: {}", binary.display());
        }
        let mut child = Command::new(binary)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .with_context(|| format!("{label} başlatılamadı"))?;
        std::thread::sleep(Duration::from_millis(250));
        if let Some(status) = child.try_wait()? {
            bail!("{label} erken sonlandı: {status}");
        }
        Ok(child)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_native_backend_wins_when_it_is_installed_and_allowed() {
        let backend = select_with(true, |_| true).expect("bir arka uç seçilmeli");
        if cfg!(target_os = "linux") {
            assert_eq!(backend, Backend::Nfqws);
        } else if cfg!(target_os = "windows") {
            assert_eq!(backend, Backend::Winws);
        } else {
            assert_eq!(backend, Backend::ByeDpi);
        }
    }

    #[test]
    fn without_elevation_it_falls_back_to_the_proxy() {
        // Bu, "yetkim yok, hiç çalışmıyor" durumunu ortadan kaldıran davranış.
        let backend = select_with(false, |_| true).expect("proxy'ye düşmeli");
        assert_eq!(backend, Backend::ByeDpi);
        assert!(!backend.requires_elevation());
    }

    #[test]
    fn a_missing_proxy_still_allows_the_native_backend() {
        let backend = select_with(true, |candidate| candidate != Backend::ByeDpi);
        if cfg!(target_os = "linux") || cfg!(target_os = "windows") {
            assert!(backend.is_ok(), "yerel motor kuruluysa seçilmeli");
        } else {
            assert!(backend.is_err());
        }
    }

    #[test]
    fn nothing_installed_reports_where_engines_are_expected() {
        let error = select_with(true, |_| false).expect_err("hata beklenir");
        let message = format!("{error}");
        assert!(
            message.contains("motor bulunamadı"),
            "eksik motor açıkça söylenmeli: {message}"
        );
    }

    #[test]
    fn installed_but_unprivileged_explains_how_to_elevate() {
        // Yalnızca yetki isteyen arka uçlar kurulu.
        let error = select_with(false, |candidate| candidate.requires_elevation());
        if cfg!(target_os = "linux") || cfg!(target_os = "windows") {
            let message = format!("{}", error.expect_err("hata beklenir"));
            assert!(
                message.contains("yetki"),
                "yetki eksikliği söylenmeli: {message}"
            );
        }
    }

    #[test]
    fn transparency_matches_the_elevation_requirement() {
        // Şeffaf yakalama çekirdek erişimi ister; proxy istemez.
        for backend in [Backend::Nfqws, Backend::Winws, Backend::ByeDpi] {
            assert_eq!(backend.is_transparent(), backend.requires_elevation());
        }
    }
}
