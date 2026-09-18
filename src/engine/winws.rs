//! Windows şeffaf arka ucu: `winws.exe` + WinDivert.
//!
//! zapret ailesinin Windows portu. Strateji argümanları `nfqws` ile birebir
//! aynı; fark, paketlerin nereden yakalandığında: Linux'ta nftables NFQUEUE
//! kuralları gerekirken, burada yakalama filtresi motorun kendi `--wf-*`
//! argümanlarıyla tanımlanıyor ve WinDivert sürücüsü üzerinden çalışıyor.
//!
//! Sürücü sürecin ömrüne bağlı olduğu için ayrıca temizlik gerekmiyor: süreç
//! öldüğünde yakalama da kalkıyor. Bu, Linux tarafındaki nftables temizliğine
//! göre önemli bir sadelik.

use super::spawn::spawn_checked;
use super::{Backend, Session};
use crate::paths;
use crate::strategy::Strategy;
use crate::verify::Channel;
use anyhow::{bail, Result};
use std::path::Path;

/// WinDivert sürücü dosyası. Motorun yanında bulunmazsa `winws` başlar ama
/// hiçbir paket yakalayamaz; bu sessiz başarısızlığı önceden yakalıyoruz.
const DRIVER_FILES: [&str; 2] = ["WinDivert.dll", "WinDivert64.sys"];

fn ensure_driver_present() -> Result<()> {
    let directory = paths::engine_dir();
    for file in DRIVER_FILES {
        if !directory.join(file).is_file() {
            bail!(
                "WinDivert sürücü dosyası eksik: {}",
                directory.join(file).display()
            );
        }
    }
    Ok(())
}

pub fn start(strategy: &Strategy, hostlist: Option<&Path>) -> Result<Session> {
    ensure_driver_present()?;

    // Yakalama filtresi: HTTP/HTTPS ve QUIC.
    let mut args = vec!["--wf-tcp=80,443".to_owned(), "--wf-udp=443".to_owned()];

    args.extend(strategy.zapret_args());
    if let Some(list) = hostlist {
        args.push(format!("--hostlist={}", list.display()));
    }

    // `--new` ile ikinci bir profil: QUIC. Discord ve YouTube QUIC üzerinden
    // konuştuğu için yalnızca TCP'yi düzeltmek çoğu zaman yetmiyor.
    args.push("--new".to_owned());
    args.extend(strategy.zapret_quic_args());
    if let Some(list) = hostlist {
        args.push(format!("--hostlist={}", list.display()));
    }

    let binary = paths::engine_binary(Backend::Winws.as_str());
    let child = spawn_checked(&binary, &args, "winws")?;

    Ok(Session {
        backend: Backend::Winws,
        channel: Channel::Direct,
        children: vec![child],
        // WinDivert yakalaması süreçle birlikte kalkıyor; geri alınacak kural yok.
        firewall_installed: false,
        detached: false,
    })
}

#[cfg(test)]
mod tests {
    use crate::strategy::{Desync, Fooling, Strategy};

    #[test]
    fn the_capture_filter_covers_both_tcp_and_quic() {
        // Yalnızca TCP yakalamak Discord sesini çözmüyor; QUIC de gerekli.
        let strategy = Strategy {
            desync: Desync::Fake,
            ttl: Some(4),
            fooling: vec![Fooling::BadSum],
            split: None,
            repeats: 1,
            tlsrec: None,
        };
        let quic = strategy.zapret_quic_args().join(" ");
        assert!(quic.contains("--filter-udp=443"));
        assert!(quic.contains("quic"));
    }
}
