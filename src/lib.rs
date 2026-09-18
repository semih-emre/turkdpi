//! TurkDPI çekirdeği.
//!
//! Katmanlar:
//!   * [`wire`] — ham TLS/DNS/HTTP paketleri. Hiçbir kriptografi kütüphanesine
//!     bağlı değil; amacı oturum kurmak değil, DPI tepkisini ölçmek.
//!   * [`probe`] — engellemenin *nasıl* yapıldığını teşhis eder ve DPI'ın kaç
//!     hop uzakta olduğunu ölçer.
//!   * [`strategy`] — teşhise göre aday kaçınma stratejileri üretir ve bunları
//!     zapret ya da ByeDPI argümanlarına çevirir.
//!   * [`paths`] — platforma göre değişen dizinler.

/// Sistem DNS'ini değiştirir. NetworkManager'a bağlı olduğu için şimdilik
/// yalnızca Linux'ta var; Windows ve macOS karşılıkları henüz yazılmadı.
#[cfg(target_os = "linux")]
pub mod dnscfg;
pub mod engine;
pub mod fetch;
pub mod log;
pub mod netid;
pub mod paths;
pub mod privilege;
pub mod probe;
pub mod search;
pub mod sha256;
pub mod socks;
pub mod status;
pub mod strategy;
pub mod util;
pub mod verify;
pub mod wire;
