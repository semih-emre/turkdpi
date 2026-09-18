//! Linux şeffaf arka ucu: nftables NFQUEUE + `nfqws`.
//!
//! Paketler nftables kuralları ile kullanıcı alanındaki `nfqws` süreçlerine
//! yönlendiriliyor, orada değiştirilip geri salınıyor. Windows'tan farkı,
//! yakalama filtresinin motorun içinde değil çekirdek kural tablosunda
//! tanımlanması; bu yüzden burada kuralların geri alınması gerekiyor.

use super::spawn::spawn_checked;
use super::{Backend, Session};
use crate::strategy::Strategy;
use crate::util;
use crate::verify::Channel;
use crate::{paths, privilege};
use anyhow::{bail, Context, Result};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::process::{Command, Stdio};

/// TCP ve UDP için ayrı kuyruklar. `bypass` bayrağı önemli: motor çökerse
/// paketler kuyrukta birikmek yerine normal akışına devam ediyor, yani
/// kullanıcının interneti kesilmiyor.
const TCP_QUEUE: u16 = 200;
const UDP_QUEUE: u16 = 201;

/// Motorun kendi ürettiği paketleri tekrar yakalamamak için kullanılan işaret.
const FWMARK: &str = "0x40000000";

const RULESET: &str = r#"
table inet turkdpi {
  chain postrouting {
    type filter hook postrouting priority 102; policy accept;
    meta mark & 0x40000000 == 0 meta l4proto tcp tcp dport { 80, 443 } queue num 200 bypass
    meta mark & 0x40000000 == 0 oifname != "lo" meta l4proto udp udp dport { 443, 1024-65535 } queue num 201 bypass
  }
  chain prerouting {
    type filter hook prerouting priority -102; policy accept;
    meta l4proto tcp tcp sport { 80, 443 } tcp flags & (syn | ack) == (syn | ack) queue num 200 bypass
  }
}
"#;

fn nft_binary() -> &'static str {
    if Path::new("/usr/sbin/nft").is_file() {
        "/usr/sbin/nft"
    } else {
        "/usr/bin/nft"
    }
}

fn run_nft(args: &[&str], stdin: Option<&[u8]>) -> Result<std::process::Output> {
    let mut child = Command::new(nft_binary())
        .args(args)
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("nft çalıştırılamadı")?;
    if let Some(data) = stdin {
        child
            .stdin
            .take()
            .context("nft stdin açılamadı")?
            .write_all(data)?;
    }
    Ok(child.wait_with_output()?)
}

/// Mevcut kural setini yedekler. Bir şeyler ters giderse kullanıcı kendi
/// kurallarını geri yükleyebilsin diye.
fn backup_ruleset() -> Result<()> {
    let backup_dir = paths::state_dir().join("backups");
    fs::create_dir_all(&backup_dir)?;
    let output = run_nft(&["-j", "list", "ruleset"], None)?;
    if !output.status.success() {
        bail!(
            "nftables yedeği alınamadı: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let path = backup_dir.join(format!("ruleset-{}.json", util::utc_timestamp()));
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(&output.stdout)?;
    Ok(())
}

fn install_rules() -> Result<()> {
    // Önce sözdizimi denetimi: yarım uygulanmış bir kural seti interneti
    // tamamen kesebilirdi.
    let check = run_nft(&["-c", "-f", "-"], Some(RULESET.as_bytes()))?;
    if !check.status.success() {
        bail!(
            "nft kural doğrulaması başarısız: {}",
            String::from_utf8_lossy(&check.stderr)
        );
    }
    let output = run_nft(&["-f", "-"], Some(RULESET.as_bytes()))?;
    if !output.status.success() {
        bail!(
            "nft kuralları uygulanamadı: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

/// Kuralları geri alır. Tablo yoksa sessizce başarılı sayılıyor: temizlik
/// işlemi her durumda güvenle çağrılabilmeli.
pub fn remove_rules() -> Result<()> {
    let exists = run_nft(&["list", "table", "inet", "turkdpi"], None)?;
    if !exists.status.success() {
        return Ok(());
    }
    let output = run_nft(&["delete", "table", "inet", "turkdpi"], None)?;
    if !output.status.success() {
        bail!(
            "turkdpi tablosu silinemedi: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

fn engine_args(
    queue: u16,
    strategy: &Strategy,
    hostlist: Option<&Path>,
    quic: bool,
) -> Vec<String> {
    let mut args = vec![
        format!("--qnum={queue}"),
        "--user=nobody".to_owned(),
        format!("--dpi-desync-fwmark={FWMARK}"),
    ];
    if quic {
        args.extend(strategy.zapret_quic_args());
    } else {
        args.extend(strategy.zapret_args());
    }
    if let Some(list) = hostlist {
        args.push(format!("--hostlist={}", list.display()));
    }
    args
}

pub fn start(strategy: &Strategy, hostlist: Option<&Path>) -> Result<Session> {
    if !privilege::is_elevated() {
        bail!("{}", privilege::elevation_hint());
    }
    // Kullanıcının kendi güvenlik duvarı kurallarını değiştirmeden önce
    // yedekle. Yedek alınamaması ölümcül değil: kural eklemek mevcut kuralları
    // silmiyor, sadece geri dönüş kolaylığı kaybediliyor.
    if let Err(error) = backup_ruleset() {
        eprintln!("uyarı: nftables yedeği alınamadı: {error:#}");
    }

    let binary = paths::engine_binary(Backend::Nfqws.as_str());

    // Motorlar kurallardan önce başlatılıyor. Ters sırada yapılsaydı,
    // kuyruklar hazırken dinleyen olmadığı için paketler kısa süreliğine
    // `bypass` ile işlenmeden geçerdi.
    let tcp = spawn_checked(
        &binary,
        &engine_args(TCP_QUEUE, strategy, hostlist, false),
        "nfqws (TCP)",
    )?;
    let udp = spawn_checked(
        &binary,
        &engine_args(UDP_QUEUE, strategy, hostlist, true),
        "nfqws (QUIC)",
    )?;

    let mut session = Session {
        backend: Backend::Nfqws,
        channel: Channel::Direct,
        children: vec![tcp, udp],
        firewall_installed: false,
        detached: false,
    };

    // Kural yükleme başarısız olursa `session` burada düşer ve `Drop`
    // motorları kapatır; arkada başıboş süreç kalmaz.
    install_rules()?;
    session.firewall_installed = true;
    Ok(session)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::strategy::{Desync, Fooling};

    fn sample() -> Strategy {
        Strategy {
            desync: Desync::Fake,
            ttl: Some(4),
            fooling: vec![Fooling::BadSum],
            split: None,
            repeats: 1,
            tlsrec: None,
        }
    }

    #[test]
    fn tcp_and_quic_use_separate_queues() {
        let tcp = engine_args(TCP_QUEUE, &sample(), None, false);
        let quic = engine_args(UDP_QUEUE, &sample(), None, true);
        assert!(tcp.contains(&"--qnum=200".to_owned()));
        assert!(quic.contains(&"--qnum=201".to_owned()));
        assert_ne!(TCP_QUEUE, UDP_QUEUE);
    }

    #[test]
    fn the_engine_ignores_its_own_packets() {
        // İşaret verilmezse motor kendi ürettiği sahte paketleri tekrar
        // yakalar ve sonsuz döngüye girer.
        let args = engine_args(TCP_QUEUE, &sample(), None, false);
        assert!(args.iter().any(|arg| arg.contains("--dpi-desync-fwmark=")));
    }

    #[test]
    fn a_hostlist_is_passed_through_when_given() {
        let path = Path::new("/usr/share/turkdpi/discord-hosts.txt");
        let args = engine_args(TCP_QUEUE, &sample(), Some(path), false);
        assert!(args
            .iter()
            .any(|arg| arg == "--hostlist=/usr/share/turkdpi/discord-hosts.txt"));
        assert!(engine_args(TCP_QUEUE, &sample(), None, false)
            .iter()
            .all(|arg| !arg.starts_with("--hostlist")));
    }

    #[test]
    fn queued_packets_fall_through_if_the_engine_dies() {
        // `bypass` olmadan motor çöktüğünde tüm trafik kuyrukta kalır ve
        // kullanıcının interneti tamamen kesilir.
        let queue_rules = RULESET
            .lines()
            .filter(|line| line.contains("queue num"))
            .collect::<Vec<_>>();
        assert!(!queue_rules.is_empty(), "kuyruk kuralı bulunmalı");
        for rule in queue_rules {
            assert!(rule.contains("bypass"), "bypass eksik: {rule}");
        }
    }
}
