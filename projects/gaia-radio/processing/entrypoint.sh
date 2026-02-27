#!/bin/bash
set -e

CAPTURE_SERVICE_TYPE="_adsbbeast._tcp"
ANNOUNCE_SERVICE_TYPE="_readsb._tcp"
BASE_NAME="gaia-radio-processing"

DISCOVERY_TIMEOUT="${DISCOVERY_TIMEOUT:-30}"
DISCOVERY_RETRY_INTERVAL="${DISCOVERY_RETRY_INTERVAL:-5}"

# ---------- mDNS helpers ----------
# Processing runs its own avahi-daemon so it can discover capture
# services on the LAN and announce itself.

start_mdns() {
    if [ ! -d /run/dbus ]; then
        mkdir -p /run/dbus
    fi
    rm -f /run/dbus/pid /run/dbus/system_bus_socket
    dbus-daemon --system --nofork &
    sleep 1

    avahi-daemon --daemonize --no-chroot 2>/dev/null || true
    sleep 1
    echo "[mdns] avahi-daemon started"
}

find_next_instance() {
    local service_type="$1"
    local base_name="$2"
    local max_num=0

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

# Discover the first available instance of a service type.
# Returns "host port" (space-separated) on stdout, or exits if not found within timeout.
# Prefers IPv4 addresses over IPv6 to avoid colon-parsing issues.
discover_service() {
    local service_type="$1"
    local deadline=$((SECONDS + DISCOVERY_TIMEOUT))

    echo "[mdns] Waiting for upstream service ${service_type}..." >&2

    # Quick sanity check: can we talk to avahi at all?
    echo "[mdns] D-Bus sanity check:" >&2
    local check_out
    check_out=$(timeout 3 avahi-browse -a -t -p 2>&1 || true)
    if [ -z "$check_out" ]; then
        echo "[mdns] WARNING: avahi-browse returned nothing. Is avahi-daemon running on the host?" >&2
        echo "[mdns] Check: systemctl status avahi-daemon" >&2
        echo "[mdns] Check: ls -la /var/run/dbus/system_bus_socket" >&2
    else
        echo "[mdns] avahi-browse is working (got $(echo "$check_out" | wc -l) lines)" >&2
    fi

    while (( SECONDS < deadline )); do
        local tmpfile
        tmpfile=$(mktemp)

        # Capture raw output for debugging; stderr goes to container logs
        timeout "$DISCOVERY_RETRY_INTERVAL" avahi-browse -p -r "$service_type" >"$tmpfile" 2>&2 || true

        local line_count
        line_count=$(wc -l < "$tmpfile")
        if (( line_count > 0 )); then
            echo "[mdns] avahi-browse returned ${line_count} lines for ${service_type}:" >&2
            head -5 "$tmpfile" >&2
        fi

        local ipv4_addr="" ipv4_port="" ipv4_name=""
        local ipv6_addr="" ipv6_port="" ipv6_name=""

        while IFS=';' read -r event iface protocol name stype domain hostname addr port txt; do
            if [[ "$event" == "=" && -n "$addr" && -n "$port" ]]; then
                if [[ "$protocol" == "IPv4" && -z "$ipv4_addr" ]]; then
                    ipv4_addr="$addr"
                    ipv4_port="$port"
                    ipv4_name="$name"
                elif [[ "$protocol" == "IPv6" && -z "$ipv6_addr" ]]; then
                    ipv6_addr="$addr"
                    ipv6_port="$port"
                    ipv6_name="$name"
                fi
            fi
        done < "$tmpfile"

        rm -f "$tmpfile"

        if [[ -n "$ipv4_addr" ]]; then
            echo "[mdns] Found (IPv4): ${ipv4_name} at ${ipv4_addr}:${ipv4_port}" >&2
            echo "${ipv4_addr} ${ipv4_port}"
            return 0
        elif [[ -n "$ipv6_addr" ]]; then
            echo "[mdns] Found (IPv6): ${ipv6_name} at [${ipv6_addr}]:${ipv6_port}" >&2
            echo "${ipv6_addr} ${ipv6_port}"
            return 0
        fi

        echo "[mdns] No ${service_type} found yet, sleeping ${DISCOVERY_RETRY_INTERVAL}s before retry..." >&2
        sleep "$DISCOVERY_RETRY_INTERVAL"
    done

    echo "[mdns] ERROR: Could not find ${service_type} within ${DISCOVERY_TIMEOUT}s" >&2
    return 1
}

# ---------- Main ----------

echo "[processing] Starting mDNS subsystem..."
start_mdns

echo "[processing] Discovering capture service (${CAPTURE_SERVICE_TYPE})..."
read -r CAPTURE_HOST CAPTURE_PORT <<< "$(discover_service "$CAPTURE_SERVICE_TYPE")"
echo "[processing] Will connect to capture Beast output at ${CAPTURE_HOST}:${CAPTURE_PORT}"

echo "[processing] Discovering existing processing instances..."
INSTANCE_NUM=$(find_next_instance "$ANNOUNCE_SERVICE_TYPE" "$BASE_NAME")
INSTANCE_NAME="${BASE_NAME}-${INSTANCE_NUM}"
echo "[processing] This instance will be: ${INSTANCE_NAME}"

announce_service "$INSTANCE_NAME" "$ANNOUNCE_SERVICE_TYPE" "8181"

BEAST_SOURCE="${CAPTURE_HOST}:${CAPTURE_PORT}"
RS1090_OUTPUT_DIR="${RS1090_OUTPUT_DIR:-/run/readsb}"
RS1090_DB_PATH="${RS1090_DB_PATH:-/var/lib/co2tracker/co2.db}"
RS1090_LISTEN="${RS1090_LISTEN:-0.0.0.0:8181}"

echo "[processing] Starting rs1090 (beast_source=${BEAST_SOURCE})..."
exec /usr/local/bin/rs1090 \
    --beast-source "${BEAST_SOURCE}" \
    --output-dir "${RS1090_OUTPUT_DIR}" \
    --db-path "${RS1090_DB_PATH}" \
    --listen "${RS1090_LISTEN}" \
    ${RECEIVER_LAT:+--receiver-lat "$RECEIVER_LAT"} \
    ${RECEIVER_LON:+--receiver-lon "$RECEIVER_LON"}
