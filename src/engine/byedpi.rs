//! ByeDPI (`ciadpi`) arka ucu: her platformda çalışan yerel SOCKS5 proxy.
//!
//! Bu, projenin çapraz platform tabanı. Çekirdek sürücüsü ya da yönetici
//! yetkisi istemediği için Windows, Linux ve macOS'ta koşulsuz çalışıyor.
//! Karşılığında yalnızca kendisine yönlendirilen trafiği etkiliyor; bu yüzden
//! yetki varsa şeffaf motor tercih ediliyor.

use super::spawn::spawn_checked;
use super::{Backend, Session};
use crate::paths;
use crate::strategy::Strategy;
use crate::verify::Channel;
use anyhow::{Context, Result};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener};
use std::path::Path;

/// Proxy'nin dinleyeceği adres.
///
/// Yalnızca geri döngü arayüzü: varsayılan `0.0.0.0` bırakılsaydı, aynı ağdaki
/// herkesin kullanabileceği açık bir SOCKS proxy'si yayınlamış olurduk.
const LISTEN_ADDRESS: Ipv4Addr = Ipv4Addr::LOCALHOST;

/// Kullanılabilir bir yerel port bulur.
///
/// Çekirdeğe 0 portuyla bağlanıp atadığı numarayı okuyoruz, sonra dinleyiciyi
/// bırakıyoruz. Bırakma ile motorun bağlanması arasında teorik bir yarış var,
/// ama port çekirdek tarafından yeni atandığı için pratikte yeniden
/// kullanılması olası değil ve motor bağlanamazsa bunu hemen fark ediyoruz.
fn free_port() -> Result<u16> {
    let listener =
        TcpListener::bind((LISTEN_ADDRESS, 0)).context("proxy için boş port ayrılamadı")?;
    let port = listener.local_addr()?.port();
    drop(listener);
    Ok(port)
}

pub fn start(strategy: &Strategy, hostlist: Option<&Path>) -> Result<Session> {
    let port = free_port()?;
    let mut args = vec![
        "--ip".to_owned(),
        LISTEN_ADDRESS.to_string(),
        "--port".to_owned(),
        port.to_string(),
    ];
    args.extend(strategy.byedpi_args());
    if let Some(list) = hostlist {
        args.push("--hosts".to_owned());
        args.push(list.display().to_string());
    }

    let binary = paths::engine_binary(Backend::ByeDpi.as_str());
    let child = spawn_checked(&binary, &args, "ciadpi")?;

    Ok(Session {
        backend: Backend::ByeDpi,
        channel: Channel::Socks(SocketAddr::new(IpAddr::V4(LISTEN_ADDRESS), port)),
        children: vec![child],
        firewall_installed: false,
        detached: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::strategy::{Desync, SplitAt};

    #[test]
    fn a_free_port_is_actually_bindable() {
        let port = free_port().expect("port ayrılmalı");
        assert_ne!(port, 0);
        // Bırakıldıktan sonra tekrar bağlanabilmeli; aksi halde motor da
        // bağlanamazdı.
        TcpListener::bind((LISTEN_ADDRESS, port)).expect("port serbest kalmalı");
    }

    #[test]
    fn successive_ports_do_not_collide() {
        let first = free_port().expect("ilk port");
        let _holder = TcpListener::bind((LISTEN_ADDRESS, first)).expect("ilk portu tut");
        let second = free_port().expect("ikinci port");
        assert_ne!(first, second, "kullanımdaki port tekrar verilmemeli");
    }

    #[test]
    fn the_proxy_only_listens_on_loopback() {
        // Bu bir güvenlik gereği: aksi halde ağdaki herkese açık bir SOCKS
        // proxy'si yayınlanmış olurdu.
        assert!(LISTEN_ADDRESS.is_loopback());
    }

    #[test]
    fn strategy_arguments_are_forwarded_to_the_engine() {
        let strategy = Strategy {
            desync: Desync::MultiSplit,
            ttl: None,
            fooling: Vec::new(),
            split: Some(SplitAt::SniMiddle),
            repeats: 1,
            tlsrec: None,
        };
        let rendered = strategy.byedpi_args();
        assert_eq!(rendered[0], "--split");
        assert_eq!(rendered[1], "0+sm");
    }
}
