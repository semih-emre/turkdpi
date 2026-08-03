# Maintainer: TurkDPI contributors
pkgname=turkdpi
pkgver=0.1.0
pkgrel=1
pkgdesc='CachyOS/KDE için Discord odaklı yerel DPI dayanıklılığı yöneticisi'
arch=('x86_64')
url='https://github.com/bol-van/zapret'
license=('MIT')
depends=('curl' 'libmnl' 'libnetfilter_queue' 'libnfnetlink' 'networkmanager' 'nftables' 'polkit' 'qt6-base' 'qt6-declarative' 'zlib-ng-compat')
makedepends=('cargo' 'cmake' 'git' 'ninja' 'rust')
source=("${pkgname}-${pkgver}.tar.gz"
        'zapret::git+https://github.com/bol-van/zapret.git#commit=f0b0d89f02f44bb047fbfde5d96e9a1fc38e46f0')
sha256sums=('SKIP' 'SKIP')

build() {
  cd "${srcdir}/${pkgname}-${pkgver}"
  cargo build --release
  cmake -S gui -B build/gui -G Ninja -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX=/usr
  cmake --build build/gui
  make -C "${srcdir}/zapret/nfq"
}

check() {
  cd "${srcdir}/${pkgname}-${pkgver}"
  cargo test --release
}

package() {
  cd "${srcdir}/${pkgname}-${pkgver}"
  install -Dm755 target/release/turkdpi-service "${pkgdir}/usr/bin/turkdpi-service"
  DESTDIR="${pkgdir}" cmake --install build/gui
  install -Dm755 "${srcdir}/zapret/nfq/nfqws" "${pkgdir}/usr/lib/turkdpi/nfqws"
  install -Dm644 systemd/turkdpi.service "${pkgdir}/usr/lib/systemd/system/turkdpi.service"
  install -Dm644 polkit/org.turkdpi.manage.policy "${pkgdir}/usr/share/polkit-1/actions/org.turkdpi.manage.policy"
  install -Dm755 scripts/90-turkdpi "${pkgdir}/etc/NetworkManager/dispatcher.d/90-turkdpi"
  install -Dm755 scripts/turkdpi-sleep "${pkgdir}/usr/lib/systemd/system-sleep/turkdpi"
  install -Dm644 profiles/*.toml -t "${pkgdir}/usr/share/turkdpi/profiles"
  install -Dm644 profiles/discord-hosts.txt "${pkgdir}/usr/share/turkdpi/discord-hosts.txt"
  install -Dm644 README.md "${pkgdir}/usr/share/doc/turkdpi/README.md"
  install -Dm644 LICENSE "${pkgdir}/usr/share/licenses/turkdpi/LICENSE"
  install -Dm644 "${srcdir}/zapret/LICENSE.txt" "${pkgdir}/usr/share/licenses/turkdpi/LICENSE.zapret"
}
