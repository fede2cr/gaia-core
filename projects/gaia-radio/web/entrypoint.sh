#!/bin/bash
set -e

WEB_PORT="${WEB_PORT:-8080}"

# ---------- Main ----------

# Generate tar1090 config
cat > /var/www/html/tar1090/config.js <<JSEOF
// tar1090 configuration - auto-generated
defaultCenterLat = 0.0;
defaultCenterLon = 0.0;
defaultZoomLvl = 7;
DisplayUnits = "metric";
JSEOF

# /run/readsb is expected to be a shared volume mount from the processing
# container, where readsb writes aircraft.json, receiver.json, etc.
# lighttpd serves /data/ aliased to /run/readsb/.
if [ ! -d /run/readsb ]; then
    mkdir -p /run/readsb
fi

echo "[web] Waiting for readsb JSON data in /run/readsb/..."
WAIT_TIMEOUT="${WAIT_TIMEOUT:-60}"
elapsed=0
while [ ! -f /run/readsb/aircraft.json ]; do
    if (( elapsed >= WAIT_TIMEOUT )); then
        echo "[web] WARNING: /run/readsb/aircraft.json not found after ${WAIT_TIMEOUT}s."
        echo "[web] Make sure the processing container shares /run/readsb as a volume."
        break
    fi
    sleep 2
    elapsed=$((elapsed + 2))
done

if [ -f /run/readsb/aircraft.json ]; then
    echo "[web] Found readsb JSON data."
fi

# Start server-side CO₂ tracker daemon in background
echo "[web] Starting CO₂ tracker daemon..."
/co2daemon.sh &

echo "[web] Starting lighttpd on port ${WEB_PORT}..."
exec lighttpd -D -f /etc/lighttpd/lighttpd.conf
