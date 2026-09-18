//! Yükseltilmiş yetki kontrolü.
//!
//! Şeffaf motorlar (nfqws, winws, tpws) çekirdek seviyesinde paket yakaladığı
//! için Linux'ta root, Windows'ta yönetici yetkisi ister. Proxy motoru
//! (ByeDPI) istemez. Bu ayrım önemli: yetki yoksa kullanıcıyı geri çevirmek
//! yerine proxy moduna düşebiliyoruz.

/// Süreç yükseltilmiş yetkiyle mi çalışıyor?
pub fn is_elevated() -> bool {
    platform::is_elevated()
}

/// Kullanıcıya bu platformda yetkinin nasıl alınacağını anlatan mesaj.
pub fn elevation_hint() -> &'static str {
    if cfg!(target_os = "windows") {
        "bu işlem yönetici yetkisi gerektirir (uygulamayı yönetici olarak çalıştırın)"
    } else if cfg!(target_os = "macos") {
        "bu işlem root yetkisi gerektirir (sudo ile çalıştırın)"
    } else {
        "bu işlem root yetkisi gerektirir (Polkit/pkexec kullanın)"
    }
}

#[cfg(unix)]
mod platform {
    pub fn is_elevated() -> bool {
        // SAFETY: geteuid parametre almaz, her zaman başarılı olur ve yan
        // etkisi yoktur.
        unsafe { libc::geteuid() == 0 }
    }
}

#[cfg(windows)]
mod platform {
    use std::ffi::c_void;

    type Handle = isize;
    type Bool = i32;

    const TOKEN_QUERY: u32 = 0x0008;
    /// `TOKEN_INFORMATION_CLASS::TokenElevation`
    const TOKEN_ELEVATION: i32 = 20;

    // windows-sys yerine elle bağlanıyoruz. Tek ihtiyacımız bu üç çağrı ve
    // bir bağımlılık eklemek, projeyi import kütüphanesi üretebilen bir
    // toolchain'e mahkûm ediyordu.
    #[link(name = "advapi32")]
    extern "system" {
        fn OpenProcessToken(process: Handle, desired_access: u32, token: *mut Handle) -> Bool;
        fn GetTokenInformation(
            token: Handle,
            information_class: i32,
            information: *mut c_void,
            information_length: u32,
            return_length: *mut u32,
        ) -> Bool;
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentProcess() -> Handle;
        fn CloseHandle(object: Handle) -> Bool;
    }

    pub fn is_elevated() -> bool {
        let mut token: Handle = 0;
        // SAFETY: GetCurrentProcess sözde tanıtıcı döndürür (kapatılması
        // gerekmez). OpenProcessToken'a geçerli bir yazılabilir işaretçi
        // veriyoruz ve dönüş değerini kontrol ediyoruz.
        unsafe {
            if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
                return false;
            }
            let mut elevation: u32 = 0;
            let mut returned: u32 = 0;
            let ok = GetTokenInformation(
                token,
                TOKEN_ELEVATION,
                &mut elevation as *mut u32 as *mut c_void,
                std::mem::size_of::<u32>() as u32,
                &mut returned,
            );
            CloseHandle(token);
            ok != 0 && elevation != 0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_check_returns_without_crashing() {
        // Sonuç ortama bağlı; burada önemli olan FFI'ın güvenli şekilde
        // tamamlanması ve tanıtıcının sızmaması.
        let _ = is_elevated();
    }

    #[test]
    fn the_hint_names_the_right_mechanism_for_this_platform() {
        let hint = elevation_hint();
        if cfg!(target_os = "windows") {
            assert!(hint.contains("yönetici"));
        } else {
            assert!(hint.contains("root"));
        }
    }
}
