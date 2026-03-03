#!/usr/bin/env bash
# Gaia - Host setup script
#
# Applies host-level configuration required by Gaia containers that cannot
# be done from inside a rootless container.  Run once on each host that will
# use camera devices (e.g. for GMN pre-alignment).
#
# Usage:
#   sudo bash setup-host.sh
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Colours (disabled when not a terminal)
if [[ -t 1 ]]; then
  GREEN='\033[0;32m'; YELLOW='\033[1;33m'; NC='\033[0m'
else
  GREEN=''; YELLOW=''; NC=''
fi

info() { echo -e "${GREEN}[+]${NC} $*"; }
warn() { echo -e "${YELLOW}[!]${NC} $*"; }

# Root check
if [[ $EUID -ne 0 ]]; then
  warn "This script must be run as root (sudo)."
  exit 1
fi

# V4L2 camera udev rule
UDEV_SRC="${SCRIPT_DIR}/host/udev/99-gaia-video.rules"
UDEV_DST="/etc/udev/rules.d/99-gaia-video.rules"

if [[ -f "$UDEV_DST" ]] && cmp -s "$UDEV_SRC" "$UDEV_DST"; then
  info "udev rule already installed and up to date."
else
  cp "$UDEV_SRC" "$UDEV_DST"
  udevadm control --reload-rules
  udevadm trigger
  info "Installed udev rule: $UDEV_DST"
  info "Video devices (/dev/video*) are now world-accessible (mode 0666)."
fi

info "Host setup complete."