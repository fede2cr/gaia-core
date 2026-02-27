#!/bin/bash
set -e

READSB_DEVICE_INDEX="${READSB_DEVICE_INDEX:-0}"
READSB_NET_BO_PORT="${READSB_NET_BO_PORT:-30005}"
READSB_EXTRA_ARGS="${READSB_EXTRA_ARGS:-}"
SERVICE_TYPE="_adsbbeast._tcp"
BASE_NAME="gaia-radio-capture"

# ---------- mDNS helpers ----------
# Capture runs on standalone hardware, so it needs its own avahi-daemon.

start_mdns() {
    # Start dbus (required by avahi)
    if [ ! -d /run/dbus ]; then
        mkdir -p /run/dbus
    fi
    rm -f /run/dbus/pid /run/dbus/system_bus_socket
    dbus-daemon --system --nofork &
    sleep 1

    # Start avahi-daemon
    avahi-daemon --daemonize --no-chroot 2>/dev/null || true
    sleep 1
}

find_next_instance() {
    local service_type="$1"
    local base_name="$2"
    local max_num=0

    # Browse the network for a few seconds to collect existing instances
    local raw
    raw=$(timeout 5 avahi-browse -p -r "$service_type" 2>/dev/null || true)

    while IFS=';' read -r event iface protocol name stype domain hostname addr port txt; do
        if [[ "$event" == "=" && "$name" =~ ^${base_name}-([0-9]+)$ ]]; then
            local num="${BASH_REMATCH[1]}"
            num=$((10#$num))
            if (( num > max_num )); then
                max_num=$num
            fi
        fi
    done <<< "$raw"

    printf "%02d" $((max_num + 1))
}

announce_service() {
    local instance_name="$1"
    local service_type="$2"
    local port="$3"

    echo "[mdns] Announcing: ${instance_name} (${service_type}) on port ${port}"
    avahi-publish-service \
        "${instance_name}" \
        "${service_type}" \
        "${port}" \
        "version=1" &
}

# ---------- Main ----------

echo "[capture] Starting mDNS subsystem..."
start_mdns

echo "[capture] Discovering existing capture instances..."
INSTANCE_NUM=$(find_next_instance "$SERVICE_TYPE" "$BASE_NAME")
INSTANCE_NAME="${BASE_NAME}-${INSTANCE_NUM}"
echo "[capture] This instance will be: ${INSTANCE_NAME}"

announce_service "$INSTANCE_NAME" "$SERVICE_TYPE" "$READSB_NET_BO_PORT"

echo "[capture] Starting readsb (device index ${READSB_DEVICE_INDEX}, Beast output on port ${READSB_NET_BO_PORT})..."
exec /usr/local/bin/readsb \
    --device-type rtlsdr \
    --device "$READSB_DEVICE_INDEX" \
    --metric \
    --net \
    --net-bo-port "$READSB_NET_BO_PORT" \
    --write-json /run/readsb \
    --write-json-every 1 \
    --json-location-accuracy 2 \
    $READSB_EXTRA_ARGS
