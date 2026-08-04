# TurkDPI 0.4.1

TurkDPI; CachyOS x86_64/KDE Plasma ile Debian 13 ve Raspberry Pi OS ARM64 üzerinde Discord, Roblox ve genel web erişimine odaklanan bir `nfqws` yöneticisidir. VPN değildir; trafiği uzak bir sunucuya taşımaz, TLS çözmez, telemetri toplamaz ve kullanıcı trafiğini kaydetmez.

> Bu yazılım ağ paketlerinin aktarım biçimini ve seçildiğinde NetworkManager DNS ayarını değiştirir. Yerel mevzuata ve kullandığınız hizmetlerin koşullarına uygun kullanmak sizin sorumluluğunuzdadır.

## 0.4.1 yenilikleri

- NetworkManager DNS/DHCP olaylarının systemd servisini sürekli yeniden başlatması önlendi.
- Manuel `systemctl stop` sonrası DNS geri yükleme olayının servisi yeniden açması engellendi.
- Açılış, durdurma ve yeniden başlatma yaşam döngüsü Raspberry Pi 5 üzerinde doğrulandı.

### 0.4.0 ile eklenenler

- Raspberry Pi 5 ARM64 ve Debian 13 için yerel `.deb` paket üretimi.
- Zapret/nfqws motorunu sabitlenmiş commit’ten ARM64 üzerinde otomatik derleme.
- Arch/CachyOS ve Debian/Raspberry Pi OS arasında çalışan ortak uygulama içi güncelleyici.
- Debian’daki `/usr/sbin/nft` yolu ve KDE dışındaki `x-terminal-emulator` desteği.
- Arch paketinde `aarch64` mimarisi ve dağıtımlar arası `zlib` sanal bağımlılığı.
- UDP/53 üzerindeki Cloudflare DNS engellenirse otomatik devreye giren şifreli Cloudflare DoH yedeği.

### 0.3.0 ile eklenenler

- Açılışta ve altı saatte bir GitHub üzerinden sürüm denetimi, KDE masaüstü bildirimi ve uygulama içi **Güncelle** düğmesi.
- Güncelleme kaynak kodunu normal kullanıcı hesabında derler; yalnızca oluşan paketin kurulumu Polkit onayı ister.
- Oturum gerektiren Discord/Roblox ana sayfalarını başarı şartı saymayan, hata ayrıntılarını gösteren daha güvenilir otomatik test.
- Discord Voice IP Discovery/STUN için resmi Zapret örnekleriyle uyumlu, bozuk sağlama toplamı kullanmayan RTC stratejisi.

### 0.2.0 ile eklenenler

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
- DNS: `/etc/resolv.conf` doğrudan yazılmaz. Etkin NetworkManager bağlantısı `nmcli` process API ile değiştirilir. Önce `1.1.1.1` ve `1.0.0.1` denenir; UDP DNS engelliyse yalnız loopback üzerinde çalışan şifreli Cloudflare DoH yedeği kullanılır.
- Gizlilik: Paket içeriği, kullanıcı mesajı, DNS sorgusu veya trafik kaydı tutulmaz.

Her ağ değişikliğinde NetworkManager dispatcher etkin systemd servisini yeniden başlatır. Otomatik profil sonucu bağlantı UUID’siyle `/var/lib/turkdpi/networks/` altında saklanır. DNS yedeği `/var/lib/turkdpi/dns-backups/` altında kip `0600` ile tutulur.

## Raspberry Pi 5 / Debian 13 kurulumu

Raspberry Pi OS 64-bit veya Debian 13 ARM64 üzerinde:

```bash
sudo apt update
sudo apt install --no-install-recommends build-essential cargo cmake dpkg-dev git \
  dnscrypt-proxy libcap-dev libmnl-dev libnetfilter-queue-dev libnfnetlink-dev network-manager ninja-build \
  nftables pkexec qt6-base-dev qt6-declarative-dev qt6-qpa-plugins \
  qml6-module-qtqml-workerscript qml6-module-qtquick \
  qml6-module-qtquick-controls qml6-module-qtquick-layouts \
  qml6-module-qtquick-templates qml6-module-qtquick-window zlib1g-dev

git clone https://github.com/semih-emre/turkdpi.git
cd turkdpi
./scripts/build-deb.sh
sudo apt install ./build/deb/turkdpi_0.4.1_arm64.deb
```

## Hazır amd64 ve arm64 paketleri

GitHub Actions her `main` güncellemesinde iki mimari için yerel Linux paketleri üretir:

- `turkdpi-amd64`: Intel/AMD 64-bit Debian, Ubuntu ve uyumlu dağıtımlar.
- `turkdpi-arm64`: Raspberry Pi 5 ve diğer ARM64 Debian tabanlı sistemler.
- `turkdpi-cachyos-x86_64`: CachyOS ve Arch tabanlı Intel/AMD 64-bit sistemler
  için pacman ile kurulabilen `.pkg.tar.zst` paketi.

Qt arayüzünün asgari sürümü, Ubuntu 24.04 ile uyumlu olacak şekilde Qt 6.4'tür.

Paketler deponun **Actions → Linux paketlerini derle** sayfasındaki başarılı
çalışmanın **Artifacts** bölümünden indirilebilir. Her pakette servis, Qt arayüzü
ve `nfqws` dosyalarının hedef CPU mimarisi otomatik doğrulanır.

Grafik masaüstü olmayan Pi kurulumunda servis ve komut satırı aracı kullanılabilir:

```bash
sudo turkdpi-service start discord
sudo turkdpi-service status
sudo turkdpi-service cleanup
```

Qt arayüzü için çalışan bir Wayland/X11 oturumu gerekir. Paket, grafik oturumu olmasa da derlenebilir ve servis olarak çalışabilir.

## CachyOS kurulumu

```bash
sudo pacman -S --needed base-devel cargo cmake dnscrypt-proxy git konsole ninja rust qt6-base qt6-declarative \
  networkmanager nftables polkit curl libnetfilter_queue libnfnetlink libmnl zlib-ng-compat
```

CachyOS klasik `zlib` yerine `zlib-ng-compat` kullanır. Pacman bu paketi veya `lib32-zlib-ng-compat` paketini kaldırmayı önerirse işlemi iptal edin.

Kaynak deposundan paket oluşturma:

```bash
git clone https://github.com/semih-emre/turkdpi.git
cd turkdpi
git archive --prefix=turkdpi-0.4.1/ -o turkdpi-0.4.1.tar.gz HEAD
makepkg -Csi
```

`PKGBUILD`, Zapret `v72.10` sürümünü tam commit kimliğine sabitler. İlk derlemede Zapret kaynağı indirilir.

Mevcut kurulumdan güncelleme:

```bash
cd turkdpi
git pull --ff-only
git archive --prefix=turkdpi-0.4.1/ -o turkdpi-0.4.1.tar.gz HEAD
makepkg -Csi
```

## Kullanım

```bash
turkdpi-gui
```

Bir profil başlatıldığında Cloudflare DNS varsayılan olarak etkinleşir. Doğrudan `1.1.1.1`/`1.0.0.1` yanıt vermezse uygulama paketle gelen sabit Cloudflare doğrulayıcılarıyla `127.0.3.1` üzerinde şifreli DoH yedeğini başlatır. GUI’deki DNS kutusu kapatılırsa süreç durdurulur ve bağlantının önceki DNS değerleri geri yüklenir.

### Uygulama içinden güncelleme

TurkDPI açıldıktan kısa süre sonra ve uygulama açık kaldığı sürece altı saatte bir `version.json` dosyasını projenin GitHub `main` dalından denetler. Daha yeni bir sürüm varsa masaüstü bildirimi gösterilir ve **Güncelle** düğmesi etkinleşir. Düğme Konsole veya sistemin varsayılan terminalini açar, HTTPS ile kaynak kodunu indirir, bildirilen sürüm ile kaynağın sürümünü karşılaştırır ve paketi normal kullanıcı hesabında derler. CachyOS’ta `pacman`, Debian/Raspberry Pi OS’ta `apt` ile yapılan son kurulum adımı Polkit onayı ister.

Güncelleme bittikten sonra uygulamayı kapatıp yeniden açın. Denetim başarısız olursa mevcut sürüm çalışmaya devam eder; arayüzde hata açıklaması gösterilir.

Komut satırından DNS yönetimi:

```bash
sudo turkdpi-service set-dns cloudflare
sudo turkdpi-service set-dns automatic
```

Sistem açılışında otomatik profil testi:

```bash
sudo systemctl enable --now turkdpi.service
```

Discord RTC uçtan uca testi kullanıcı oturumu gerektirdiğinden otomatik test yalnız DNS çözümlemesini, kimlik doğrulama istemeyen Discord Gateway HTTPS uç noktasını ve yerel UDP gönderimini doğrular. “UDP gönderildi” sonucu ses sunucusundan yanıt alındığı anlamına gelmez.

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
- Cloudflare DNS veya şifreli DoH captive portal kullanan halka açık Wi‑Fi ağlarında giriş sayfasını engelleyebilir. Bu durumda DNS kutusunu kapatın.
- Raspberry Pi OS Lite gibi grafik oturumu olmayan sistemlerde Qt arayüzü açılamaz; servis ve CLI kullanılabilir.
- Raspberry Pi’de Discord/Roblox istemcisi çalışmıyorsa gerçek uygulama trafiğinin uçtan uca testi aynı ağdaki istemci cihazdan yapılmalıdır.

## Doğrulama

```bash
cargo fmt --check
cargo check
cargo clippy --all-targets -- -D warnings
cmake -S gui -B build/gui -G Ninja
cmake --build build/gui
shellcheck scripts/*.sh scripts/90-turkdpi scripts/turkdpi-sleep
namcap PKGBUILD
scripts/build-deb.sh
```

Canlı ağ testi öncesinde ayrıca `sudo nft -j list ruleset > nft-before.json` ile elle yedek alın.
