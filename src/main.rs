//! TurkDPI komut satırı arayüzü.
//!
//! GUI bu ikiliyi çağırıyor ve `status.json` üzerinden sonucu okuyor. Tüm
//! komutlar üç platformda da aynı şekilde çalışıyor; platform farkları
//! [`turkdpi::engine`] katmanında kapalı.

use anyhow::{bail, Result};
use turkdpi::engine::{self, Backend};
use turkdpi::probe::NetworkReport;
use turkdpi::search::{self, Progress};
use turkdpi::status::{self, Status};
use turkdpi::strategy::Strategy;
use turkdpi::verify::{self, Channel, VerifyResult};
use turkdpi::{log, paths, privilege};

/// Teşhis ve doğrulamada kullanılan hedefler.
///
/// Discord'un altyapısı birden çok alan adına yayılmış durumda: web arayüzü,
/// gerçek zamanlı ağ geçidi ve CDN farklı adresler üzerinden çalışıyor. Tek
/// bir hedefe bakmak yanıltıcı oluyor — site açılırken sesin bağlanmaması tam
/// olarak bu yüzden.
const DEFAULT_TARGETS: [&str; 3] = ["discord.com", "gateway.discord.gg", "cdn.discordapp.com"];

/// Motorun yalnızca bu alan adlarına müdahale etmesini sağlayan liste.
///
/// Tüm trafiğe müdahale etmek bankacılık siteleri gibi hassas bağlantıları
/// bozabiliyor; liste varsa kapsam daraltılıyor.
fn hostlist() -> Option<std::path::PathBuf> {
    let path = paths::share_dir().join("services-hosts.txt");
    path.is_file().then_some(path)
}

/// Aramanın ilerlemesini terminale ve günlüğe yazar.
#[derive(Default)]
struct CliProgress {
    /// Sonucu beklenen deneme. Günlükte deneme ve sonucu tek satırda
    /// birleştirmek için tutuluyor.
    pending: Option<String>,
}

impl Progress for CliProgress {
    fn diagnosed(&mut self, report: &NetworkReport) {
        println!("Teşhis: {}", report.dominant.summary());
        log::info(&format!("teşhis: {}", report.dominant.summary()));
        if let Some(hop) = report.dpi_hop {
            println!("  DPI mesafesi: {hop} hop");
            log::info(&format!("DPI mesafesi: {hop} hop"));
        }
        for target in &report.targets {
            println!("  {} -> {}", target.host, target.verdict.summary());
            log::info(&format!(
                "  {} -> {}",
                target.host,
                target.verdict.summary()
            ));
        }
        println!();
    }

    fn trying(&mut self, index: usize, total: usize, strategy: &Strategy) {
        print!("[{index}/{total}] {strategy} ... ");
        // Sonuç aynı satıra yazılacak; tampon hemen boşaltılmalı.
        use std::io::Write;
        let _ = std::io::stdout().flush();
        self.pending = Some(format!("[{index}/{total}] {strategy}"));
    }

    fn tried(&mut self, result: &VerifyResult) {
        println!("{}/{} hedef açıldı", result.passed(), result.total());
        // Denenen strateji ile sonucu tek bir kayıtta birleştiriyoruz; ayrı
        // satırlara bölünse günlükte eşleştirmek zorlaşırdı.
        if let Some(attempt) = self.pending.take() {
            log::info(&format!(
                "{attempt} -> {}/{} hedef açıldı",
                result.passed(),
                result.total()
            ));
        }
        for host in &result.hosts {
            if !host.ok {
                log::warn(&format!("  {} açılamadı: {}", host.host, host.detail));
            }
        }
    }
}

fn diagnose(targets: &[&str]) -> Result<()> {
    let report = turkdpi::probe::diagnose_all(targets);
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

/// TTL tarama düzeneğinin gerçekten çalıştığını doğrular.
///
/// DPI mesafesi ölçümü yalnızca engelli bir ağda devreye giriyor, bu yüzden
/// açık bir ağda sessizce bozulmuş olabilir. Bu komut açık bir hedefe artan
/// TTL'lerle ClientHello gönderiyor: paketler sunucuya ancak yol uzunluğu
/// kadar TTL verildiğinde ulaşmalı. Eşiğin görünmesi, `set_ttl` çağrısının
/// etki ettiğini kanıtlıyor.
fn selftest(host: &str) -> Result<()> {
    use std::net::{IpAddr, SocketAddr};
    use turkdpi::probe::{resolve_system, tcp_probe, TcpOutcome};

    let Some(&address) = resolve_system(host).first() else {
        bail!("{host} çözümlenemedi");
    };
    let socket = SocketAddr::new(IpAddr::V4(address), 443);
    println!("hedef: {host} [{address}]");

    let mut first_reply = None;
    for ttl in 1..=15_u8 {
        let outcome = tcp_probe(socket, host, Some(u32::from(ttl)));
        let label = match outcome {
            TcpOutcome::Reply(reply) => format!("{reply:?}"),
            TcpOutcome::Reset => "RST".into(),
            TcpOutcome::Timeout => "yanıt yok".into(),
            TcpOutcome::ConnectFailed => "bağlantı kurulamadı".into(),
        };
        println!("  TTL {ttl:>2} -> {label}");
        if first_reply.is_none() && matches!(outcome, TcpOutcome::Reply(_) | TcpOutcome::Reset) {
            first_reply = Some(ttl);
        }
    }

    match first_reply {
        Some(ttl) => println!(
            "\nSONUÇ: ilk yanıt TTL {ttl} ile geldi; TTL denetimi çalışıyor.\n\
             Engelli bir ağda bu eşik DPI'ın mesafesini verir."
        ),
        None => println!(
            "\nSONUÇ: hiçbir TTL'de yanıt alınamadı. TTL denetimi bu sistemde\n\
             etkisiz olabilir; strateji üretimi ölçüm yerine varsayılan aralığa düşer."
        ),
    }
    Ok(())
}

/// Bulunan stratejiyi kalıcı olarak uygular.
fn apply(backend: Backend, strategy: &Strategy, diagnosis: String, complete: bool) -> Result<()> {
    // Önceki motor varsa önce temizlenmeli; iki motorun aynı anda aynı
    // paketlere müdahale etmesi bağlantıyı tamamen bozuyor.
    engine::stop_detached()?;

    let list = hostlist();
    let session = engine::start(backend, strategy, list.as_deref())?;
    let channel = session.channel;
    let proxy = match channel {
        Channel::Socks(address) => Some(address.to_string()),
        Channel::Direct => None,
    };
    session.detach()?;

    let message = if complete {
        "tüm hedefler açıldı".to_owned()
    } else {
        "hedeflerin bir kısmı açıldı; en iyi bulunan strateji uygulandı".to_owned()
    };
    let mut current = Status::idle(message);
    current.active = true;
    current.profile = strategy.id();
    current.method = backend.description().to_owned();
    current.backend = Some(backend.as_str().to_owned());
    current.strategy = Some(strategy.clone());
    current.diagnosis = Some(diagnosis);
    current.proxy = proxy.clone();
    status::write(&current)?;

    println!("\nUygulandı: {strategy}");
    println!("Motor: {}", backend.description());
    if let Some(address) = proxy {
        println!(
            "\nProxy modu etkin. Uygulamaları şu SOCKS5 adresine yönlendirin:\n  {address}\n\
             (Şeffaf mod için yönetici/root yetkisiyle çalıştırın.)"
        );
    }
    Ok(())
}

fn auto() -> Result<()> {
    let list = hostlist();
    let mut progress = CliProgress::default();
    let outcome = search::run(
        &DEFAULT_TARGETS,
        list.as_deref(),
        search::DEFAULT_ATTEMPT_LIMIT,
        &mut progress,
    )?;

    let diagnosis = outcome.report.dominant.summary();
    match (&outcome.winner, outcome.backend) {
        (Some(strategy), Some(backend)) => apply(backend, strategy, diagnosis, outcome.complete),
        // Engel yoksa müdahale etmiyoruz: çalışan bir bağlantıyı kurcalamak
        // iyileştirmek yerine bozma riski taşıyor.
        _ if outcome.complete => {
            println!("Bu ağda engelleme tespit edilmedi; müdahaleye gerek yok.");
            status::write(&Status::idle("engelleme tespit edilmedi"))?;
            Ok(())
        }
        _ => {
            status::write(&Status::idle("hiçbir strateji işe yaramadı"))?;
            bail!(
                "denenen {} stratejinin hiçbiri hedefleri açamadı; teşhis: {diagnosis}",
                outcome.attempts.len()
            )
        }
    }
}

fn stop() -> Result<()> {
    engine::stop_detached()?;
    status::write(&Status::idle("durduruldu"))?;
    println!("Durduruldu.");
    Ok(())
}

fn show_status() -> Result<()> {
    println!("{}", serde_json::to_string_pretty(&status::read())?);
    Ok(())
}

/// Mevcut durumda hedeflere erişilip erişilemediğini ölçer.
fn test() -> Result<()> {
    let current = status::read();
    let channel = match current.proxy.as_deref().and_then(|a| a.parse().ok()) {
        Some(address) => Channel::Socks(address),
        None => Channel::Direct,
    };
    let result = verify::verify(channel, &DEFAULT_TARGETS);
    for host in &result.hosts {
        println!(
            "{} {} — {}",
            if host.ok { "✓" } else { "✗" },
            host.host,
            host.detail
        );
    }
    if !result.is_complete() {
        bail!("{}/{} hedef açık", result.passed(), result.total());
    }
    println!("\nTüm hedefler açık.");
    Ok(())
}

/// Bu sistemde hangi motorların kullanılabilir olduğunu listeler.
fn backends() -> Result<()> {
    println!(
        "Yetki: {}",
        if privilege::is_elevated() {
            "var"
        } else {
            "yok"
        }
    );
    println!("Motor dizini: {}", paths::engine_dir().display());
    println!();
    for backend in engine::preference_order() {
        println!(
            "{:<8} {:<34} kurulu: {:<5} yetki gerekir: {}",
            backend.as_str(),
            backend.description(),
            backend.is_installed(),
            backend.requires_elevation()
        );
    }
    println!();
    match engine::select() {
        Ok(chosen) => println!(
            "Seçilecek motor: {} ({})",
            chosen.as_str(),
            chosen.description()
        ),
        Err(error) => println!("Kullanılabilir motor yok: {error:#}"),
    }
    Ok(())
}

/// Ortamı makine-okunur biçimde bildirir.
///
/// GUI bunu okuyup hangi düğmeleri etkinleştireceğine karar veriyor. Ayrı bir
/// komut olmasının nedeni, `backends` çıktısının insan için biçimlendirilmiş
/// olması: onu ayrıştırmak, metni her değiştirdiğimizde arayüzü bozardı.
fn info() -> Result<()> {
    #[derive(serde::Serialize)]
    struct BackendInfo {
        name: &'static str,
        description: &'static str,
        installed: bool,
        requires_elevation: bool,
    }
    #[derive(serde::Serialize)]
    struct Info {
        elevated: bool,
        engine_dir: String,
        backends: Vec<BackendInfo>,
        selected: Option<&'static str>,
        version: &'static str,
    }

    let backends: Vec<BackendInfo> = engine::preference_order()
        .into_iter()
        .map(|backend| BackendInfo {
            name: backend.as_str(),
            description: backend.description(),
            installed: backend.is_installed(),
            requires_elevation: backend.requires_elevation(),
        })
        .collect();

    println!(
        "{}",
        serde_json::to_string_pretty(&Info {
            elevated: privilege::is_elevated(),
            engine_dir: paths::engine_dir().display().to_string(),
            backends,
            selected: engine::select().ok().map(Backend::as_str),
            version: env!("CARGO_PKG_VERSION"),
        })?
    );
    Ok(())
}

/// Proxy motorunu indirir ve kurar.
fn install_engine(force: bool) -> Result<()> {
    let manifest = turkdpi::fetch::default_manifest();
    println!("Bildirim: {}", manifest.display());
    println!("Hedef: {}", turkdpi::fetch::target_key());
    let installed = turkdpi::fetch::install_byedpi(&manifest, force)?;
    println!("Kuruldu: {}", installed.display());
    Ok(())
}

const USAGE: &str = "kullanım: turkdpi-service <komut>

  auto                  ağı teşhis et, çalışan stratejiyi bul ve uygula
  stop                  motoru durdur ve kuralları geri al
  status                mevcut durumu JSON olarak yaz
  test                  hedeflere erişimi ölç
  diagnose [alan adı…]  engellemenin nasıl yapıldığını teşhis et
  selftest [alan adı]   TTL ölçüm düzeneğini doğrula
  backends              kullanılabilir motorları listele
  info                  ortamı JSON olarak bildir (GUI için)
  logs [satır]          günlük dosyalarını göster (varsayılan son 200 satır)
  set-dns <mod>         cloudflare | automatic (şimdilik yalnızca Linux)
  install-engine [-f]   proxy motorunu indir ve doğrula";

/// Sistem DNS'ini Cloudflare'e alır ya da geri yükler.
///
/// Teşhis DNS zehirlemesi bulduğunda asıl çözüm bu. NetworkManager'a bağlı
/// olduğu için şimdilik yalnızca Linux'ta var; Windows ve macOS'ta zehirlenmiş
/// DNS, proxy motorunun isim çözümlemeyi kendi tarafında yapmasıyla aşılıyor.
#[cfg(target_os = "linux")]
fn set_dns(mode: &str) -> Result<()> {
    if !privilege::is_elevated() {
        bail!("{}", privilege::elevation_hint());
    }
    let state = paths::state_dir();
    let message = match mode {
        "cloudflare" => turkdpi::dnscfg::apply_cloudflare(&state)?,
        "automatic" => turkdpi::dnscfg::restore(&state)?,
        _ => bail!("izin verilmeyen DNS şablonu: {mode} (cloudflare|automatic)"),
    };
    let mut current = status::read();
    current.dns_cloudflare = turkdpi::dnscfg::is_cloudflare_active(&state);
    current.message = message.clone();
    status::write(&current)?;
    log::info(&format!("DNS: {message}"));
    println!("{message}");
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn set_dns(_mode: &str) -> Result<()> {
    bail!(
        "DNS değiştirme şu anda yalnızca Linux'ta destekleniyor.\n\
         Bu platformda proxy motoru isim çözümlemeyi kendi tarafında yaptığı için\n\
         zehirlenmiş DNS zaten atlatılıyor."
    )
}

/// Günlükleri listeler ve en yenisinin sonunu yazar.
///
/// Kullanıcının hata bildirirken göndereceği dosya bu; yolun açıkça
/// yazılması, "günlük nerede" sorusunu ortadan kaldırıyor.
fn logs(tail: usize) -> Result<()> {
    let directory = paths::log_dir();
    println!("Günlük dizini: {}", directory.display());
    println!(
        "Saklama: son {} gün (daha eskiler otomatik siliniyor)",
        log::RETENTION_DAYS
    );

    let files = log::files();
    let Some(newest) = files.first() else {
        println!("\nHenüz günlük kaydı yok.");
        return Ok(());
    };

    println!("\nDosyalar:");
    for file in &files {
        let size = std::fs::metadata(file).map(|meta| meta.len()).unwrap_or(0);
        println!("  {} ({} KB)", file.display(), size / 1024);
    }

    let contents = std::fs::read_to_string(newest)?;
    let lines: Vec<&str> = contents.lines().collect();
    let start = lines.len().saturating_sub(tail);
    println!(
        "\n--- {} (son {} satır) ---",
        newest.display(),
        lines.len() - start
    );
    for line in &lines[start..] {
        println!("{line}");
    }
    Ok(())
}

fn main() -> Result<()> {
    // Eski günlükler her çalıştırmada temizleniyor. Ayrı bir zamanlayıcıya
    // gerek kalmıyor ve dizin hiçbir zaman sınırsız büyümüyor.
    log::prune();

    let args: Vec<String> = std::env::args().skip(1).collect();
    log::info(&format!("komut: turkdpi-service {}", args.join(" ")));

    let result = dispatch(&args);
    if let Err(error) = &result {
        // Hatalar günlüğe tam zinciriyle yazılıyor; kullanıcının gönderdiği
        // dosyadan kök nedene inebilmek için gerekli.
        log::error(&format!("{error:#}"));
    }
    result
}

fn dispatch(args: &[String]) -> Result<()> {
    match args {
        [command] if command == "auto" => auto(),
        [command] if command == "stop" || command == "cleanup" => stop(),
        [command] if command == "status" => show_status(),
        [command] if command == "test" => test(),
        [command] if command == "backends" => backends(),
        [command] if command == "info" => info(),
        [command, mode] if command == "set-dns" => set_dns(mode),
        [command] if command == "logs" => logs(200),
        [command, count] if command == "logs" => logs(count.parse().unwrap_or(200)),
        [command] if command == "diagnose" => diagnose(&DEFAULT_TARGETS),
        [command, rest @ ..] if command == "diagnose" => {
            let targets: Vec<&str> = rest.iter().map(String::as_str).collect();
            diagnose(&targets)
        }
        [command] if command == "selftest" => selftest("discord.com"),
        [command, host] if command == "selftest" => selftest(host),
        [command] if command == "install-engine" => install_engine(false),
        [command, flag] if command == "install-engine" && (flag == "-f" || flag == "--force") => {
            install_engine(true)
        }
        _ => bail!("{USAGE}"),
    }
}
