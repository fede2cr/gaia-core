#!/bin/bash
set -e

# ── mDNS / D-Bus setup ──────────────────────────────────────────────────
# We always start our own D-Bus system bus and avahi-daemon inside the
# container.  Mounting the host's /run/dbus doesn't work with rootless
# Podman because the UID mapping prevents access to root-owned sockets.
#
# With network_mode: host the container shares the host's network
# namespace, so avahi-daemon can see mDNS multicast traffic on the LAN.
# If the host also runs avahi-daemon, both coexist on UDP 5353 via
# SO_REUSEADDR + multicast.

echo "[gaia-core] Starting D-Bus system bus"
mkdir -p /run/dbus
rm -f /run/dbus/pid /run/dbus/system_bus_socket
dbus-daemon --system --nofork &
DBUS_PID=$!
sleep 1

# Verify D-Bus is alive before proceeding.
if ! kill -0 "$DBUS_PID" 2>/dev/null; then
    echo "[gaia-core] ERROR: dbus-daemon failed to start"
    exit 1
fi
echo "[gaia-core] D-Bus started (pid $DBUS_PID) ✓"

echo "[gaia-core] Starting avahi-daemon"
pkill avahi-daemon 2>/dev/null || true
sleep 0.3
avahi-daemon --daemonize --no-chroot 2>/dev/null || true
sleep 1

if timeout 3 avahi-browse -a -t -p >/dev/null 2>&1; then
    echo "[gaia-core] avahi-daemon reachable ✓"
else
    echo "[gaia-core] WARNING: avahi-browse cannot reach avahi-daemon"
    echo "[gaia-core] mDNS discovery may not work"
fi

# Launch the Gaia Core server.
exec /app/gaia-core "$@"
