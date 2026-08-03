#!/usr/bin/env bash
set -euo pipefail

if [[ ${EUID} -eq 0 ]]; then
  printf '%s\n' 'Bu betiği root olarak değil, sudo yetkili normal kullanıcı olarak çalıştırın.' >&2
  exit 1
fi

sudo /usr/bin/systemctl disable --now turkdpi.service 2>/dev/null || true
if [[ -x /usr/bin/turkdpi-service ]]; then
  if ! sudo /usr/bin/turkdpi-service cleanup; then
    printf '%s\n' 'Ağ kuralları veya DNS geri yüklenemedi; kurtarma için yardımcı ikili korunuyor.' >&2
    exit 1
  fi
fi
sudo /usr/bin/rm -f -- /usr/bin/turkdpi-service /usr/bin/turkdpi-gui
sudo /usr/bin/rm -f -- /usr/share/applications/turkdpi.desktop
sudo /usr/bin/rm -f -- /usr/lib/systemd/system/turkdpi.service
sudo /usr/bin/rm -f -- /usr/share/polkit-1/actions/org.turkdpi.manage.policy
sudo /usr/bin/rm -f -- /etc/NetworkManager/dispatcher.d/90-turkdpi
sudo /usr/bin/rm -f -- /usr/lib/systemd/system-sleep/turkdpi
sudo /usr/bin/rm -rf -- /usr/lib/turkdpi /usr/share/turkdpi /usr/share/doc/turkdpi
sudo /usr/bin/systemctl daemon-reload
printf '%s\n' 'TurkDPI kaldırıldı. /var/lib/turkdpi yedek ve ağ profilleri kurtarma amacıyla korundu.'
