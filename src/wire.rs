//! Ham protokol paketleri: TLS ClientHello, DNS sorgusu ve HTTP isteği.
//!
//! Bu modül bilerek hiçbir TLS ya da DNS kütüphanesine bağlı değildir. Amacımız
//! bir oturum kurmak değil, DPI'ın bu paketlere nasıl tepki verdiğini ölçmek.
//! Baytları kendimiz üretip gelen yanıtı kendimiz sınıflandırdığımızda "RST
//! enjekte edildi", "paket düşürüldü" ve "engel sayfası döndü" durumlarını
//! birbirinden ayırabiliyoruz; `curl` bunların hepsini tek bir hata olarak
//! raporluyor. Yan faydası: tüm bağımlılıklar saf Rust kalıyor, bu yüzden üç
//! platformda da C derleyicisi olmadan derleniyor.

use std::time::{SystemTime, UNIX_EPOCH};

/// ClientHello'nun rastgele alanları ve DNS işlem kimlikleri için küçük bir
/// xorshift üreteci. Kriptografik değildir ve olması da gerekmiyor: hiçbir
/// zaman bir el sıkışmayı tamamlamıyoruz, bu baytlar yalnızca paketin gerçekçi
/// görünmesini ve arka arkaya yapılan ölçümlerin birbirinden ayırt edilmesini
/// sağlıyor.
pub struct Rng(u64);

impl Rng {
    pub fn new() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9e37_79b9_7f4a_7c15);
        let stack_entropy = &nanos as *const u64 as u64;
        // Tohum asla sıfır olmamalı; xorshift sıfırda kilitlenir.
        Self(nanos ^ stack_entropy.rotate_left(17) | 1)
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    pub fn fill(&mut self, buffer: &mut [u8]) {
        for chunk in buffer.chunks_mut(8) {
            let bytes = self.next_u64().to_le_bytes();
            chunk.copy_from_slice(&bytes[..chunk.len()]);
        }
    }
}

impl Default for Rng {
    fn default() -> Self {
        Self::new()
    }
}

fn push_u16(buffer: &mut Vec<u8>, value: u16) {
    buffer.extend_from_slice(&value.to_be_bytes());
}

/// Gövdeyi `body` üretir, ardından önüne 16 bitlik uzunluğunu yazar. TLS
/// uzantılarının tamamı bu biçimde olduğu için uzunlukları elle saymaktan
/// kurtarıyor.
fn with_u16_length(buffer: &mut Vec<u8>, body: impl FnOnce(&mut Vec<u8>)) {
    let placeholder = buffer.len();
    push_u16(buffer, 0);
    body(buffer);
    let length = (buffer.len() - placeholder - 2) as u16;
    buffer[placeholder..placeholder + 2].copy_from_slice(&length.to_be_bytes());
}

fn extension(buffer: &mut Vec<u8>, kind: u16, body: impl FnOnce(&mut Vec<u8>)) {
    push_u16(buffer, kind);
    with_u16_length(buffer, body);
}

/// Yaygın bir tarayıcının gönderdiğine benzeyen, TLS 1.3 yetenekli bir
/// ClientHello üretir.
///
/// Paketin gerçekçi olması önemli: Türkiye'deki DPI donanımlarının bir kısmı
/// eksik ya da sıra dışı ClientHello'ları zaten ayrı bir kurala sokuyor, bu da
/// ölçümü kirletir. Burada üretilen paket gerçek bir sunucuya gönderildiğinde
/// geçerli bir ServerHello ile yanıtlanır.
pub fn client_hello(server_name: &str) -> Vec<u8> {
    let mut rng = Rng::new();
    let mut handshake = Vec::with_capacity(512);

    // client_version: kayıt katmanında TLS 1.2 gösterilir, gerçek sürüm
    // supported_versions uzantısında pazarlık edilir.
    push_u16(&mut handshake, 0x0303);

    let mut random = [0_u8; 32];
    rng.fill(&mut random);
    handshake.extend_from_slice(&random);

    let mut session_id = [0_u8; 32];
    rng.fill(&mut session_id);
    handshake.push(32);
    handshake.extend_from_slice(&session_id);

    const CIPHER_SUITES: [u16; 15] = [
        0x1302, 0x1303, 0x1301, // TLS 1.3
        0xc02c, 0xc030, 0xc02b, 0xc02f, 0xcca9, 0xcca8, // ECDHE AEAD
        0xc013, 0xc014, 0x009c, 0x009d, 0x002f, 0x0035, // eski ama yaygın
    ];
    with_u16_length(&mut handshake, |buffer| {
        for suite in CIPHER_SUITES {
            push_u16(buffer, suite);
        }
    });

    handshake.push(1); // compression_methods uzunluğu
    handshake.push(0); // null compression

    with_u16_length(&mut handshake, |extensions| {
        // server_name: ölçmek istediğimiz asıl alan bu.
        extension(extensions, 0x0000, |body| {
            with_u16_length(body, |list| {
                list.push(0); // host_name tipi
                with_u16_length(list, |name| name.extend_from_slice(server_name.as_bytes()));
            });
        });
        extension(extensions, 0x0017, |_| {}); // extended_master_secret
        extension(extensions, 0xff01, |body| body.push(0)); // renegotiation_info
        extension(extensions, 0x000a, |body| {
            with_u16_length(body, |groups| {
                for group in [0x001d_u16, 0x0017, 0x0018, 0x0019] {
                    push_u16(groups, group);
                }
            });
        });
        extension(extensions, 0x000b, |body| {
            body.push(1);
            body.push(0); // uncompressed
        });
        extension(extensions, 0x0023, |_| {}); // session_ticket
        extension(extensions, 0x0010, |body| {
            with_u16_length(body, |protocols| {
                for protocol in ["h2", "http/1.1"] {
                    protocols.push(protocol.len() as u8);
                    protocols.extend_from_slice(protocol.as_bytes());
                }
            });
        });
        extension(extensions, 0x000d, |body| {
            with_u16_length(body, |algorithms| {
                for algorithm in [
                    0x0403_u16, 0x0804, 0x0401, 0x0503, 0x0805, 0x0501, 0x0806, 0x0601,
                ] {
                    push_u16(algorithms, algorithm);
                }
            });
        });
        extension(extensions, 0x002b, |body| {
            body.push(4);
            push_u16(body, 0x0304); // TLS 1.3
            push_u16(body, 0x0303); // TLS 1.2
        });
        extension(extensions, 0x0033, |body| {
            with_u16_length(body, |shares| {
                push_u16(shares, 0x001d); // x25519
                with_u16_length(shares, |key| {
                    let mut public_key = [0_u8; 32];
                    rng.fill(&mut public_key);
                    key.extend_from_slice(&public_key);
                });
            });
        });
    });

    // Handshake başlığı: tip + 24 bitlik uzunluk.
    let mut message = Vec::with_capacity(handshake.len() + 9);
    message.push(0x01); // client_hello
    let length = handshake.len();
    message.push((length >> 16) as u8);
    message.push((length >> 8) as u8);
    message.push(length as u8);
    message.extend_from_slice(&handshake);

    // Kayıt katmanı başlığı.
    let mut record = Vec::with_capacity(message.len() + 5);
    record.push(0x16); // handshake
    push_u16(&mut record, 0x0301); // uyumluluk için TLS 1.0
    push_u16(&mut record, message.len() as u16);
    record.extend_from_slice(&message);
    record
}

/// Sunucudan dönen ilk baytların ne anlama geldiği.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TlsReply {
    /// Geçerli bir ServerHello geldi: ClientHello DPI'ı aşıp sunucuya ulaştı.
    ServerHello,
    /// TLS uyarısı geldi. Sunucuya ulaştık ama el sıkışma reddedildi; engelleme
    /// açısından başarı sayılır, çünkü paket hedefe varmış.
    Alert,
    /// 443 portundan düz metin HTTP yanıtı geldi. Bu bir engelleme sayfası
    /// enjeksiyonudur; gerçek sunucu asla böyle yanıt vermez.
    BlockPage,
    /// Yanıt geldi ama TLS'e benzemiyor.
    Unknown,
}

/// Bir bağlantıdan okunan ilk baytları sınıflandırır.
pub fn classify_tls_reply(data: &[u8]) -> TlsReply {
    if data.len() >= 4 && data[0] == 0x16 && data[1] == 0x03 {
        // Kayıt içindeki ilk handshake mesajının tipi 5. bayttadır.
        if data.len() >= 6 && data[5] == 0x02 {
            return TlsReply::ServerHello;
        }
        return TlsReply::Unknown;
    }
    if data.len() >= 2 && data[0] == 0x15 && data[1] == 0x03 {
        return TlsReply::Alert;
    }
    if data.starts_with(b"HTTP/") {
        return TlsReply::BlockPage;
    }
    TlsReply::Unknown
}

/// Verilen alan adı için A kaydı soran bir DNS sorgusu üretir.
/// Dönen değer `(işlem_kimliği, paket)`.
pub fn dns_query(domain: &str, record_type: u16) -> (u16, Vec<u8>) {
    let mut rng = Rng::new();
    let transaction_id = (rng.next_u64() as u16) | 1;
    let mut packet = Vec::with_capacity(domain.len() + 32);
    push_u16(&mut packet, transaction_id);
    push_u16(&mut packet, 0x0100); // standart sorgu, özyineleme istenir
    push_u16(&mut packet, 1); // soru sayısı
    push_u16(&mut packet, 0); // yanıt
    push_u16(&mut packet, 0); // yetkili
    push_u16(&mut packet, 0); // ek
    for label in domain.split('.').filter(|label| !label.is_empty()) {
        packet.push(label.len() as u8);
        packet.extend_from_slice(label.as_bytes());
    }
    packet.push(0); // kök etiketi
    push_u16(&mut packet, record_type);
    push_u16(&mut packet, 1); // IN sınıfı
    (transaction_id, packet)
}

/// DNS yanıtının başlığını çözer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DnsHeader {
    pub transaction_id: u16,
    pub response_code: u8,
    pub answer_count: u16,
}

pub fn parse_dns_header(packet: &[u8]) -> Option<DnsHeader> {
    if packet.len() < 12 {
        return None;
    }
    let flags = u16::from_be_bytes([packet[2], packet[3]]);
    if flags & 0x8000 == 0 {
        return None; // yanıt biti yok
    }
    Some(DnsHeader {
        transaction_id: u16::from_be_bytes([packet[0], packet[1]]),
        response_code: (flags & 0x000f) as u8,
        answer_count: u16::from_be_bytes([packet[6], packet[7]]),
    })
}

/// DNS yanıtındaki A kayıtlarını (IPv4 adreslerini) çıkarır.
///
/// Sıkıştırma işaretçilerini takip etmek yerine isim alanlarını yalnızca
/// atlıyoruz; A kaydının verisine ulaşmak için bu yeterli.
pub fn parse_dns_a_records(packet: &[u8]) -> Vec<std::net::Ipv4Addr> {
    let Some(header) = parse_dns_header(packet) else {
        return Vec::new();
    };
    let question_count = u16::from_be_bytes([packet[4], packet[5]]);
    let mut cursor = 12;

    let skip_name = |packet: &[u8], cursor: &mut usize| -> bool {
        loop {
            let Some(&length) = packet.get(*cursor) else {
                return false;
            };
            if length & 0xc0 == 0xc0 {
                *cursor += 2; // sıkıştırma işaretçisi
                return true;
            }
            *cursor += 1;
            if length == 0 {
                return true;
            }
            *cursor += length as usize;
        }
    };

    for _ in 0..question_count {
        if !skip_name(packet, &mut cursor) {
            return Vec::new();
        }
        cursor += 4; // tip + sınıf
    }

    let mut addresses = Vec::new();
    for _ in 0..header.answer_count {
        if !skip_name(packet, &mut cursor) {
            break;
        }
        if cursor + 10 > packet.len() {
            break;
        }
        let record_type = u16::from_be_bytes([packet[cursor], packet[cursor + 1]]);
        let data_length = u16::from_be_bytes([packet[cursor + 8], packet[cursor + 9]]) as usize;
        cursor += 10;
        if cursor + data_length > packet.len() {
            break;
        }
        if record_type == 1 && data_length == 4 {
            addresses.push(std::net::Ipv4Addr::new(
                packet[cursor],
                packet[cursor + 1],
                packet[cursor + 2],
                packet[cursor + 3],
            ));
        }
        cursor += data_length;
    }
    addresses
}

/// Düz metin HTTP isteği. 80 portundaki engelleme sayfalarını tespit etmek için
/// kullanılır; Türkiye'de yaygın olan yöntem 302 ile bir uyarı sayfasına
/// yönlendirmektir.
pub fn http_request(host: &str, path: &str) -> Vec<u8> {
    format!(
        "GET {path} HTTP/1.1\r\nHost: {host}\r\nUser-Agent: Mozilla/5.0\r\nAccept: */*\r\nConnection: close\r\n\r\n"
    )
    .into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_hello_is_a_well_formed_tls_record() {
        let record = client_hello("discord.com");
        assert_eq!(record[0], 0x16, "handshake kayıt tipi");
        assert_eq!(&record[1..3], &[0x03, 0x01], "kayıt sürümü");

        let record_length = u16::from_be_bytes([record[3], record[4]]) as usize;
        assert_eq!(
            record_length,
            record.len() - 5,
            "kayıt uzunluğu gövdeyle eşleşmeli"
        );

        assert_eq!(record[5], 0x01, "client_hello mesaj tipi");
        let message_length =
            ((record[6] as usize) << 16) | ((record[7] as usize) << 8) | record[8] as usize;
        assert_eq!(
            message_length,
            record.len() - 9,
            "handshake uzunluğu gövdeyle eşleşmeli"
        );
    }

    #[test]
    fn client_hello_carries_the_requested_server_name() {
        let record = client_hello("gateway.discord.gg");
        let needle = b"gateway.discord.gg";
        assert!(
            record.windows(needle.len()).any(|window| window == needle),
            "SNI paketin içinde bulunmalı"
        );
    }

    #[test]
    fn client_hello_randomises_every_call() {
        let first = client_hello("discord.com");
        let second = client_hello("discord.com");
        assert_ne!(
            first, second,
            "art arda ölçümler birbirinden ayırt edilebilmeli"
        );
    }

    #[test]
    fn server_hello_is_recognised() {
        // Kayıt başlığı + handshake tipi 0x02.
        let reply = [0x16, 0x03, 0x03, 0x00, 0x50, 0x02, 0x00, 0x00, 0x4c];
        assert_eq!(classify_tls_reply(&reply), TlsReply::ServerHello);
    }

    #[test]
    fn plaintext_http_on_443_is_a_block_page() {
        assert_eq!(
            classify_tls_reply(b"HTTP/1.1 302 Found\r\n"),
            TlsReply::BlockPage
        );
    }

    #[test]
    fn tls_alert_is_distinguished_from_a_block() {
        assert_eq!(
            classify_tls_reply(&[0x15, 0x03, 0x03, 0x00]),
            TlsReply::Alert
        );
    }

    #[test]
    fn dns_query_encodes_labels_and_asks_for_a_records() {
        let (transaction_id, packet) = dns_query("discord.com", 1);
        assert_eq!(
            u16::from_be_bytes([packet[0], packet[1]]),
            transaction_id,
            "işlem kimliği başlıkta olmalı"
        );
        assert_eq!(u16::from_be_bytes([packet[4], packet[5]]), 1, "tek soru");
        assert_eq!(&packet[12..20], b"\x07discord");
        assert_eq!(&packet[20..24], b"\x03com");
        assert_eq!(packet[24], 0, "kök etiketi");
        assert_eq!(u16::from_be_bytes([packet[25], packet[26]]), 1, "A kaydı");
    }

    #[test]
    fn a_records_are_extracted_from_a_response() {
        let (_, query) = dns_query("a.com", 1);
        let mut response = query.clone();
        response[2] = 0x81; // yanıt biti
        response[3] = 0x80;
        response[7] = 1; // tek yanıt
        response.extend_from_slice(&[0xc0, 0x0c]); // isim işaretçisi
        response.extend_from_slice(&[0x00, 0x01]); // tip A
        response.extend_from_slice(&[0x00, 0x01]); // sınıf IN
        response.extend_from_slice(&[0x00, 0x00, 0x01, 0x2c]); // TTL
        response.extend_from_slice(&[0x00, 0x04]); // veri uzunluğu
        response.extend_from_slice(&[93, 184, 216, 34]);

        let addresses = parse_dns_a_records(&response);
        assert_eq!(addresses, vec![std::net::Ipv4Addr::new(93, 184, 216, 34)]);
    }

    #[test]
    fn a_query_without_the_response_bit_yields_nothing() {
        let (_, query) = dns_query("a.com", 1);
        assert!(parse_dns_a_records(&query).is_empty());
    }
}
