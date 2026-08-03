#!/usr/bin/env bash
set -euo pipefail

if [[ ${EUID} -eq 0 ]]; then
  printf '%s\n' 'Paketi root olarak değil, normal kullanıcı hesabıyla derleyin.' >&2
  exit 1
fi

project_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
version=$(sed -nE 's/^[[:space:]]*"version"[[:space:]]*:[[:space:]]*"([0-9]+\.[0-9]+\.[0-9]+)"[,]?[[:space:]]*$/\1/p' "${project_dir}/version.json")
architecture=$(dpkg --print-architecture)
zapret_commit=f0b0d89f02f44bb047fbfde5d96e9a1fc38e46f0
build_dir=${project_dir}/build
deb_dir=${build_dir}/deb
package_root=${deb_dir}/root
zapret_dir=${build_dir}/zapret
package_file=${deb_dir}/turkdpi_${version}_${architecture}.deb

if [[ ! ${version} =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  printf '%s\n' 'version.json içindeki sürüm geçersiz.' >&2
  exit 2
fi
if [[ ${architecture} != arm64 && ${architecture} != amd64 ]]; then
  printf 'Desteklenmeyen Debian mimarisi: %s\n' "${architecture}" >&2
  exit 3
fi
for command_name in cargo cmake dpkg dpkg-deb git install make ninja sed; do
  if ! command -v "${command_name}" >/dev/null; then
    printf 'Gerekli derleme komutu bulunamadı: %s\n' "${command_name}" >&2
    exit 4
  fi
done

cargo build --manifest-path "${project_dir}/Cargo.toml" --release
cmake -S "${project_dir}/gui" -B "${build_dir}/gui" -G Ninja \
  -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX=/usr
cmake --build "${build_dir}/gui"

if [[ ! -d ${zapret_dir}/.git ]]; then
  git clone --filter=blob:none --no-checkout https://github.com/bol-van/zapret.git "${zapret_dir}"
fi
git -C "${zapret_dir}" fetch --depth=1 origin "${zapret_commit}"
git -C "${zapret_dir}" checkout --detach FETCH_HEAD
make -C "${zapret_dir}/nfq" clean
make -C "${zapret_dir}/nfq"

if [[ ${package_root} != "${project_dir}/build/deb/root" ]]; then
  printf '%s\n' 'Güvenli olmayan paket kökü reddedildi.' >&2
  exit 5
fi
rm -rf -- "${package_root}"
mkdir -p -- "${package_root}/DEBIAN"
DESTDIR="${package_root}" cmake --install "${build_dir}/gui"

install -Dm755 "${project_dir}/target/release/turkdpi-service" "${package_root}/usr/bin/turkdpi-service"
install -Dm755 "${zapret_dir}/nfq/nfqws" "${package_root}/usr/lib/turkdpi/nfqws"
install -Dm644 "${project_dir}/systemd/turkdpi.service" "${package_root}/usr/lib/systemd/system/turkdpi.service"
install -Dm644 "${project_dir}/polkit/org.turkdpi.manage.policy" "${package_root}/usr/share/polkit-1/actions/org.turkdpi.manage.policy"
install -Dm755 "${project_dir}/scripts/90-turkdpi" "${package_root}/etc/NetworkManager/dispatcher.d/90-turkdpi"
install -Dm755 "${project_dir}/scripts/turkdpi-sleep" "${package_root}/usr/lib/systemd/system-sleep/turkdpi"
install -Dm644 "${project_dir}"/profiles/*.toml -t "${package_root}/usr/share/turkdpi/profiles"
install -Dm644 "${project_dir}"/profiles/*.txt -t "${package_root}/usr/share/turkdpi"
install -Dm644 "${project_dir}/version.json" "${package_root}/usr/share/turkdpi/version.json"
install -Dm644 "${project_dir}/README.md" "${package_root}/usr/share/doc/turkdpi/README.md"
install -Dm644 "${project_dir}/LICENSE" "${package_root}/usr/share/doc/turkdpi/copyright"
install -Dm644 "${zapret_dir}/docs/LICENSE.txt" "${package_root}/usr/share/doc/turkdpi/copyright.zapret"

installed_size=$(du -sk "${package_root}" | cut -f1)
cat >"${package_root}/DEBIAN/control" <<EOF
Package: turkdpi
Version: ${version}
Section: net
Priority: optional
Architecture: ${architecture}
Maintainer: TurkDPI contributors <smhtpl330@gmail.com>
Installed-Size: ${installed_size}
Depends: curl, libmnl0, libnetfilter-queue1, libnfnetlink0, libqt6dbus6, libqt6network6, network-manager, nftables, pkexec, qml6-module-qtqml-workerscript, qml6-module-qtquick, qml6-module-qtquick-controls, qml6-module-qtquick-layouts, qt6-qpa-plugins, zlib1g
Description: Discord, Roblox ve web için yerel DPI dayanıklılığı yöneticisi
 Raspberry Pi 5 ARM64, Debian ve CachyOS üzerinde nfqws, nftables ve
 NetworkManager kullanarak seçilebilir yerel profiller sağlar.
EOF
cat >"${package_root}/DEBIAN/postinst" <<'EOF'
#!/bin/sh
set -e
systemctl daemon-reload 2>/dev/null || true
exit 0
EOF
cat >"${package_root}/DEBIAN/prerm" <<'EOF'
#!/bin/sh
set -e
if [ "$1" = remove ]; then
  systemctl disable --now turkdpi.service 2>/dev/null || true
  /usr/bin/turkdpi-service cleanup 2>/dev/null || true
fi
exit 0
EOF
cat >"${package_root}/DEBIAN/postrm" <<'EOF'
#!/bin/sh
set -e
systemctl daemon-reload 2>/dev/null || true
exit 0
EOF
chmod 0755 "${package_root}/DEBIAN/postinst" "${package_root}/DEBIAN/prerm" "${package_root}/DEBIAN/postrm"

rm -f -- "${package_file}"
dpkg-deb --root-owner-group --build "${package_root}" "${package_file}"
printf 'Debian paketi hazır: %s\n' "${package_file}"
