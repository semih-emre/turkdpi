//! Proxy motorunun uçtan uca çalıştığını doğrular.
//!
//! Bu test motoru gerçekten başlatıyor, yerel SOCKS5 proxy'sine bağlanıyor ve
//! proxy üzerinden gerçek bir TLS el sıkışması başlatıyor. Birim testlerin
//! yakalayamadığı şeyi yakalıyor: argümanların motor tarafından kabul
//! edildiğini ve proxy yolunun uçtan uca kurulduğunu.
//!
//! Motor kurulu değilse ya da ağ yoksa test atlanıyor; CI'da motor kurulu
//! olduğu için orada gerçekten çalışıyor.

use turkdpi::engine::{self, Backend};
use turkdpi::strategy::{Desync, SplitAt, Strategy};
use turkdpi::verify::{self, Channel};

fn sample_strategy() -> Strategy {
    Strategy {
        desync: Desync::MultiSplit,
        ttl: None,
        fooling: Vec::new(),
        split: Some(SplitAt::SniMiddle),
        repeats: 1,
        tlsrec: None,
    }
}

/// Ağ erişimi olmadan doğrulama testleri anlamsız; CI'nın çevrimdışı
/// koştuğu durumlarda yanlış kırmızı vermemek için önce bunu kontrol ediyoruz.
fn network_available() -> bool {
    use std::net::{TcpStream, ToSocketAddrs};
    use std::time::Duration;
    ("example.com", 443)
        .to_socket_addrs()
        .ok()
        .and_then(|mut addresses| addresses.next())
        .map(|address| TcpStream::connect_timeout(&address, Duration::from_secs(5)).is_ok())
        .unwrap_or(false)
}

#[test]
fn the_proxy_engine_carries_real_traffic() {
    if !Backend::ByeDpi.is_installed() {
        eprintln!("atlandı: ciadpi kurulu değil");
        return;
    }
    if !network_available() {
        eprintln!("atlandı: ağ erişimi yok");
        return;
    }

    let session = engine::start(Backend::ByeDpi, &sample_strategy(), None)
        .expect("proxy motoru başlatılabilmeli");

    let Channel::Socks(address) = session.channel else {
        panic!("proxy motoru SOCKS kanalı sunmalı");
    };
    assert!(
        address.ip().is_loopback(),
        "proxy yalnızca geri döngüde dinlemeli, bulunan: {address}"
    );

    // Motorun dinlemeye başlaması bir an alıyor.
    std::thread::sleep(std::time::Duration::from_millis(800));

    let result = verify::verify(session.channel, &["example.com"]);
    assert!(
        result.is_complete(),
        "proxy üzerinden el sıkışma tamamlanmalı: {:?}",
        result.hosts
    );
}

#[test]
fn a_session_stops_cleanly_when_dropped() {
    if !Backend::ByeDpi.is_installed() {
        eprintln!("atlandı: ciadpi kurulu değil");
        return;
    }

    let address = {
        let session =
            engine::start(Backend::ByeDpi, &sample_strategy(), None).expect("başlatılabilmeli");
        let Channel::Socks(address) = session.channel else {
            panic!("SOCKS kanalı bekleniyor");
        };
        address
        // `session` burada düşüyor ve motor kapanmalı.
    };

    std::thread::sleep(std::time::Duration::from_millis(600));

    // Motor kapandıysa portu yeniden bağlayabilmeliyiz. Bu, strateji araması
    // için kritik: onlarca motor arka arkaya açılıp kapanıyor ve sızan bir
    // süreç sonraki denemeleri bozardı.
    assert!(
        std::net::TcpListener::bind(address).is_ok(),
        "oturum düştükten sonra port serbest kalmalı: {address}"
    );
}
