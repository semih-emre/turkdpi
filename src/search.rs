//! Strateji araması: teşhisten çalışan yapılandırmaya kadar olan akış.
//!
//! Sıra şu:
//!   1. Ağı teşhis et ([`crate::probe`]).
//!   2. Bu ağ için daha önce öğrenilmiş bir strateji varsa onu ilk sıraya al.
//!   3. Teşhise göre aday stratejiler üret ([`crate::strategy`]).
//!   4. Her adayı sırayla uygula ve gerçek bağlantıyla doğrula.
//!   5. Tüm hedefleri geçen ilk adayda dur; yoksa en iyi kısmi sonucu kullan.
//!   6. Kazananı ağ hafızasına yaz.
//!
//! Erken durma önemli: her deneme motoru başlatıp birkaç TLS el sıkışması
//! yapmayı gerektiriyor, yani saniyeler sürüyor. Adayların teşhise göre
//! sıralanması, doğru olanın genellikle ilk birkaç denemede bulunmasını
//! sağlıyor.

use crate::engine::{self, Backend};
use crate::netid;
use crate::paths;
use crate::probe::{self, NetworkReport};
use crate::strategy::Strategy;
use crate::verify::{self, VerifyResult};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::time::Duration;

/// Motorun yakalamaya başlaması için beklenen süre. Süreç ayağa kalkmış olsa
/// bile sürücü/kuyruk bağlanması bir an alıyor; hemen ölçmek yanlış negatif
/// üretiyor.
const ENGINE_SETTLE: Duration = Duration::from_millis(600);

/// Varsayılan deneme üst sınırı. Aday listesi bundan uzun olabiliyor, ama
/// kullanıcıyı dakikalarca bekletmenin faydası yok: doğru aday teşhis
/// sayesinde baştaysa zaten erken bulunuyor.
pub const DEFAULT_ATTEMPT_LIMIT: usize = 24;

/// Tek bir denemenin sonucu.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Attempt {
    pub strategy: Strategy,
    pub passed: usize,
    pub total: usize,
}

/// Aramanın tamamı.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SearchOutcome {
    pub report: NetworkReport,
    pub backend: Option<Backend>,
    /// Seçilen strateji. `None` ise hiçbir aday hiçbir hedefi açamadı.
    pub winner: Option<Strategy>,
    /// Kazanan tüm hedefleri geçti mi, yoksa kısmi mi?
    pub complete: bool,
    pub attempts: Vec<Attempt>,
    pub network: Option<String>,
}

/// Arama ilerlemesini bildiren geri çağrı.
pub trait Progress {
    fn diagnosed(&mut self, report: &NetworkReport);
    fn trying(&mut self, index: usize, total: usize, strategy: &Strategy);
    fn tried(&mut self, result: &VerifyResult);
}

/// Hiçbir şey bildirmeyen ilerleme dinleyicisi.
pub struct Silent;

impl Progress for Silent {
    fn diagnosed(&mut self, _report: &NetworkReport) {}
    fn trying(&mut self, _index: usize, _total: usize, _strategy: &Strategy) {}
    fn tried(&mut self, _result: &VerifyResult) {}
}

/// Hafıza dosyalarının okunup yazıldığı yer. Dizin dışarıdan veriliyor;
/// böylece testler ortam değişkeni değiştirmek zorunda kalmıyor (testler
/// paralel çalıştığı için bu bir yarış kaynağı olurdu).
fn memory_path(directory: &Path, network: &str) -> std::path::PathBuf {
    directory.join(format!("{network}.json"))
}

fn remembered_in(directory: &Path, network: &str) -> Option<Strategy> {
    let raw = fs::read_to_string(memory_path(directory, network)).ok()?;
    serde_json::from_str(&raw).ok()
}

fn remember_in(directory: &Path, network: &str, strategy: &Strategy) -> Result<()> {
    fs::create_dir_all(directory)
        .with_context(|| format!("ağ hafızası yazılamadı: {}", directory.display()))?;
    let encoded = serde_json::to_string_pretty(strategy)?;
    fs::write(memory_path(directory, network), encoded)?;
    Ok(())
}

/// Bu ağ için daha önce öğrenilmiş stratejiyi okur.
pub fn remembered(network: &str) -> Option<Strategy> {
    remembered_in(&paths::networks_dir(), network)
}

/// Çalışan stratejiyi bu ağ için kaydeder.
pub fn remember(network: &str, strategy: &Strategy) -> Result<()> {
    remember_in(&paths::networks_dir(), network, strategy)
}

/// Aday listesini hazırlar: hafızadaki strateji varsa en öne alınır.
///
/// Saf bir fonksiyon: hafızayı kendisi okumuyor, çağıran veriyor. Bu sayede
/// sıralama mantığı diske dokunmadan test edilebiliyor.
fn ordered_candidates(report: &NetworkReport, previous: Option<Strategy>) -> Vec<Strategy> {
    let mut candidates = crate::strategy::candidates_for(report);
    let Some(previous) = previous else {
        return candidates;
    };
    // Hafızadaki strateji körü körüne uygulanmıyor; sadece sıranın başına
    // alınıyor. Doğrulamayı geçemezse arama normal şekilde devam ediyor.
    candidates.retain(|candidate| candidate.id() != previous.id());
    candidates.insert(0, previous);
    candidates
}

/// Tek bir stratejiyi uygulayıp doğrular.
///
/// Oturum fonksiyondan çıkarken düşüyor ve `Drop` motoru durduruyor; bu yüzden
/// hata durumunda bile arkada çalışan motor kalmıyor.
fn try_strategy(
    backend: Backend,
    strategy: &Strategy,
    hostlist: Option<&Path>,
    targets: &[&str],
) -> Result<VerifyResult> {
    let session = engine::start(backend, strategy, hostlist)?;
    std::thread::sleep(ENGINE_SETTLE);
    let result = verify::verify(session.channel, targets);
    drop(session);
    Ok(result)
}

/// Aramayı çalıştırır.
pub fn run(
    targets: &[&str],
    hostlist: Option<&Path>,
    limit: usize,
    progress: &mut impl Progress,
) -> Result<SearchOutcome> {
    let report = probe::diagnose_all(targets);
    progress.diagnosed(&report);
    let network = netid::fingerprint();

    // Engel yoksa motoru hiç başlatmıyoruz. Çalışan bir bağlantıya müdahale
    // etmek, iyileştirmek yerine bozma riski taşıyor.
    //
    // Motor seçimi bu kontrolden *sonra* yapılıyor: engelsiz bir ağda motorun
    // kurulu olmaması bir sorun değil, hata vermek kullanıcıyı boşuna
    // telaşlandırır.
    if !report.dominant.is_blocked() {
        return Ok(SearchOutcome {
            report,
            backend: None,
            winner: None,
            complete: true,
            attempts: Vec::new(),
            network,
        });
    }

    let backend = engine::select()?;

    let previous = network.as_deref().and_then(remembered);
    let candidates = ordered_candidates(&report, previous);
    let total = candidates.len().min(limit);

    let mut attempts = Vec::new();
    let mut best: Option<(Strategy, usize)> = None;

    for (index, strategy) in candidates.into_iter().take(limit).enumerate() {
        progress.trying(index + 1, total, &strategy);
        // Tek bir adayın başlatılamaması aramayı bitirmemeli: motor bu
        // argüman birleşimini desteklemiyor olabilir, sıradakine geçilir.
        let Ok(result) = try_strategy(backend, &strategy, hostlist, targets) else {
            continue;
        };
        progress.tried(&result);
        attempts.push(Attempt {
            strategy: strategy.clone(),
            passed: result.passed(),
            total: result.total(),
        });

        if result.is_complete() {
            if let Some(name) = &network {
                let _ = remember(name, &strategy);
            }
            return Ok(SearchOutcome {
                report,
                backend: Some(backend),
                winner: Some(strategy),
                complete: true,
                attempts,
                network,
            });
        }
        // Kısmi başarı da saklanıyor: hiçbir aday tam geçemezse en çok hedefi
        // açan kullanılıyor. Discord'un sohbeti açılıp sesi açılmaması, hiç
        // açılmamasından iyi.
        let score = result.score();
        if score > 0 && best.as_ref().is_none_or(|(_, top)| score > *top) {
            best = Some((strategy, score));
        }
    }

    let winner = best.map(|(strategy, _)| strategy);
    if let (Some(name), Some(strategy)) = (&network, &winner) {
        let _ = remember(name, strategy);
    }
    Ok(SearchOutcome {
        report,
        backend: Some(backend),
        winner,
        complete: false,
        attempts,
        network,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::probe::Verdict;
    use crate::strategy::{Desync, Fooling, SplitAt};

    fn report(dominant: Verdict, dpi_hop: Option<u8>) -> NetworkReport {
        NetworkReport {
            targets: Vec::new(),
            dominant,
            dpi_hop,
        }
    }

    fn strategy(ttl: u8) -> Strategy {
        Strategy {
            desync: Desync::Fake,
            ttl: Some(ttl),
            fooling: vec![Fooling::BadSum],
            split: Some(SplitAt::SniMiddle),
            repeats: 1,
            tlsrec: None,
        }
    }

    /// Her test kendi dizinini kullanıyor; paralel çalışmada çakışma olmuyor.
    fn scratch(name: &str) -> std::path::PathBuf {
        let directory = std::env::temp_dir().join(format!("turkdpi-test-{name}"));
        let _ = fs::remove_dir_all(&directory);
        directory
    }

    #[test]
    fn a_remembered_strategy_is_tried_first() {
        let known = strategy(9);
        let ordered = ordered_candidates(
            &report(Verdict::SniReset { dpi_hop: Some(3) }, Some(3)),
            Some(known.clone()),
        );
        assert_eq!(
            ordered[0].id(),
            known.id(),
            "hafızadaki strateji başa gelmeli"
        );
    }

    #[test]
    fn a_remembered_strategy_is_not_duplicated_in_the_list() {
        // Aday üretiminin zaten ürettiği bir stratejiyi hatırlıyoruz.
        let candidates = crate::strategy::candidates_for(&report(Verdict::SniDrop, None));
        let duplicate = candidates[3].clone();

        let ordered = ordered_candidates(&report(Verdict::SniDrop, None), Some(duplicate.clone()));
        let occurrences = ordered
            .iter()
            .filter(|candidate| candidate.id() == duplicate.id())
            .count();
        assert_eq!(occurrences, 1, "strateji listede bir kez bulunmalı");
        assert_eq!(ordered[0].id(), duplicate.id());
        assert_eq!(
            ordered.len(),
            candidates.len(),
            "liste uzunluğu değişmemeli"
        );
    }

    #[test]
    fn memory_round_trips_through_disk() {
        let directory = scratch("memory-roundtrip");
        let saved = strategy(7);
        remember_in(&directory, "net-roundtrip", &saved).expect("yazılmalı");
        let loaded = remembered_in(&directory, "net-roundtrip").expect("okunmalı");
        assert_eq!(loaded, saved);
        let _ = fs::remove_dir_all(&directory);
    }

    #[test]
    fn an_unknown_network_has_no_memory() {
        let directory = scratch("memory-unknown");
        assert!(remembered_in(&directory, "net-never-seen").is_none());
    }

    #[test]
    fn a_corrupt_memory_file_is_ignored_rather_than_fatal() {
        // Disk dolduğunda ya da güncelleme sırasında yarım yazılmış bir dosya
        // kalabilir; bunun aramayı çökertmemesi gerekiyor.
        let directory = scratch("memory-corrupt");
        fs::create_dir_all(&directory).expect("dizin");
        fs::write(memory_path(&directory, "net-bad"), "{ bozuk").expect("yaz");
        assert!(remembered_in(&directory, "net-bad").is_none());
        let _ = fs::remove_dir_all(&directory);
    }

    #[test]
    fn without_a_network_fingerprint_the_plain_candidate_list_is_used() {
        let plain = crate::strategy::candidates_for(&report(Verdict::SniDrop, None));
        let ordered = ordered_candidates(&report(Verdict::SniDrop, None), None);
        assert_eq!(ordered.len(), plain.len());
        assert_eq!(ordered[0].id(), plain[0].id());
    }

    #[test]
    fn writing_memory_creates_the_directory_if_it_is_missing() {
        // İlk çalıştırmada ağ hafızası dizini henüz yok.
        let directory = scratch("memory-mkdir").join("nested");
        remember_in(&directory, "net-new", &strategy(4)).expect("dizin oluşturulmalı");
        assert!(remembered_in(&directory, "net-new").is_some());
        let _ = fs::remove_dir_all(directory.parent().expect("üst dizin"));
    }
}
