//! Bir stratejinin gerçekten işe yarayıp yaramadığını ölçer.
//!
//! Doğrulama, teşhisle aynı ham ClientHello makinesini kullanıyor: hedefe
//! gerçek SNI ile bağlanıp geçerli bir ServerHello dönüp dönmediğine bakıyor.
//! Bu, `curl` çağırmaktan iki açıdan daha iyi: dış bir programa bağımlı değil
//! (Windows'ta curl'ün varlığı garanti değil) ve "engellendi" ile "sunucu
//! kapalı" durumlarını ayırt edebiliyor.

use crate::socks;
use crate::wire::{self, TlsReply};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_millis(5000);

/// Doğrulamanın hangi yoldan yapılacağı.
#[derive(Clone, Copy, Debug)]
pub enum Channel {
    /// Şeffaf motor (nfqws, winws, tpws) tüm trafiği etkilediği için normal
    /// bağlantı kurulur.
    Direct,
    /// Proxy motoru (ByeDPI) yalnızca kendi üzerinden geçeni etkiler; bağlantı
    /// SOCKS5 üzerinden kurulmalı.
    Socks(SocketAddr),
}

/// Tek bir hedefin doğrulama sonucu.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HostResult {
    pub host: String,
    pub ok: bool,
    pub detail: String,
}

/// Bir stratejinin tüm hedefler üzerindeki sonucu.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VerifyResult {
    pub hosts: Vec<HostResult>,
}

impl VerifyResult {
    pub fn passed(&self) -> usize {
        self.hosts.iter().filter(|host| host.ok).count()
    }

    pub fn total(&self) -> usize {
        self.hosts.len()
    }

    /// Bütün hedefler çalışıyor mu?
    pub fn is_complete(&self) -> bool {
        !self.hosts.is_empty() && self.hosts.iter().all(|host| host.ok)
    }

    /// Kısmi başarı da değerli: Discord'un sohbeti açılıp sesin açılmaması,
    /// hiç açılmamasından iyidir ve daha iyi bir aday bulunamazsa kullanılır.
    pub fn score(&self) -> usize {
        self.passed()
    }
}

/// Tek bir hedefe bağlanıp TLS el sıkışmasının başlayıp başlamadığına bakar.
fn check_host(channel: Channel, host: &str) -> HostResult {
    let stream = match channel {
        Channel::Direct => resolve_and_connect(host),
        Channel::Socks(proxy) => {
            socks::connect_through(proxy, host, 443, TIMEOUT).map_err(|error| error.to_string())
        }
    };

    let mut stream = match stream {
        Ok(stream) => stream,
        Err(detail) => {
            return HostResult {
                host: host.to_owned(),
                ok: false,
                detail: format!("bağlantı kurulamadı: {detail}"),
            }
        }
    };

    let _ = stream.set_read_timeout(Some(TIMEOUT));
    if let Err(error) = stream.write_all(&wire::client_hello(host)) {
        return HostResult {
            host: host.to_owned(),
            ok: false,
            detail: format!("ClientHello gönderilemedi: {error}"),
        };
    }

    let mut buffer = [0_u8; 1024];
    match stream.read(&mut buffer) {
        Ok(0) => HostResult {
            host: host.to_owned(),
            ok: false,
            detail: "yanıt yok: paket düşürülüyor".into(),
        },
        Ok(length) => {
            let reply = wire::classify_tls_reply(&buffer[..length]);
            // Alert de kabul: sunucuya ulaştığımız anlamına geliyor, el
            // sıkışmanın kriptografik olarak tamamlanması bizim derdimiz değil.
            let ok = matches!(reply, TlsReply::ServerHello | TlsReply::Alert);
            HostResult {
                host: host.to_owned(),
                ok,
                detail: match reply {
                    TlsReply::ServerHello => "ServerHello alındı".into(),
                    TlsReply::Alert => "TLS uyarısı alındı (sunucuya ulaşıldı)".into(),
                    TlsReply::BlockPage => "engel sayfası döndü".into(),
                    TlsReply::Unknown => "tanınmayan yanıt".into(),
                },
            }
        }
        Err(error) => HostResult {
            host: host.to_owned(),
            ok: false,
            detail: format!("yanıt okunamadı: {error}"),
        },
    }
}

fn resolve_and_connect(host: &str) -> Result<TcpStream, String> {
    use std::net::ToSocketAddrs;
    let mut last_error = "ad çözümlenemedi".to_owned();
    let addresses = (host, 443)
        .to_socket_addrs()
        .map_err(|error| error.to_string())?;
    for address in addresses {
        match TcpStream::connect_timeout(&address, TIMEOUT) {
            Ok(stream) => {
                let _ = stream.set_nodelay(true);
                return Ok(stream);
            }
            Err(error) => last_error = error.to_string(),
        }
    }
    Err(last_error)
}

/// Verilen hedeflerin tamamını doğrular.
pub fn verify(channel: Channel, hosts: &[&str]) -> VerifyResult {
    VerifyResult {
        hosts: hosts.iter().map(|host| check_host(channel, host)).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result(flags: &[bool]) -> VerifyResult {
        VerifyResult {
            hosts: flags
                .iter()
                .enumerate()
                .map(|(index, &ok)| HostResult {
                    host: format!("host{index}.example"),
                    ok,
                    detail: String::new(),
                })
                .collect(),
        }
    }

    #[test]
    fn a_full_pass_is_complete() {
        let outcome = result(&[true, true, true]);
        assert!(outcome.is_complete());
        assert_eq!(outcome.passed(), 3);
        assert_eq!(outcome.total(), 3);
    }

    #[test]
    fn a_partial_pass_is_not_complete_but_still_scores() {
        let outcome = result(&[true, false, true]);
        assert!(!outcome.is_complete());
        assert_eq!(outcome.score(), 2, "kısmi başarı sıralamada sayılmalı");
    }

    #[test]
    fn an_empty_result_is_never_complete() {
        // Hiç hedef ölçülmediyse bunu başarı saymak, çalışmayan bir stratejinin
        // kazanan seçilmesine yol açardı.
        assert!(!result(&[]).is_complete());
        assert_eq!(result(&[]).score(), 0);
    }

    #[test]
    fn a_better_strategy_outranks_a_worse_one() {
        assert!(result(&[true, true, false]).score() > result(&[true, false, false]).score());
    }
}
