//! Küçük yardımcılar.

use std::time::{SystemTime, UNIX_EPOCH};

/// Bugünü `20260918` biçiminde döndürür.
///
/// Günlük dosyalarının adlandırılmasında kullanılıyor: dosya adı hem okunabilir
/// hem de ada göre sıralandığında tarih sırasına giriyor.
pub fn utc_date() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or(0);
    let (year, month, day) = civil_from_days(seconds.div_euclid(86_400));
    format!("{year:04}{month:02}{day:02}")
}

/// Şu anı `20260918T101530Z` biçiminde döndürür.
///
/// Yedek dosyalarının adlandırılmasında kullanılıyor. Bunun için tam bir takvim
/// kütüphanesi çekmek yerine dönüşüm burada yapılıyor: kullanıcı bir yedeği
/// geri yüklemek istediğinde dosya adının okunabilir olması gerekiyor, ama bu
/// tek ihtiyaç için bir bağımlılık eklemeye değmez.
pub fn utc_timestamp() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or(0);
    let (year, month, day) = civil_from_days(seconds.div_euclid(86_400));
    let time_of_day = seconds.rem_euclid(86_400);
    let (hour, minute, second) = (
        time_of_day / 3600,
        (time_of_day % 3600) / 60,
        time_of_day % 60,
    );
    format!("{year:04}{month:02}{day:02}T{hour:02}{minute:02}{second:02}Z")
}

/// 1970-01-01'den itibaren geçen gün sayısını takvim tarihine çevirir.
///
/// Howard Hinnant'ın `civil_from_days` algoritması: 400 yıllık Gregoryen
/// döngüsünü ("era") temel alarak artık yılları dallanma olmadan çözer.
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    // Dönemin başlangıcını 0000-03-01'e kaydır; böylece artık gün yılın sonuna
    // düşer ve ay uzunlukları düzenli bir örüntü izler.
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097); // [0, 146096]
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365; // [0, 399]
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_position = (5 * day_of_year + 2) / 153; // [0, 11], Mart = 0
    let day = day_of_year - (153 * month_position + 2) / 5 + 1;
    let month = if month_position < 10 {
        month_position + 3
    } else {
        month_position - 9
    };
    // Ocak ve Şubat, kaydırılmış takvimde bir sonraki yıla aitti.
    (year + i64::from(month <= 2), month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_epoch_converts_to_its_calendar_date() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
    }

    #[test]
    fn leap_days_are_handled() {
        // 2000 bir artık yıldı (400'e bölünüyor), 1900 değildi.
        assert_eq!(civil_from_days(10_956), (1999, 12, 31));
        assert_eq!(civil_from_days(10_957), (2000, 1, 1));
        assert_eq!(civil_from_days(11_016), (2000, 2, 29), "artık gün");
        assert_eq!(civil_from_days(11_017), (2000, 3, 1));
    }

    #[test]
    fn a_known_date_round_trips() {
        // 2026-09-18
        let days = 20_714;
        assert_eq!(civil_from_days(days), (2026, 9, 18));
    }

    #[test]
    fn dates_before_the_epoch_work() {
        assert_eq!(civil_from_days(-1), (1969, 12, 31));
    }

    #[test]
    fn the_timestamp_has_a_fixed_width_and_shape() {
        let timestamp = utc_timestamp();
        assert_eq!(timestamp.len(), 16, "20260918T101530Z biçimi");
        assert!(timestamp.ends_with('Z'));
        assert_eq!(timestamp.as_bytes()[8], b'T');
        assert!(
            timestamp
                .trim_end_matches('Z')
                .split('T')
                .all(|part| part.chars().all(|c| c.is_ascii_digit())),
            "harf içermemeli: {timestamp}"
        );
    }

    #[test]
    fn timestamps_sort_chronologically_as_plain_strings() {
        // Yedek dosyaları ada göre sıralandığında zaman sırasına girmeli.
        let (year, month, day) = civil_from_days(20_714);
        let earlier = format!("{year:04}{month:02}{day:02}T000000Z");
        let later = format!("{year:04}{month:02}{day:02}T235959Z");
        assert!(earlier < later);
    }
}
