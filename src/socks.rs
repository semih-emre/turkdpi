//! Asgari SOCKS5 istemcisi (RFC 1928).
//!
//! Proxy motoru (ByeDPI) yalnızca kendi üzerinden geçen trafiği değiştirir.
//! Bu yüzden bir stratejinin işe yarayıp yaramadığını ölçerken bağlantıyı
//! proxy üzerinden kurmamız gerekiyor; doğrudan bağlanmak motoru tamamen
//! atlar ve her strateji "başarısız" görünür.
//!
//! Yalnızca kimlik doğrulamasız CONNECT destekleniyor; yerel proxy'ye bağlanmak
//! için gereken tek şey bu.

use std::io::{Read, Result as IoResult, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

/// SOCKS5 proxy üzerinden bir alan adına TCP bağlantısı açar.
///
/// İsim çözümlemesi bilerek proxy'ye bırakılıyor (ATYP=3, alan adı). Yerel
/// çözümleyici zehirliyse doğru adrese gitmek başka türlü mümkün olmuyor.
pub fn connect_through(
    proxy: SocketAddr,
    host: &str,
    port: u16,
    timeout: Duration,
) -> IoResult<TcpStream> {
    let mut stream = TcpStream::connect_timeout(&proxy, timeout)?;
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(timeout))?;
    stream.set_nodelay(true)?;

    // Selamlama: tek yöntem sunuyoruz, "kimlik doğrulama yok".
    stream.write_all(&[0x05, 0x01, 0x00])?;
    let mut greeting = [0_u8; 2];
    stream.read_exact(&mut greeting)?;
    if greeting[0] != 0x05 || greeting[1] != 0x00 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::ConnectionRefused,
            "SOCKS5 proxy kimlik doğrulamasız bağlantıyı reddetti",
        ));
    }

    stream.write_all(&connect_request(host, port)?)?;

    // Yanıt: sürüm, durum, ayrılmış, adres tipi.
    let mut head = [0_u8; 4];
    stream.read_exact(&mut head)?;
    if head[1] != 0x00 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::ConnectionRefused,
            format!("SOCKS5 bağlantısı reddedildi (durum {})", head[1]),
        ));
    }
    // Bağlanılan adres okunup atılıyor; uzunluğu adres tipine bağlı.
    let address_length = match head[3] {
        0x01 => 4,
        0x04 => 16,
        0x03 => {
            let mut length = [0_u8; 1];
            stream.read_exact(&mut length)?;
            length[0] as usize
        }
        other => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("bilinmeyen SOCKS5 adres tipi {other}"),
            ))
        }
    };
    let mut discard = vec![0_u8; address_length + 2]; // adres + port
    stream.read_exact(&mut discard)?;
    Ok(stream)
}

/// CONNECT isteğini kodlar.
fn connect_request(host: &str, port: u16) -> IoResult<Vec<u8>> {
    if host.is_empty() || host.len() > 255 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "SOCKS5 alan adı 1-255 bayt olmalı",
        ));
    }
    let mut request = Vec::with_capacity(host.len() + 7);
    request.extend_from_slice(&[0x05, 0x01, 0x00, 0x03]); // CONNECT, alan adı
    request.push(host.len() as u8);
    request.extend_from_slice(host.as_bytes());
    request.extend_from_slice(&port.to_be_bytes());
    Ok(request)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_connect_request_is_encoded_per_rfc1928() {
        let request = connect_request("discord.com", 443).expect("kodlanmalı");
        assert_eq!(request[0], 0x05, "SOCKS sürümü");
        assert_eq!(request[1], 0x01, "CONNECT komutu");
        assert_eq!(request[3], 0x03, "alan adı tipi");
        assert_eq!(request[4], 11, "discord.com 11 bayt");
        assert_eq!(&request[5..16], b"discord.com");
        assert_eq!(&request[16..18], &443_u16.to_be_bytes());
    }

    #[test]
    fn oversized_and_empty_hosts_are_rejected() {
        // Uzunluk tek bayta sığmalı; aksi halde paket bozulur.
        assert!(connect_request(&"a".repeat(256), 443).is_err());
        assert!(connect_request("", 443).is_err());
    }

    #[test]
    fn a_maximum_length_host_is_still_accepted() {
        let host = "a".repeat(255);
        let request = connect_request(&host, 80).expect("255 bayt sınırda geçerli");
        assert_eq!(request[4], 255);
    }
}
