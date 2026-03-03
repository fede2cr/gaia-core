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
UDEV_VIDEO_SRC="${SCRIPT_DIR}/host/udev/99-gaia-video.rules"
UDEV_VIDEO_DST="/etc/udev/rules.d/99-gaia-video.rules"

if [[ -f "$UDEV_VIDEO_DST" ]] && cmp -s "$UDEV_VIDEO_SRC" "$UDEV_VIDEO_DST"; then
  info "Video udev rule already installed and up to date."
else
  cp "$UDEV_VIDEO_SRC" "$UDEV_VIDEO_DST"
  UDEV_CHANGED=1
  info "Installed udev rule: $UDEV_VIDEO_DST"
  info "Video devices (/dev/video*) are now world-accessible (mode 0666)."
fi

# ALSA audio udev rule
UDEV_AUDIO_SRC="${SCRIPT_DIR}/host/udev/99-gaia-audio.rules"
UDEV_AUDIO_DST="/etc/udev/rules.d/99-gaia-audio.rules"

if [[ -f "$UDEV_AUDIO_DST" ]] && cmp -s "$UDEV_AUDIO_SRC" "$UDEV_AUDIO_DST"; then
  info "Audio udev rule already installed and up to date."
else
  cp "$UDEV_AUDIO_SRC" "$UDEV_AUDIO_DST"
  UDEV_CHANGED=1
  info "Installed udev rule: $UDEV_AUDIO_DST"
  info "Audio devices (/dev/snd/*) are now world-accessible (mode 0666)."
fi

# Reload udev if any rules changed
if [[ "${UDEV_CHANGED:-0}" -eq 1 ]]; then
  udevadm control --reload-rules
  udevadm trigger
  info "udev rules reloaded."
fi

info "Host setup complete."