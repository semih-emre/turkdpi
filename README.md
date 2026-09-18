# TurkDPI

Engellenen servislere VPN olmadan erişmek için, ağı **ölçüp** uygun kaçınma
stratejisini **arayarak bulan** bir araç. Windows, Linux ve macOS'ta çalışır.

Bu bir VPN ya da proxy hizmeti değildir: trafik hiçbir üçüncü sunucuya
yönlendirilmez. Yapılan tek şey, ilk paketlerin engelleme donanımı tarafından
eşleştirilmesini önlemek.

---

## Neden başka araçlardan farklı

Sabit ayar listesi denemek Türkiye'de güvenilir çalışmıyor, çünkü operatörler
farklı DPI donanımı kullanıyor: doğru TTL değeri, bölme konumu ve hile yöntemi
ağdan ağa değişiyor. TurkDPI tahmin etmek yerine ölçüyor.

**1. Teşhis.** Engellemenin *nasıl* yapıldığı belirleniyor:

| Teşhis | Anlamı |
|---|---|
| `sni_reset` | ClientHello sonrası RST enjekte ediliyor (Türkiye'de en yaygın) |
| `sni_drop` | Paket sessizce düşürülüyor |
| `block_page` | 443 portundan düz metin HTTP yanıtı dönüyor |
| `dns_poisoned` | Sistem çözümleyicisi sahte adres veriyor |
| `ip_blocked` | TCP bağlantısı hiç kurulamıyor |

**2. DPI mesafesinin ölçümü.** En değerli adım bu. Kurulu bir bağlantıda
ClientHello artan TTL değerleriyle gönderiliyor; RST'nin geldiği ilk TTL,
engelleme donanımının kaç hop uzakta olduğunu veriyor. Bu sayı doğrudan
`--dpi-desync-ttl` (zapret) ve `--ttl` (ByeDPI) değerine dönüşüyor — sahte
paketin DPI'ı kandırıp gerçek sunucuya ulaşmaması için tam olarak gereken şey.

**3. Strateji araması.** Teşhise göre aday stratejiler üretilip en olasıdan
başlayarak sırayla deneniyor. RST enjeksiyonu varsa sahte paket yöntemleri,
paket düşürülüyorsa SNI'yi segmentlere yayan bölme yöntemleri öne alınıyor.
Her aday gerçek bir TLS el sıkışmasıyla doğrulanıyor.

**4. Ağ hafızası.** Çalışan strateji ağ bazında kaydediliyor; aynı ağa tekrar
bağlanıldığında arama yapılmadan uygulanıyor. Kayıtlı strateji yine de körü
körüne güvenilmiyor — önce doğrulanıyor, tutmazsa tam arama yapılıyor.

---

## Motorlar

Aynı strateji, sistemde ne varsa ona çevriliyor:

| Arka uç | Platform | Yöntem | Yetki | Kapsam |
|---|---|---|---|---|
| `nfqws` | Linux | nftables NFQUEUE | root | tüm trafik |
| `winws` | Windows | WinDivert sürücüsü | yönetici | tüm trafik |
| `ciadpi` (ByeDPI) | hepsi | yerel SOCKS5 proxy | **gerekmez** | proxy'ye yönlendirilen |

Seçim otomatik: yetki varsa şeffaf mod, yoksa proxy moduna düşülüyor. Böylece
yönetici yetkisi olmayan kullanıcı da "hiç çalışmadı" durumunda kalmıyor.

Şeffaf mod tercih ediliyor çünkü uygulama ayarı gerektirmiyor ve UDP'yi de
kapsıyor — Discord sesi için gereken bu.

---

## Kurulum

### Windows

Sürüm sayfasından `turkdpi-windows-x64.zip` indirip açın, `turkdpi-gui.exe`
çalıştırın. Şeffaf mod istiyorsanız yönetici olarak çalıştırın.

### Linux (Debian / Ubuntu / Raspberry Pi)

```bash
sudo apt install ./turkdpi_<sürüm>_<mimari>.deb
```

### Arch / CachyOS

```bash
makepkg -si
```

### macOS

`turkdpi-macos-arm64.zip` indirip açın. İmzasız olduğu için ilk açılışta
Gatekeeper uyarı verir; Sistem Ayarları → Gizlilik ve Güvenlik üzerinden izin
vermeniz gerekir.

---

## Kullanım

Arayüzde tek düğme yeterli: **Otomatik Düzelt**. Komut satırından:

```bash
turkdpi-service auto
```

Diğer komutlar:

| Komut | İşlev |
|---|---|
| `auto` | teşhis et, stratejiyi bul ve uygula |
| `stop` | motoru durdur, kuralları geri al |
| `status` | mevcut durumu JSON olarak yaz |
| `test` | hedeflere erişimi ölç |
| `diagnose [alan adı…]` | engellemenin nasıl yapıldığını teşhis et |
| `selftest [alan adı]` | TTL ölçüm düzeneğini doğrula |
| `backends` | kullanılabilir motorları listele |
| `info` | ortamı JSON olarak bildir |
| `logs [satır]` | günlükleri göster |
| `install-engine [-f]` | proxy motorunu indir ve doğrula |
| `set-dns <mod>` | `cloudflare` \| `automatic` (şimdilik yalnızca Linux) |

### Proxy modu kullanıyorsanız

Trafik kendiliğinden yönlenmez. Arayüzdeki adresi (`127.0.0.1:<port>`)
uygulamanın SOCKS5 proxy ayarına girin. Discord'da: Ayarlar → Ses ve Görüntü
bölümünde proxy ayarı yoktur; sistem proxy'sini kullanmak ya da yönetici
yetkisiyle şeffaf moda geçmek gerekir.

---

## Günlükler

Hata bildirirken göndereceğiniz dosya burada:

| Platform | Konum |
|---|---|
| Windows | `%ProgramData%\TurkDPI\logs` |
| Linux | `/var/lib/turkdpi/logs` |
| macOS | `/Library/Application Support/TurkDPI/logs` |

Arayüzdeki **Günlükleri Aç** düğmesi doğrudan bu klasörü açar.

Kayıtlar günlük dosyalara yazılır, **son 3 gün** saklanır ve daha eskiler
otomatik silinir. Tek bir dosya 4 MB'ı aşarsa baş tarafı atılıp son kısım
korunur. Trafik içeriği ya da ziyaret edilen adresler kaydedilmez; yalnızca
teşhis sonucu, denenen stratejiler ve hata mesajları tutulur.

---

## Motor ikilileri

Üçüncü taraf ikililer bu depoya commit edilmez. `engine/manifest.json` hangi
sürümün nereden indirileceğini ve SHA-256 özetini sabitler; `install-engine`
indirdiği dosyayı bu özetle doğrular ve eşleşmezse kurulumu iptal eder.

Doğrulama isteğe bağlı değil: indirme, tam da bu aracın aşmaya çalıştığı türden
müdahaleye açık bir ağ üzerinden geçiyor.

Kullanılan projeler:

- [zapret](https://github.com/bol-van/zapret) — `nfqws`, `winws` (MIT)
- [ByeDPI](https://github.com/hufrea/byedpi) — `ciadpi` (GPL-3.0)

---

## Derleme

Rust 1.88+ ve Qt 6.4+ gerekir.

```bash
cargo test                        # çekirdek testleri
cargo build --release             # servis
cmake -S gui -B build/gui && cmake --build build/gui   # arayüz
```

Bağımlılıklar bilerek asgari tutuldu: `anyhow`, `serde`, `serde_json`, `toml`
ve Unix'te `libc`. TLS, kriptografi ve takvim kütüphanesi yok — gereken asgari
kod (ham ClientHello üretimi, SHA-256, tarih dönüşümü) `src/wire.rs`,
`src/sha256.rs` ve `src/util.rs` içinde yazıldı. Bunun nedeni taşınabilirlik:
bu kütüphanelerin çoğu C derleyicisi ya da import kütüphanesi üretimi
gerektiriyor ve projenin derlenebildiği toolchain sayısını daraltıyordu.

### Mimari

| Modül | Sorumluluk |
|---|---|
| `wire` | ham TLS ClientHello, DNS sorgusu, yanıt sınıflandırma |
| `probe` | engelleme teşhisi ve DPI mesafesi ölçümü |
| `strategy` | aday strateji üretimi, zapret/ByeDPI argümanlarına çevirme |
| `engine` | arka uç seçimi, süreç ve güvenlik duvarı yaşam döngüsü |
| `verify` | stratejinin gerçekten işe yarayıp yaramadığının ölçümü |
| `search` | teşhisten çalışan yapılandırmaya kadar olan akış |
| `socks` | SOCKS5 istemcisi (proxy modunu doğrulamak için) |
| `log` | diske günlük, yaş ve boyut sınırlarıyla |

---

## Sınırlar

- **IP seviyesinde engellemede** paket kurcalamanın faydası yok; teşhis bunu
  `ip_blocked` olarak bildirir ve dürüstçe söyler.
- **DNS değiştirme** şimdilik yalnızca Linux'ta (NetworkManager üzerinden).
  Windows ve macOS'ta zehirlenmiş DNS, proxy motorunun isim çözümlemeyi kendi
  tarafında yapmasıyla atlatılır.
- **macOS'ta şeffaf mod yok**; proxy modu çalışır.
- Ağ parmak izi yerel alt ağdan üretilir, bu yüzden iki farklı ağ aynı özel
  blokta (örneğin `192.168.1.0/24`) çakışabilir. Yanlış eşleşmenin bedeli
  birkaç saniye: kayıtlı strateji doğrulamayı geçemezse tam arama yapılır.

---

## Lisans

MIT. Ayrıntılar için [LICENSE](LICENSE).
