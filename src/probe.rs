//! Engellemenin *nasıl* yapıldığını ölçen teşhis katmanı.
//!
//! Eski `auto` komutu dört sabit profili sırayla deniyordu. Bu, operatörün
//! hangi yöntemi kullandığını bilmediği için bazı ağlarda hiçbir zaman doğru
//! kombinasyonu bulamıyordu. Buradaki kod önce ölçüyor: DNS zehirleniyor mu,
//! ClientHello'dan sonra RST mi enjekte ediliyor yoksa paket sessizce mi
//! düşürülüyor, engel sayfası mı dönüyor ve en önemlisi DPI kaç hop uzakta.
//!
//! Son madde pratikte en değerli olanı: `--dpi-desync-ttl` değerinin doğru
//! seçilmesi, sahte paketin DPI'ı kandırıp gerçek sunucuya ulaşmamasını sağlar.
//! Bu değer operatöre ve hatta abonenin bağlantı tipine göre değişir, tahmin
//! edilemez; ölçülmesi gerekir.

use crate::wire::{self, TlsReply};
use serde::{Deserialize, Serialize};
use std::io::{ErrorKind, Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream, ToSocketAddrs, UdpSocket};
use std::time::Duration;

const CONNECT_TIMEOUT: Duration = Duration::from_millis(4000);
const REPLY_TIMEOUT: Duration = Duration::from_millis(2500);
/// TTL taramasında kullanılır. Paket zaten yolda ölecekse uzun beklemenin
/// anlamı yok; tarama 12 adım olduğu için kısa tutmak toplam süreyi belirliyor.
const TTL_SCAN_TIMEOUT: Duration = Duration::from_millis(900);
const DNS_TIMEOUT: Duration = Duration::from_millis(2000);
/// DPI donanımları abonelik ağının kenarında durur; 12 hop fazlasıyla yeterli.
const MAX_TTL_SCAN: u8 = 12;

/// Karşılaştırma için kullanılan, engellenmediği varsayılan çözümleyiciler.
const REFERENCE_RESOLVERS: [&str; 3] = ["1.1.1.1:53", "8.8.8.8:53", "9.9.9.9:53"];

/// SNI'nin tetikleyici olup olmadığını anlamak için kullanılan zararsız alan
/// adı. Aynı IP'ye bu isimle bağlanmak çalışıyorsa engelleme IP tabanlı değil,
/// SNI tabanlıdır.
const CONTROL_SNI: &str = "www.example.com";

/// Tek bir TCP denemesinin sonucu.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TcpOutcome {
    /// Bağlantı kuruldu ve karşıdan anlamlı bir TLS yanıtı geldi.
    Reply(TlsReply),
    /// ClientHello gönderildikten sonra bağlantı sıfırlandı. Gerçek sunucular
    /// geçerli bir ClientHello'ya RST ile yanıt vermez; bu enjeksiyondur.
    Reset,
    /// Bağlantı açık kaldı ama hiçbir yanıt gelmedi: paket düşürülmüş.
    Timeout,
    /// TCP el sıkışması hiç tamamlanamadı.
    ConnectFailed,
}

/// Bir hedef için engelleme teşhisi.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Verdict {
    /// Erişim açık; bu hedef için müdahaleye gerek yok.
    Open,
    /// ClientHello'dan sonra RST enjekte ediliyor. Türkiye'deki en yaygın
    /// yöntem. `dpi_hop` ölçülebildiyse `--dpi-desync-ttl` için doğru değerdir.
    SniReset { dpi_hop: Option<u8> },
    /// ClientHello'dan sonra sessizlik: paket düşürülüyor.
    SniDrop,
    /// 443 portundan düz metin HTTP yanıtı geliyor: engel sayfası enjeksiyonu.
    BlockPage,
    /// Sistem çözümleyicisi referanstan farklı ve çalışmayan bir IP döndürüyor.
    DnsPoisoned {
        system: Vec<Ipv4Addr>,
        reference: Vec<Ipv4Addr>,
    },
    /// TCP bağlantısı hiç kurulamıyor: IP seviyesinde engelleme.
    IpBlocked,
    /// Ağ erişimi yok; bu bir engelleme değil, ölçüm yapılamadı demek.
    Unreachable,
}

impl Verdict {
    /// Bu teşhis bir engelleme mi anlatıyor?
    pub fn is_blocked(&self) -> bool {
        !matches!(self, Verdict::Open | Verdict::Unreachable)
    }

    pub fn summary(&self) -> String {
        match self {
            Verdict::Open => "erişim açık".into(),
            Verdict::SniReset { dpi_hop: Some(hop) } => {
                format!("SNI tabanlı RST enjeksiyonu (DPI {hop} hop uzakta)")
            }
            Verdict::SniReset { dpi_hop: None } => {
                "SNI tabanlı RST enjeksiyonu (DPI mesafesi ölçülemedi)".into()
            }
            Verdict::SniDrop => "SNI tabanlı paket düşürme".into(),
            Verdict::BlockPage => "engel sayfası enjeksiyonu".into(),
            Verdict::DnsPoisoned { .. } => "DNS zehirlemesi".into(),
            Verdict::IpBlocked => "IP seviyesinde engelleme".into(),
            Verdict::Unreachable => "ağ erişimi yok".into(),
        }
    }
}

/// Bir hedefin tam teşhis raporu.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TargetReport {
    pub host: String,
    pub verdict: Verdict,
    /// SNI tetikleyici mi? Aynı IP'ye zararsız bir isimle bağlanmayı dener.
    pub sni_triggered: bool,
    pub resolved: Vec<Ipv4Addr>,
}

/// Tek bir TCP + ClientHello denemesi yapar.
///
/// `ttl` verilirse ClientHello o TTL ile gönderilir. TCP el sıkışması normal
/// TTL ile tamamlandığı için bağlantı kurulur, ancak ClientHello yolda ölür.
/// DPI'ın mesafesini bu şekilde ölçüyoruz.
pub fn tcp_probe(address: SocketAddr, server_name: &str, ttl: Option<u32>) -> TcpOutcome {
    let Ok(mut stream) = TcpStream::connect_timeout(&address, CONNECT_TIMEOUT) else {
        return TcpOutcome::ConnectFailed;
    };
    let reply_timeout = if ttl.is_some() {
        TTL_SCAN_TIMEOUT
    } else {
        REPLY_TIMEOUT
    };
    if stream.set_read_timeout(Some(reply_timeout)).is_err() {
        return TcpOutcome::ConnectFailed;
    }
    let _ = stream.set_nodelay(true);
    if let Some(ttl) = ttl {
        if stream.set_ttl(ttl).is_err() {
            return TcpOutcome::ConnectFailed;
        }
    }

    let hello = wire::client_hello(server_name);
    match stream.write_all(&hello) {
        Ok(()) => {}
        Err(error) if error.kind() == ErrorKind::ConnectionReset => return TcpOutcome::Reset,
        Err(_) => return TcpOutcome::ConnectFailed,
    }
    let _ = stream.flush();

    let mut buffer = [0_u8; 1024];
    match stream.read(&mut buffer) {
        Ok(0) => TcpOutcome::Timeout,
        Ok(length) => TcpOutcome::Reply(wire::classify_tls_reply(&buffer[..length])),
        Err(error) => match error.kind() {
            ErrorKind::ConnectionReset | ErrorKind::ConnectionAborted => TcpOutcome::Reset,
            ErrorKind::WouldBlock | ErrorKind::TimedOut => TcpOutcome::Timeout,
            _ => TcpOutcome::ConnectFailed,
        },
    }
}

/// RST enjekte eden DPI'ın kaç hop uzakta olduğunu bulur.
///
/// TTL'i 1'den başlatıp artırıyoruz. Düşük TTL'de ClientHello DPI'a varmadan
/// ölür ve hiçbir şey dönmez; DPI'ın bulunduğu hop'a ulaşıldığı anda RST
/// gelmeye başlar. İlk RST üreten TTL, DPI'ın mesafesidir.
pub fn measure_dpi_hop(address: SocketAddr, server_name: &str) -> Option<u8> {
    (1..=MAX_TTL_SCAN)
        .find(|&ttl| tcp_probe(address, server_name, Some(u32::from(ttl))) == TcpOutcome::Reset)
}

/// Belirli bir DNS sunucusuna doğrudan UDP sorgusu göndererek A kayıtlarını alır.
pub fn resolve_via(resolver: &str, host: &str) -> Vec<Ipv4Addr> {
    let (transaction_id, query) = wire::dns_query(host, 1);
    let Ok(socket) = UdpSocket::bind("0.0.0.0:0") else {
        return Vec::new();
    };
    if socket.set_read_timeout(Some(DNS_TIMEOUT)).is_err()
        || socket.connect(resolver).is_err()
        || socket.send(&query).is_err()
    {
        return Vec::new();
    }
    let mut response = [0_u8; 1500];
    let Ok(length) = socket.recv(&mut response) else {
        return Vec::new();
    };
    let packet = &response[..length];
    // Yanıt bizim sorumuza ait değilse yok say: sahte yanıt yarışı olabilir.
    match wire::parse_dns_header(packet) {
        Some(header) if header.transaction_id == transaction_id => {}
        _ => return Vec::new(),
    }
    wire::parse_dns_a_records(packet)
}

/// İşletim sisteminin yapılandırılmış çözümleyicisini kullanır.
pub fn resolve_system(host: &str) -> Vec<Ipv4Addr> {
    (host, 443)
        .to_socket_addrs()
        .map(|addresses| {
            addresses
                .filter_map(|address| match address.ip() {
                    IpAddr::V4(v4) => Some(v4),
                    IpAddr::V6(_) => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

/// İlk yanıt veren referans çözümleyiciden adresleri alır.
fn resolve_reference(host: &str) -> Vec<Ipv4Addr> {
    for resolver in REFERENCE_RESOLVERS {
        let addresses = resolve_via(resolver, host);
        if !addresses.is_empty() {
            return addresses;
        }
    }
    Vec::new()
}

/// Bir adres üzerinde gerçek SNI ile erişim olup olmadığını söyler.
fn address_serves(address: Ipv4Addr, host: &str) -> bool {
    let socket = SocketAddr::new(IpAddr::V4(address), 443);
    matches!(
        tcp_probe(socket, host, None),
        TcpOutcome::Reply(TlsReply::ServerHello) | TcpOutcome::Reply(TlsReply::Alert)
    )
}

/// Bir hedefi baştan sona teşhis eder.
pub fn diagnose(host: &str) -> TargetReport {
    let system = resolve_system(host);
    let reference = resolve_reference(host);

    // Hangi adres üzerinden ölçeceğimizi seçiyoruz. Referans çözümleyici
    // yanıt verdiyse onu tercih ediyoruz: sistem DNS'i zehirliyse ölçümün
    // tamamı yanlış çıkar.
    let candidates: Vec<Ipv4Addr> = if !reference.is_empty() {
        reference.clone()
    } else {
        system.clone()
    };

    let Some(&address) = candidates.first() else {
        return TargetReport {
            host: host.to_owned(),
            verdict: Verdict::Unreachable,
            sni_triggered: false,
            resolved: Vec::new(),
        };
    };
    let socket = SocketAddr::new(IpAddr::V4(address), 443);

    // DNS zehirlemesi: iki çözümleyici ortak adres vermiyorsa ve sistemin
    // verdiği adres gerçekten hizmet vermiyorsa, yanıt sahte demektir.
    let disjoint = !system.is_empty()
        && !reference.is_empty()
        && !system.iter().any(|address| reference.contains(address));
    if disjoint
        && !system
            .iter()
            .take(2)
            .copied()
            .any(|a| address_serves(a, host))
    {
        return TargetReport {
            host: host.to_owned(),
            verdict: Verdict::DnsPoisoned {
                system: system.clone(),
                reference: reference.clone(),
            },
            sni_triggered: true,
            resolved: reference,
        };
    }

    let outcome = tcp_probe(socket, host, None);
    // Zararsız bir isimle aynı adrese bağlanmak çalışıyorsa tetikleyici SNI'dir.
    let control = tcp_probe(socket, CONTROL_SNI, None);
    let sni_triggered = matches!(outcome, TcpOutcome::Reset | TcpOutcome::Timeout)
        && !matches!(control, TcpOutcome::ConnectFailed);

    let verdict = match outcome {
        TcpOutcome::Reply(TlsReply::ServerHello) | TcpOutcome::Reply(TlsReply::Alert) => {
            Verdict::Open
        }
        TcpOutcome::Reply(TlsReply::BlockPage) => Verdict::BlockPage,
        TcpOutcome::Reply(TlsReply::Unknown) => Verdict::SniDrop,
        TcpOutcome::Reset => Verdict::SniReset {
            dpi_hop: measure_dpi_hop(socket, host),
        },
        TcpOutcome::Timeout => Verdict::SniDrop,
        // Bağlantı hiç kurulamadıysa ama kontrol ismi de kurulamıyorsa sorun
        // adresin kendisinde; SNI ile ilgisi yok.
        TcpOutcome::ConnectFailed => Verdict::IpBlocked,
    };

    TargetReport {
        host: host.to_owned(),
        verdict,
        sni_triggered,
        resolved: candidates,
    }
}

/// Birden çok hedefi teşhis edip ortak bir sonuç çıkarır.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NetworkReport {
    pub targets: Vec<TargetReport>,
    /// Tüm hedeflerde gözlemlenen baskın engelleme yöntemi.
    pub dominant: Verdict,
    /// Ölçülebilen en küçük DPI mesafesi; strateji üretiminde kullanılır.
    pub dpi_hop: Option<u8>,
}

pub fn diagnose_all(hosts: &[&str]) -> NetworkReport {
    let targets: Vec<TargetReport> = hosts.iter().map(|host| diagnose(host)).collect();

    let dpi_hop = targets
        .iter()
        .filter_map(|target| match target.verdict {
            Verdict::SniReset { dpi_hop } => dpi_hop,
            _ => None,
        })
        .min();

    // Baskın yöntem: engellenen hedefler arasında en sık görülen teşhis.
    // Hiçbiri engelli değilse erişim açıktır.
    let blocked: Vec<&Verdict> = targets
        .iter()
        .map(|target| &target.verdict)
        .filter(|verdict| verdict.is_blocked())
        .collect();
    let dominant = blocked
        .iter()
        .max_by_key(|candidate| {
            blocked
                .iter()
                .filter(|other| {
                    std::mem::discriminant(**other) == std::mem::discriminant(**candidate)
                })
                .count()
        })
        .map(|verdict| (*verdict).clone())
        .unwrap_or(Verdict::Open);

    NetworkReport {
        targets,
        dominant,
        dpi_hop,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_and_unreachable_are_not_blocks() {
        assert!(!Verdict::Open.is_blocked());
        assert!(!Verdict::Unreachable.is_blocked());
    }

    #[test]
    fn every_interference_method_counts_as_a_block() {
        for verdict in [
            Verdict::SniReset { dpi_hop: Some(4) },
            Verdict::SniDrop,
            Verdict::BlockPage,
            Verdict::IpBlocked,
            Verdict::DnsPoisoned {
                system: vec![Ipv4Addr::new(195, 175, 254, 2)],
                reference: vec![Ipv4Addr::new(162, 159, 128, 233)],
            },
        ] {
            assert!(verdict.is_blocked(), "{verdict:?} engelleme sayılmalı");
        }
    }

    #[test]
    fn measured_hop_appears_in_the_summary() {
        let verdict = Verdict::SniReset { dpi_hop: Some(6) };
        assert!(verdict.summary().contains('6'));
    }

    #[test]
    fn verdict_survives_a_json_round_trip() {
        // GUI raporu JSON üzerinden okuduğu için biçim kararlı olmalı.
        let verdict = Verdict::SniReset { dpi_hop: Some(5) };
        let encoded = serde_json::to_string(&verdict).expect("serialise");
        assert!(encoded.contains("sni_reset"), "etiket snake_case olmalı");
        let decoded: Verdict = serde_json::from_str(&encoded).expect("deserialise");
        assert_eq!(decoded, verdict);
    }
}
