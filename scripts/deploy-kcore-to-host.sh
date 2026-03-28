#!/usr/bin/env bash
# Copy freshly built kcore-dashboard and kcore-controller closures to a NixOS host via nix copy,
# pin systemd units to those store paths, daemon-reload, and restart.
#
# Usage:
#   ./scripts/deploy-kcore-to-host.sh root@192.168.40.105
#   FLAKE=/path/to/kcore ./scripts/deploy-kcore-to-host.sh user@host
#
# Requires: passwordless sudo on the remote for systemctl/te mkdir, or run as root@host.
set -euo pipefail

TARGET="${1:?usage: $0 [user@]host}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
FLAKE="${FLAKE:-$ROOT}"

echo "==> Building kcore-dashboard and kcore-controller from $FLAKE"
DASH="$(nix build "$FLAKE#kcore-dashboard" --no-link --print-out-paths)"
CTRL="$(nix build "$FLAKE#kcore-controller" --no-link --print-out-paths)"
echo "    dashboard: $DASH"
echo "    controller: $CTRL"

echo "==> nix copy (closures) -> ssh://$TARGET"
nix copy --to "ssh://$TARGET" "$DASH" "$CTRL"

echo "==> Remote: systemd drop-ins + restart"
ssh "$TARGET" bash -s -- "$DASH" "$CTRL" <<'REMOTE'
set -euo pipefail
DASH="$1"
CTRL="$2"
SUDO=""
if [[ "$(id -u)" -ne 0 ]]; then
  SUDO="sudo"
fi
restart_unit() {
  local name="$1" exe="$2"
  if ! systemctl cat "$name" &>/dev/null; then
    echo "    (skip $name: unit not found)"
    return 0
  fi
  $SUDO mkdir -p "/etc/systemd/system/${name}.service.d"
  $SUDO tee "/etc/systemd/system/${name}.service.d/z-nix-store-override.conf" >/dev/null <<EOF
[Service]
ExecStart=
ExecStart=$exe
EOF
  echo "    override $name -> $exe"
}
restart_unit kcore-dashboard "$DASH/bin/kcore-dashboard"
restart_unit kcore-controller "$CTRL/bin/kcore-controller"
$SUDO systemctl daemon-reload
for u in kcore-dashboard kcore-controller; do
  if systemctl cat "$u" &>/dev/null; then
    $SUDO systemctl restart "$u"
    echo "    restarted $u"
  fi
done
echo "==> status (first lines)"
for u in kcore-dashboard kcore-controller; do
  if systemctl cat "$u" &>/dev/null; then
    $SUDO systemctl --no-pager -l status "$u" | head -12 || true
    echo ""
  fi
done
REMOTE

HOST_ONLY="${TARGET#*@}"
echo "Done. Open http://${HOST_ONLY}:8080/storage (or your LEPTOS_SITE_ADDR) and hard-refresh (Ctrl+Shift+R)."
