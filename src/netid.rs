//! Bağlı olunan ağın parmak izi.
//!
//! Bir ağ için çalışan strateji öğrenildiğinde kaydediliyor; aynı ağa tekrar
//! bağlanıldığında arama yapmadan doğrudan uygulanabiliyor. Bu, ev ağında
//! saniyeler içinde bağlanmayı sağlıyor.
//!
//! Parmak izi, dışarı çıkan arayüzün yerel IP'sinin /24 bloğundan üretiliyor.
//! Bu yöntem dış komut çalıştırmadığı için üç platformda da aynı şekilde
//! çalışıyor, ama kusursuz değil: iki farklı ağ aynı özel blokta olabilir
//! (192.168.1.0/24 çok yaygın). Bu kabul edilebilir, çünkü kayıtlı strateji
//! körü körüne uygulanmıyor — önce doğrulanıyor, tutmazsa tam arama yapılıyor.
//! Yanlış eşleşmenin bedeli birkaç saniye, kazancı ise her seferinde tam
//! aramadan kurtulmak.

use std::net::UdpSocket;

/// Bağlı olunan ağ için kısa, kararlı bir kimlik döndürür.
///
/// Ağ tespit edilemezse `None` döner; bu durumda öğrenme devre dışı kalır.
pub fn fingerprint() -> Option<String> {
    let local = outbound_address()?;
    Some(format!("net-{:08x}", fnv1a(local.as_bytes())))
}

/// Dışarıya çıkarken kullanılan yerel arayüzün adresini bulur.
///
/// UDP soketinde `connect` çağrısı hiçbir paket göndermez; yalnızca çekirdeğin
/// yönlendirme tablosuna bakıp yerel uç noktayı seçmesini sağlar. Bu sayede
/// hedefin erişilebilir olması bile gerekmiyor.
fn outbound_address() -> Option<String> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("1.1.1.1:53").ok()?;
    let address = socket.local_addr().ok()?;
    match address.ip() {
        std::net::IpAddr::V4(v4) => {
            let [a, b, c, _] = v4.octets();
            // Son sekizli DHCP ile değişebildiği için dışarıda bırakılıyor.
            Some(format!("{a}.{b}.{c}.0/24"))
        }
        std::net::IpAddr::V6(v6) => Some(v6.to_string()),
    }
}

/// FNV-1a: kısa girdiler için yeterli, bağımlılıksız ve platformlar arasında
/// aynı sonucu veren bir karma.
fn fnv1a(data: &[u8]) -> u32 {
    let mut hash: u32 = 0x811c_9dc5;
    for &byte in data {
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hash_is_stable_across_runs() {
        assert_eq!(fnv1a(b"192.168.1.0/24"), fnv1a(b"192.168.1.0/24"));
    }

    #[test]
    fn different_networks_get_different_identifiers() {
        assert_ne!(fnv1a(b"192.168.1.0/24"), fnv1a(b"10.0.0.0/24"));
    }

    #[test]
    fn a_fingerprint_is_filename_safe() {
        // Kimlik dosya adı olarak kullanılıyor; yol ayırıcısı içermemeli.
        let identifier = format!("net-{:08x}", fnv1a(b"192.168.1.0/24"));
        assert!(
            identifier
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-'),
            "dosya adı güvenli olmalı: {identifier}"
        );
    }
}
