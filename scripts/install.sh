#!/usr/bin/env bash
set -euo pipefail

if [[ ${EUID} -eq 0 ]]; then
  printf '%s\n' 'Bu betiği root olarak değil, sudo yetkili normal kullanıcı olarak çalıştırın.' >&2
  exit 1
fi

project_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
build_dir=${project_dir}/build

command -v cargo >/dev/null
command -v cmake >/dev/null
command -v ninja >/dev/null
command -v sudo >/dev/null

cargo build --manifest-path "${project_dir}/Cargo.toml" --release
cmake -S "${project_dir}/gui" -B "${build_dir}/gui" -G Ninja -DCMAKE_BUILD_TYPE=Release
cmake --build "${build_dir}/gui"

if [[ ! -x ${project_dir}/engine/nfqws ]]; then
  printf '%s\n' 'engine/nfqws bulunamadı. Önce Zapret v72.10 nfq/nfqws ikilisini bu konuma derleyin.' >&2
  exit 1
fi

sudo /usr/bin/install -Dm755 "${project_dir}/target/release/turkdpi-service" /usr/bin/turkdpi-service
sudo /usr/bin/install -Dm755 "${build_dir}/gui/turkdpi-gui" /usr/bin/turkdpi-gui
sudo /usr/bin/install -Dm755 "${project_dir}/engine/nfqws" /usr/lib/turkdpi/nfqws
sudo /usr/bin/install -Dm644 "${project_dir}/gui/turkdpi.desktop" /usr/share/applications/turkdpi.desktop
sudo /usr/bin/install -Dm644 "${project_dir}/systemd/turkdpi.service" /usr/lib/systemd/system/turkdpi.service
sudo /usr/bin/install -Dm644 "${project_dir}/polkit/org.turkdpi.manage.policy" /usr/share/polkit-1/actions/org.turkdpi.manage.policy
sudo /usr/bin/install -Dm755 "${project_dir}/scripts/90-turkdpi" /etc/NetworkManager/dispatcher.d/90-turkdpi
sudo /usr/bin/install -Dm755 "${project_dir}/scripts/turkdpi-sleep" /usr/lib/systemd/system-sleep/turkdpi
for profile in safe balanced discord aggressive; do
  sudo /usr/bin/install -Dm644 "${project_dir}/profiles/${profile}.toml" "/usr/share/turkdpi/profiles/${profile}.toml"
done
sudo /usr/bin/install -Dm644 "${project_dir}/profiles/discord-hosts.txt" /usr/share/turkdpi/discord-hosts.txt
sudo /usr/bin/install -Dm644 "${project_dir}/README.md" /usr/share/doc/turkdpi/README.md
sudo /usr/bin/install -Dm644 "${project_dir}/LICENSE" /usr/share/licenses/turkdpi/LICENSE
sudo /usr/bin/systemctl daemon-reload
printf '%s\n' 'Kurulum tamamlandı. turkdpi-gui komutuyla başlatın.'
