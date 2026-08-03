# TurkDPI 0.1.0

TurkDPI, CachyOS x86_64 ve KDE Plasma için Discord odaklı bir `nfqws` yöneticisi başlangıç deposudur. VPN değildir; trafiği uzak sunucuya taşımaz, TLS çözmez ve telemetri toplamaz. TCP 80/443 paketlerini `nfqws` hostlist filtresinden, UDP 443 ve 50000–65535 paketlerini ayrı bir kuyruktan geçirir.

> Bu yazılım ağ paketlerinin aktarım biçimini değiştirir. Yerel mevzuata ve kullandığınız hizmetlerin koşullarına uygun kullanmak sizin sorumluluğunuzdadır.

## Mimari ve güvenlik sınırı

- `turkdpi-gui`: Root olmayan Qt 6/QML arayüzü. NetworkManager bağlantı adını sistem D-Bus üzerinden okur.
- `turkdpi-service`: Küçük, root yetkili Rust yardımcısı. Yalnızca `start`, `stop`, `test`, `auto`, `cleanup`, `status` ve `set-profile` eylemlerini kabul eder.
- Polkit: GUI, yardımcının mutlak yolunu `pkexec` ile çağırır. Profil hem GUI hem servis tarafında beyaz listeyle doğrulanır; shell oluşturulmaz.
- nftables: Yalnızca `table inet turkdpi` oluşturulur/silinir. Kuyruk kuralları `bypass` içerir; NFQUEUE dinleyicisi yoksa trafik kesilmez.
- Gizlilik: Paket içeriği, kullanıcı mesajı, DNS içeriği veya trafik kaydı tutulmaz. Durum dosyası yalnızca profil ve sonuç özetidir.

Her ağ değişikliğinde NetworkManager dispatcher etkin systemd servisini yeniden başlatır. Otomatik test sonucu, `nmcli` ile okunan ve biçimi doğrulanan bağlantı UUID'sine göre `/var/lib/turkdpi/networks/` altında saklanır. Uykudan dönüşte aynı yeniden test akışı çalışır.

## CachyOS kurulumu (önerilen: makepkg)

Gerekli geliştirme paketleri:

```bash
sudo pacman -S --needed base-devel cargo cmake git ninja rust qt6-base qt6-declarative \
  networkmanager nftables polkit curl libnetfilter_queue libnfnetlink libmnl zlib-ng-compat
```

CachyOS, klasik `zlib` yerine onunla uyumlu ve optimize edilmiş `zlib-ng-compat` paketini kullanır. Pacman `zlib-ng-compat` paketini kaldırmayı önerirse işlemi iptal edin; özellikle `lib32-zlib-ng-compat` kurulu sistemlerde klasik `zlib` paketine geçmeyin.

Yerel kaynak arşivini PKGBUILD'in beklediği adla oluşturup paketi kurun:

```bash
cd turkdpi
tar --exclude='./target' --exclude='./build' --exclude='./engine' --exclude='./turkdpi-0.1.0.tar.gz' \
  -czf turkdpi-0.1.0.tar.gz --transform='s,^\.,turkdpi-0.1.0,' .
makepkg -si
```

`PKGBUILD`, Zapret `v72.10` etiketinin tam commit kimliğine sabitlenmiştir. Dağıtılacak paketlerde yerel arşiv için `SKIP` yerine `updpkgsums` ile gerçek SHA-256 değeri yazılmalıdır.

Alternatif `scripts/install.sh`, önceden derlenmiş `engine/nfqws` bekler. Kaynak paketleme yolu daha tekrarlanabilirdir.

## Kullanım

Menüden **Türkiye DPI Yöneticisi** uygulamasını açın veya:

```bash
turkdpi-gui
```

İlk olarak **Bağlantıyı Test Et**, ardından **Otomatik Test** kullanılabilir. Otomatik sıra `safe → discord → balanced → aggressive` şeklindedir ve ilk geçen profil ağ UUID'siyle kaydedilir. UDP kontrolü yalnızca datagram gönderilebildiğini doğrular; karşı uç yanıtı veya Discord ses oturumu doğrulanmış sayılmaz.

Sistem açılışı:

```bash
sudo systemctl enable --now turkdpi.service
```

## Kaldırma

```bash
scripts/uninstall.sh
```

Betik yalnızca TurkDPI dosyalarını ve `inet turkdpi` tablosunu kaldırır. Kurtarma amacıyla `/var/lib/turkdpi` korunur.

## Kurtarma

GUI yanıt vermiyorsa:

```bash
sudo systemctl stop turkdpi.service
sudo /usr/bin/turkdpi-service cleanup
sudo nft list table inet turkdpi
```

Son komutun “No such file or directory” vermesi beklenir. Yardımcı kurulu değilse yalnızca uygulamaya ait tabloyu açıkça silin:

```bash
sudo nft delete table inet turkdpi
```

Hiçbir zaman genel `nft flush ruleset` çalıştırmayın. Her `start`, `stop` veya `cleanup` öncesindeki tam nftables görünümü `/var/lib/turkdpi/backups/ruleset-*.json` altında kip `0600` ile saklanır. Bu JSON dosyaları inceleme içindir; körlemesine geri yüklenmemelidir.

## Bilinen 0.1.0 sınırları

- Discord ses bağlantısı, oturum açmadan uçtan uca doğrulanamaz; UDP testi kesin başarı ölçümü değildir.
- UDP 443 ve yüksek ses portları alan adına göre filtrelenemediğinden bu portlardaki diğer uygulama trafiği de NFQUEUE'ya girer. `bypass` ve nfqws filtreleri bağlantının kilitlenmesini önler, ancak üretim öncesi yerel ağda sınanmalıdır.
- GUI ağ değişikliğini gösterir; dispatcher ile otomatik yeniden test yalnızca systemd servisi etkinleştirildiğinde çalışır.
- Bu Windows geliştirme ortamında Linux çekirdeği, systemd, Polkit, Qt 6 ve nftables entegrasyon testi yapılamaz. Kontroller bir CachyOS makinesinde tamamlanmalıdır.

## Yayın öncesi doğrulama

```bash
cargo fmt --check
cargo check
cargo clippy --all-targets -- -D warnings
cmake -S gui -B build/gui -G Ninja
cmake --build build/gui
shellcheck scripts/*.sh scripts/90-turkdpi scripts/turkdpi-sleep
namcap PKGBUILD
sudo nft -c -f /path/to/generated-rules.nft  # yalnızca ayrı test dosyasıyla
```

Canlı ağ testi öncesinde `sudo nft -j list ruleset > nft-before.json` ile ayrıca elle yedek alın.
