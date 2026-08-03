# TurkDPI 0.2.0

TurkDPI, CachyOS x86_64 ve KDE Plasma için Discord, Roblox ve genel web erişimine odaklanan bir `nfqws` yöneticisidir. VPN değildir; trafiği uzak bir sunucuya taşımaz, TLS çözmez, telemetri toplamaz ve kullanıcı trafiğini kaydetmez.

> Bu yazılım ağ paketlerinin aktarım biçimini ve seçildiğinde NetworkManager DNS ayarını değiştirir. Yerel mevzuata ve kullandığınız hizmetlerin koşullarına uygun kullanmak sizin sorumluluğunuzdadır.

## 0.2.0 yenilikleri

- Discord Voice IP Discovery ve STUN paketleri için özel RTC profili.
- Discord Voice Ready mesajının verdiği değişken hedef portları kaçırmamak için DNS/DHCP/NTP dışındaki UDP çıkışlarının NFQUEUE tarafından görülmesi.
- Roblox web/CDN alan adları ve dinamik `49152–65535/UDP` oyun oturumları için ayrı profil.
- Belirli alan listesine bağlı olmayan genel HTTP/TLS/QUIC profili.
- Etkin NetworkManager bağlantısına otomatik Cloudflare DNS şablonu: `1.1.1.1`, `1.0.0.1` ve IPv6 karşılıkları.
- DNS değişikliğinden önce bağlantı UUID’sine göre yedekleme ve durdurma/temizlemede otomatik geri yükleme.

## Profiller

- **Discord:** Discord web, Gateway, QUIC, STUN ve Voice IP Discovery/RTC.
- **Roblox:** Roblox web/CDN ile dinamik oyun UDP portları. İlk bilinmeyen UDP paketlerine müdahale ettiği için yalnız Roblox gerektiğinde seçilmelidir.
- **Genel:** Alan adı listesi kullanmadan tüm HTTP/TLS ve QUIC web trafiği. Bilinmeyen oyun UDP’sine dokunmaz.
- **Güvenli:** Discord ve Roblox alan listesinde düşük müdahaleli TLS/QUIC; Discord RTC desteği.
- **Dengeli:** Aynı hizmetlerde daha güçlü fake + multisplit stratejisi.
- **Agresif:** Tüm web trafiği ve ilk bilinmeyen UDP paketleri. Diğer profiller çalışmazsa kullanılmalıdır.

Alan adına göre filtreleme TLS SNI/HTTP Host ve QUIC Initial aşamasında yapılabilir. Discord ses ve Roblox oyun paketleri bağlantı kurulduktan sonra alan adı taşımadığından UDP filtreleri protokol/port temellidir.

## Mimari ve güvenlik sınırı

- `turkdpi-gui`: Root olmayan Qt 6/QML arayüzü. NetworkManager bağlantı adını sistem D-Bus üzerinden okur.
- `turkdpi-service`: Root yetkili Rust yardımcısı. Yalnızca sabit eylemleri ve beyaz listedeki profil/DNS değerlerini kabul eder; shell oluşturmaz.
- Polkit: GUI, yardımcının mutlak yolunu `pkexec` ile çağırır.
- nftables: Yalnızca `table inet turkdpi` oluşturulur/silinir. NFQUEUE kuralları `bypass` içerir; motor yoksa trafik kesilmez. Loopback ile DNS, DHCPv4/v6 ve NTP UDP trafiği kuyruğa alınmaz.
- DNS: `/etc/resolv.conf` doğrudan yazılmaz. Etkin NetworkManager bağlantısı `nmcli` process API ile değiştirilir.
- Gizlilik: Paket içeriği, kullanıcı mesajı, DNS sorgusu veya trafik kaydı tutulmaz.

Her ağ değişikliğinde NetworkManager dispatcher etkin systemd servisini yeniden başlatır. Otomatik profil sonucu bağlantı UUID’siyle `/var/lib/turkdpi/networks/` altında saklanır. DNS yedeği `/var/lib/turkdpi/dns-backups/` altında kip `0600` ile tutulur.

## CachyOS kurulumu

```bash
sudo pacman -S --needed base-devel cargo cmake git ninja rust qt6-base qt6-declarative \
  networkmanager nftables polkit curl libnetfilter_queue libnfnetlink libmnl zlib-ng-compat
```

CachyOS klasik `zlib` yerine `zlib-ng-compat` kullanır. Pacman bu paketi veya `lib32-zlib-ng-compat` paketini kaldırmayı önerirse işlemi iptal edin.

Kaynak deposundan paket oluşturma:

```bash
git clone https://github.com/semih-emre/turkdpi.git
cd turkdpi
git archive --prefix=turkdpi-0.2.0/ -o turkdpi-0.2.0.tar.gz HEAD
makepkg -Csi
```

`PKGBUILD`, Zapret `v72.10` sürümünü tam commit kimliğine sabitler. İlk derlemede Zapret kaynağı indirilir.

Mevcut kurulumdan güncelleme:

```bash
cd turkdpi
git pull --ff-only
git archive --prefix=turkdpi-0.2.0/ -o turkdpi-0.2.0.tar.gz HEAD
makepkg -Csi
```

## Kullanım

```bash
turkdpi-gui
```

Bir profil başlatıldığında Cloudflare DNS varsayılan olarak etkinleşir. GUI’deki DNS kutusu kapatılırsa bağlantının önceki DNS değerleri geri yüklenir.

Komut satırından DNS yönetimi:

```bash
sudo turkdpi-service set-dns cloudflare
sudo turkdpi-service set-dns automatic
```

Sistem açılışında otomatik profil testi:

```bash
sudo systemctl enable --now turkdpi.service
```

Discord RTC uçtan uca testi kullanıcı oturumu gerektirdiğinden otomatik test yalnız DNS, Discord/Roblox HTTPS, Discord Gateway TCP ve yerel UDP gönderimini doğrular. “UDP gönderildi” sonucu ses sunucusundan yanıt alındığı anlamına gelmez.

## Kaldırma ve kurtarma

```bash
scripts/uninstall.sh
```

GUI yanıt vermiyorsa:

```bash
sudo systemctl stop turkdpi.service
sudo turkdpi-service cleanup
sudo nft list table inet turkdpi
```

Son komutun “No such file or directory” vermesi beklenir. `cleanup`, TurkDPI’nin nfqws süreçlerini durdurur, yalnız `inet turkdpi` tablosunu siler ve yedeklenmiş NetworkManager DNS değerlerini geri yükler.

Yardımcı ikili yoksa yalnız uygulama tablosunu açıkça silin:

```bash
sudo nft delete table inet turkdpi
```

Hiçbir zaman `nft flush ruleset` çalıştırmayın. Her ağ kuralı değişikliğinden önce tam nftables görünümü `/var/lib/turkdpi/backups/ruleset-*.json` altında saklanır.

## Bilinen sınırlar

- Türkiye’de DPI davranışı ISS, rota ve zamana göre değişebilir; tek bir stratejinin her bağlantıda çalışması garanti edilemez.
- Discord RTC testi oturum açmadan tamamlanamaz.
- Roblox oyun trafiği dinamik UDP portu ve IP kullanır. Roblox profili `49152–65535/UDP` aralığındaki tanınmayan ilk paketleri işler ve aynı aralıktaki başka uygulamaları etkileyebilir.
- Agresif profil bilinmeyen UDP trafiğine de müdahale eder; yalnız son seçenek olarak kullanılmalıdır.
- Cloudflare DNS captive portal kullanan halka açık Wi‑Fi ağlarında giriş sayfasını engelleyebilir. Bu durumda DNS kutusunu kapatın.
- Linux çekirdeği, systemd, Polkit, NetworkManager ve nftables canlı testi bir CachyOS makinesinde yapılmalıdır.

## Doğrulama

```bash
cargo fmt --check
cargo check
cargo clippy --all-targets -- -D warnings
cmake -S gui -B build/gui -G Ninja
cmake --build build/gui
shellcheck scripts/*.sh scripts/90-turkdpi scripts/turkdpi-sleep
namcap PKGBUILD
```

Canlı ağ testi öncesinde ayrıca `sudo nft -j list ruleset > nft-before.json` ile elle yedek alın.
