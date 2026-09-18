//! Strateji uzayı: hangi kaçınma tekniğinin deneneceğini üretir ve sıralar.
//!
//! Eski tasarımda altı sabit profil vardı ve `auto` bunlardan dördünü sırayla
//! deniyordu. Buradaki yaklaşım farklı: teşhis raporuna bakıp aday stratejileri
//! *üretiyoruz* ve en olası olandan başlayarak sıralıyoruz.
//!
//! Aynı strateji iki farklı motor ailesine çevrilebiliyor:
//!   * zapret (`nfqws` Linux, `winws.exe` Windows, `tpws` macOS)
//!   * ByeDPI (`ciadpi`, üç platformda da yerel SOCKS5 proxy)
//!
//! Böylece ağ için bir kez öğrenilen çözüm, kullanıcının makinesinde hangi
//! motor varsa ona uygulanabiliyor.

use crate::probe::{NetworkReport, Verdict};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Paketin nasıl bozulacağı.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Desync {
    /// DPI'a sahte bir paket gösterip gerçek paketi arkasından yollamak.
    /// Enjeksiyon tabanlı engellemelerde (RST) en etkili yöntem.
    Fake,
    /// İsteği birden çok TCP segmentine bölmek. SNI iki segmente yayıldığında
    /// durum tutmayan DPI'lar eşleştiremiyor.
    MultiSplit,
    /// Bölmeye ek olarak parçaları ters sırada göndermek.
    MultiDisorder,
    /// Sahte paket + bölme birlikte.
    FakedSplit,
}

impl Desync {
    fn zapret_value(self) -> &'static str {
        match self {
            Desync::Fake => "fake",
            Desync::MultiSplit => "multisplit",
            Desync::MultiDisorder => "multidisorder",
            Desync::FakedSplit => "fakedsplit",
        }
    }

    fn short(self) -> &'static str {
        match self {
            Desync::Fake => "fake",
            Desync::MultiSplit => "split",
            Desync::MultiDisorder => "disorder",
            Desync::FakedSplit => "fakesplit",
        }
    }
}

/// Sahte paketin gerçek sunucu tarafından yok sayılmasını sağlayan hile.
/// DPI paketi geçerli sayar, sunucu çöpe atar.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Fooling {
    /// Bozuk sağlama toplamı. En yaygın ve en taşınabilir yöntem.
    BadSum,
    /// Geçersiz sıra numarası.
    BadSeq,
    /// TCP MD5 imzası. Çoğu Linux sunucusu böyle paketleri atar.
    /// Yalnızca Linux'ta üretilebiliyor.
    Md5Sig,
}

impl Fooling {
    fn zapret_value(self) -> &'static str {
        match self {
            Fooling::BadSum => "badsum",
            Fooling::BadSeq => "badseq",
            Fooling::Md5Sig => "md5sig",
        }
    }

    /// Bu hile bu platformda üretilebiliyor mu?
    ///
    /// TCP MD5 imzası yalnızca Linux çekirdeğinde üretilebiliyor; diğer
    /// platformlarda argüman motora verilse bile etkisiz kalırdı.
    fn supported_here(self) -> bool {
        !matches!(self, Fooling::Md5Sig) || cfg!(target_os = "linux")
    }
}

/// İsteğin nereden bölüneceği. İki motor bunu farklı yazdığı için soyut
/// tutuluyor.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SplitAt {
    /// Sabit bayt konumu. Küçük değerler (1, 2, 3) TLS kayıt başlığını böler.
    Byte(u16),
    /// SNI uzantısının başlangıcı.
    SniStart,
    /// Alan adının ortası. Türkiye'deki DPI'lara karşı en etkili konum.
    SniMiddle,
    /// HTTP `Host` başlığının başlangıcı.
    HostStart,
}

impl SplitAt {
    fn zapret_value(self) -> String {
        match self {
            SplitAt::Byte(offset) => offset.to_string(),
            SplitAt::SniStart => "sniext+1".into(),
            SplitAt::SniMiddle => "midsld".into(),
            SplitAt::HostStart => "host+1".into(),
        }
    }

    fn byedpi_value(self) -> String {
        match self {
            SplitAt::Byte(offset) => offset.to_string(),
            SplitAt::SniStart => "0+s".into(),
            SplitAt::SniMiddle => "0+sm".into(),
            SplitAt::HostStart => "0+h".into(),
        }
    }

    fn short(self) -> String {
        match self {
            SplitAt::Byte(offset) => format!("b{offset}"),
            SplitAt::SniStart => "sni".into(),
            SplitAt::SniMiddle => "midsld".into(),
            SplitAt::HostStart => "host".into(),
        }
    }
}

/// Denenebilir tek bir kaçınma yapılandırması.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Strategy {
    pub desync: Desync,
    /// Sahte paketin TTL'i. Ölçülen DPI mesafesine eşit olmalı: paket DPI'a
    /// ulaşmalı ama gerçek sunucuya varmamalı.
    pub ttl: Option<u8>,
    pub fooling: Vec<Fooling>,
    pub split: Option<SplitAt>,
    pub repeats: u8,
    /// ClientHello'yu ayrı TLS kayıtlarına bölme konumu.
    pub tlsrec: Option<SplitAt>,
}

impl Strategy {
    /// Kalıcı olarak saklanabilen, insan tarafından okunabilir kimlik.
    /// Ağ hafızasında bu kimlik tutuluyor.
    pub fn id(&self) -> String {
        let mut parts = vec![self.desync.short().to_owned()];
        if let Some(ttl) = self.ttl {
            parts.push(format!("ttl{ttl}"));
        }
        for fooling in &self.fooling {
            parts.push(fooling.zapret_value().to_owned());
        }
        if let Some(split) = self.split {
            parts.push(split.short());
        }
        if let Some(tlsrec) = self.tlsrec {
            parts.push(format!("rec-{}", tlsrec.short()));
        }
        if self.repeats > 1 {
            parts.push(format!("x{}", self.repeats));
        }
        parts.join("-")
    }

    /// zapret ailesi (`nfqws`, `winws`, `tpws`) için TCP argümanları.
    pub fn zapret_args(&self) -> Vec<String> {
        let mut args = vec![
            "--filter-tcp=80,443".to_owned(),
            "--filter-l7=http,tls".to_owned(),
            format!("--dpi-desync={}", self.desync.zapret_value()),
        ];
        if let Some(ttl) = self.ttl {
            args.push(format!("--dpi-desync-ttl={ttl}"));
        }
        let fooling: Vec<&str> = self
            .fooling
            .iter()
            .copied()
            .filter(|f| f.supported_here())
            .map(Fooling::zapret_value)
            .collect();
        if !fooling.is_empty() {
            args.push(format!("--dpi-desync-fooling={}", fooling.join(",")));
        }
        if let Some(split) = self.split {
            args.push(format!("--dpi-desync-split-pos={}", split.zapret_value()));
        }
        if self.repeats > 1 {
            args.push(format!("--dpi-desync-repeats={}", self.repeats));
        }
        args
    }

    /// zapret ailesi için QUIC (UDP 443) argümanları.
    ///
    /// Yalnızca TCP'yi düzeltmek çoğu zaman yetmiyor: Discord, YouTube ve
    /// Cloudflare arkasındaki pek çok servis QUIC üzerinden konuşuyor. TCP
    /// açılıp QUIC engelli kalırsa tarayıcı sessizce yavaşlıyor, Discord sesi
    /// hiç bağlanmıyor.
    ///
    /// QUIC'te bölme yöntemleri geçersiz (UDP'de segment yok), bu yüzden
    /// strateji ne olursa olsun sahte paket yöntemi kullanılıyor; stratejiden
    /// devralınan şey TTL ve hile seçimi.
    pub fn zapret_quic_args(&self) -> Vec<String> {
        let mut args = vec![
            "--filter-udp=443".to_owned(),
            "--filter-l7=quic".to_owned(),
            "--dpi-desync=fake".to_owned(),
        ];
        if let Some(ttl) = self.ttl {
            args.push(format!("--dpi-desync-ttl={ttl}"));
        }
        let fooling: Vec<&str> = self
            .fooling
            .iter()
            .copied()
            .filter(|f| f.supported_here())
            .map(Fooling::zapret_value)
            .collect();
        // QUIC'te badsum varsayılan olarak işe yarıyor; strateji hile
        // belirtmediyse buna düşüyoruz.
        args.push(format!(
            "--dpi-desync-fooling={}",
            if fooling.is_empty() {
                "badsum".to_owned()
            } else {
                fooling.join(",")
            }
        ));
        args.push(format!("--dpi-desync-repeats={}", self.repeats.max(2)));
        // İlk birkaç paketten sonra müdahaleyi kesmek, kurulmuş oturumların
        // bozulmasını önlüyor.
        args.push("--dpi-desync-cutoff=n6".to_owned());
        args
    }

    /// ByeDPI (`ciadpi`) için argümanlar.
    ///
    /// ByeDPI'da bölme yöntemi ayrı bir bayrakla seçiliyor: `--split`,
    /// `--disorder` ya da `--fake`. Sahte paket tabanlı stratejilerde `--fake`
    /// kullanıp TTL'i ona veriyoruz.
    pub fn byedpi_args(&self) -> Vec<String> {
        let position = self.split.unwrap_or(SplitAt::SniMiddle).byedpi_value();
        let mut args = match self.desync {
            Desync::Fake | Desync::FakedSplit => vec!["--fake".to_owned(), position],
            Desync::MultiDisorder => vec!["--disorder".to_owned(), position],
            Desync::MultiSplit => vec!["--split".to_owned(), position],
        };
        // FakedSplit hem sahte paket hem de ayrı bir bölme istiyor.
        if self.desync == Desync::FakedSplit {
            args.push("--split".to_owned());
            args.push(SplitAt::SniMiddle.byedpi_value());
        }
        if let Some(ttl) = self.ttl {
            args.push("--ttl".to_owned());
            args.push(ttl.to_string());
        }
        // md5sig yalnızca Linux'ta var; ByeDPI'da ayrı bir bayrak.
        if self.fooling.contains(&Fooling::Md5Sig) && cfg!(target_os = "linux") {
            args.push("--md5sig".to_owned());
        }
        if let Some(tlsrec) = self.tlsrec {
            args.push("--tlsrec".to_owned());
            args.push(tlsrec.byedpi_value());
        }
        args
    }
}

impl fmt::Display for Strategy {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.id())
    }
}

/// Sahte paket tabanlı stratejiler. RST enjeksiyonuna karşı kullanılır.
fn fake_candidates(ttl_hints: &[u8]) -> Vec<Strategy> {
    let mut candidates = Vec::new();
    for &ttl in ttl_hints {
        for fooling in [vec![Fooling::BadSum], vec![Fooling::BadSeq], Vec::new()] {
            candidates.push(Strategy {
                desync: Desync::Fake,
                ttl: Some(ttl),
                fooling: fooling.clone(),
                split: None,
                repeats: 1,
                tlsrec: None,
            });
            candidates.push(Strategy {
                desync: Desync::FakedSplit,
                ttl: Some(ttl),
                fooling,
                split: Some(SplitAt::SniMiddle),
                repeats: 1,
                tlsrec: None,
            });
        }
    }
    candidates
}

/// Bölme tabanlı stratejiler. Paket düşürmeye karşı kullanılır; sahte paket
/// göndermek düşürmeyi engellemez, SNI'yi parçalamak engeller.
fn split_candidates() -> Vec<Strategy> {
    let mut candidates = Vec::new();
    for desync in [Desync::MultiSplit, Desync::MultiDisorder] {
        for split in [
            SplitAt::SniMiddle,
            SplitAt::Byte(1),
            SplitAt::SniStart,
            SplitAt::Byte(2),
            SplitAt::HostStart,
        ] {
            candidates.push(Strategy {
                desync,
                ttl: None,
                fooling: Vec::new(),
                split: Some(split),
                repeats: 1,
                tlsrec: None,
            });
        }
    }
    // ClientHello'yu ayrı TLS kayıtlarına bölmek, segment birleştiren
    // DPI'ları da atlatıyor; bu yüzden ayrı bir aday olarak ekleniyor.
    candidates.push(Strategy {
        desync: Desync::MultiSplit,
        ttl: None,
        fooling: Vec::new(),
        split: Some(SplitAt::SniMiddle),
        repeats: 1,
        tlsrec: Some(SplitAt::Byte(1)),
    });
    candidates
}

/// Ölçülen DPI mesafesinden denenecek TTL değerlerini çıkarır.
///
/// Ölçüm tam isabet etse bile yol üzerindeki asimetri nedeniyle komşu
/// değerler de işe yarayabiliyor, bu yüzden N'den sonra N+1 ve N-1 deneniyor.
/// Ölçüm hiç yapılamadıysa pratikte en sık işe yarayan aralığa düşülüyor.
fn ttl_hints(dpi_hop: Option<u8>) -> Vec<u8> {
    match dpi_hop {
        Some(hop) => {
            let mut hints = vec![hop];
            if hop < u8::MAX {
                hints.push(hop + 1);
            }
            if hop > 1 {
                hints.push(hop - 1);
            }
            hints
        }
        None => vec![4, 3, 5, 2, 6, 8],
    }
}

/// Teşhis raporuna göre denenecek stratejileri en olasıdan başlayarak üretir.
///
/// Sıralama önemli: her aday gerçek bir bağlantı testi gerektiriyor ve bu
/// saniyeler sürüyor. Doğru adayı başa koymak, kullanıcının bekleme süresini
/// dakikalardan saniyelere indiriyor.
pub fn candidates_for(report: &NetworkReport) -> Vec<Strategy> {
    let hints = ttl_hints(report.dpi_hop);
    let mut ordered = match report.dominant {
        // Enjeksiyon tabanlı engelleme: önce sahte paket, sonra bölme.
        Verdict::SniReset { .. } | Verdict::BlockPage => {
            let mut list = fake_candidates(&hints);
            list.extend(split_candidates());
            list
        }
        // Paket düşürülüyorsa sahte paket göndermek işe yaramaz; asıl çözüm
        // SNI'yi segmentlere yaymak.
        Verdict::SniDrop => {
            let mut list = split_candidates();
            list.extend(fake_candidates(&hints));
            list
        }
        // DNS zehirlemesinde asıl çözüm DNS katmanında; yine de adresler
        // düzeldikten sonra TLS katmanında engelleme kalabiliyor.
        Verdict::DnsPoisoned { .. } => {
            let mut list = split_candidates();
            list.extend(fake_candidates(&hints));
            list
        }
        // IP seviyesinde engellemede paket kurcalamanın faydası yok, ama
        // hedeflerden bir kısmı SNI ile engelleniyor olabilir; yine de deneriz.
        Verdict::IpBlocked | Verdict::Open | Verdict::Unreachable => {
            let mut list = fake_candidates(&hints);
            list.extend(split_candidates());
            list
        }
    };

    // Aynı strateji birden çok kez üretilmiş olabilir; kimliğe göre tekilleştir.
    let mut seen = std::collections::HashSet::new();
    ordered.retain(|strategy| seen.insert(strategy.id()));
    ordered
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::probe::TargetReport;

    fn report(dominant: Verdict, dpi_hop: Option<u8>) -> NetworkReport {
        NetworkReport {
            targets: Vec::<TargetReport>::new(),
            dominant,
            dpi_hop,
        }
    }

    #[test]
    fn measured_hop_is_tried_before_its_neighbours() {
        let candidates = candidates_for(&report(Verdict::SniReset { dpi_hop: Some(5) }, Some(5)));
        let first_ttl = candidates
            .iter()
            .find_map(|strategy| strategy.ttl)
            .expect("ttl taşıyan aday");
        assert_eq!(first_ttl, 5, "ölçülen değer önce denenmeli");
    }

    #[test]
    fn a_drop_prefers_splitting_over_fake_packets() {
        let candidates = candidates_for(&report(Verdict::SniDrop, None));
        assert!(
            matches!(
                candidates[0].desync,
                Desync::MultiSplit | Desync::MultiDisorder
            ),
            "düşürmede bölme önce gelmeli, bulunan: {:?}",
            candidates[0].desync
        );
    }

    #[test]
    fn a_reset_prefers_fake_packets_over_splitting() {
        let candidates = candidates_for(&report(Verdict::SniReset { dpi_hop: Some(4) }, Some(4)));
        assert_eq!(candidates[0].desync, Desync::Fake);
    }

    #[test]
    fn candidates_are_unique() {
        let candidates = candidates_for(&report(Verdict::SniReset { dpi_hop: Some(3) }, Some(3)));
        let mut identifiers: Vec<String> = candidates.iter().map(Strategy::id).collect();
        let total = identifiers.len();
        identifiers.sort();
        identifiers.dedup();
        assert_eq!(identifiers.len(), total, "aday listesinde tekrar olmamalı");
    }

    #[test]
    fn without_a_measurement_the_common_range_is_covered() {
        let candidates = candidates_for(&report(Verdict::SniReset { dpi_hop: None }, None));
        let tried: Vec<u8> = candidates.iter().filter_map(|s| s.ttl).collect();
        for expected in [2, 3, 4, 5, 6, 8] {
            assert!(tried.contains(&expected), "TTL {expected} denenmeli");
        }
    }

    #[test]
    fn zapret_rendering_carries_ttl_and_fooling() {
        let strategy = Strategy {
            desync: Desync::Fake,
            ttl: Some(6),
            fooling: vec![Fooling::BadSum],
            split: None,
            repeats: 4,
            tlsrec: None,
        };
        let args = strategy.zapret_args();
        assert!(args.contains(&"--dpi-desync=fake".to_owned()));
        assert!(args.contains(&"--dpi-desync-ttl=6".to_owned()));
        assert!(args.contains(&"--dpi-desync-fooling=badsum".to_owned()));
        assert!(args.contains(&"--dpi-desync-repeats=4".to_owned()));
    }

    #[test]
    fn byedpi_rendering_uses_its_own_flag_names() {
        let strategy = Strategy {
            desync: Desync::MultiDisorder,
            ttl: Some(8),
            fooling: Vec::new(),
            split: Some(SplitAt::SniMiddle),
            repeats: 1,
            tlsrec: None,
        };
        let args = strategy.byedpi_args();
        assert_eq!(args[0], "--disorder");
        assert_eq!(args[1], "0+sm", "SNI ortası ByeDPI söz diziminde");
        assert!(args.contains(&"--ttl".to_owned()));
        assert!(args.contains(&"8".to_owned()));
    }

    #[test]
    fn md5sig_is_dropped_on_platforms_that_cannot_produce_it() {
        let strategy = Strategy {
            desync: Desync::Fake,
            ttl: Some(4),
            fooling: vec![Fooling::Md5Sig],
            split: None,
            repeats: 1,
            tlsrec: None,
        };
        let rendered = strategy.zapret_args().join(" ");
        if cfg!(target_os = "linux") {
            assert!(rendered.contains("md5sig"));
        } else {
            assert!(
                !rendered.contains("md5sig"),
                "md5sig yalnızca Linux'ta üretilebiliyor"
            );
        }
    }

    #[test]
    fn identifiers_are_stable_and_descriptive() {
        let strategy = Strategy {
            desync: Desync::Fake,
            ttl: Some(5),
            fooling: vec![Fooling::BadSum],
            split: None,
            repeats: 1,
            tlsrec: None,
        };
        assert_eq!(strategy.id(), "fake-ttl5-badsum");
    }

    #[test]
    fn a_strategy_survives_a_json_round_trip() {
        // Ağ hafızası stratejiyi diske JSON olarak yazıyor.
        let strategy = Strategy {
            desync: Desync::FakedSplit,
            ttl: Some(3),
            fooling: vec![Fooling::BadSum, Fooling::BadSeq],
            split: Some(SplitAt::SniMiddle),
            repeats: 2,
            tlsrec: Some(SplitAt::Byte(1)),
        };
        let encoded = serde_json::to_string(&strategy).expect("serialise");
        let decoded: Strategy = serde_json::from_str(&encoded).expect("deserialise");
        assert_eq!(decoded, strategy);
    }
}
