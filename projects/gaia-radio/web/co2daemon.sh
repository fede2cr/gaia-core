#!/bin/bash
# co2daemon.sh — Server-side CO₂ tracking daemon for Gaia Radio
#
# Reads /run/readsb/aircraft.json continuously and computes Haversine
# distances between consecutive position reports.  Estimates CO₂ based
# on ADS-B emitter category.  Publishes running totals as co2data.json
# so the web UI always has accurate all-time figures, even when no
# browser is open.
#
# Emission factors: fuel burn (kg/km) × 3.16 (IPCC kerosene CO₂ factor).
# See CO2_METHODOLOGY.md for full derivation and sources.
#
# Persistent state in /var/lib/co2tracker/ — mount a volume there for
# data that survives container restarts.

set -uo pipefail

AIRCRAFT_JSON="/run/readsb/aircraft.json"
STATE_DIR="/var/lib/co2tracker"
POS_FILE="$STATE_DIR/positions.tsv"
TOTALS_FILE="$STATE_DIR/totals.dat"
SEEN_FILE="$STATE_DIR/seen.hex"
SINCE_FILE="$STATE_DIR/since.dat"
OUTPUT_FILE="/var/www/html/tar1090/co2data.json"
INTERVAL="${CO2_INTERVAL:-5}"

mkdir -p "$STATE_DIR"

# Initialise persistent files on first ever run
[ -f "$SINCE_FILE" ]  || date -u +%Y-%m-%dT%H:%M:%SZ > "$SINCE_FILE"
[ -f "$TOTALS_FILE" ] || printf '0\n0\n' > "$TOTALS_FILE"
[ -f "$SEEN_FILE" ]   || : > "$SEEN_FILE"
[ -f "$POS_FILE" ]    || : > "$POS_FILE"

SINCE=$(cat "$SINCE_FILE")

echo "[co2daemon] CO₂ tracker daemon started (interval ${INTERVAL}s, since $SINCE)"

while true; do
    sleep "$INTERVAL"

    # Wait for readsb data
    [ -f "$AIRCRAFT_JSON" ] || continue

    # Extract aircraft with a recent position: hex<TAB>lat<TAB>lon<TAB>category
    CURRENT=$(jq -r '
        .aircraft[]?
        | select(.lat != null and .lon != null and ((.seen_pos // 999) < 120))
        | [.hex, (.lat|tostring), (.lon|tostring), (.category // "")]
        | join("\t")
    ' "$AIRCRAFT_JSON" 2>/dev/null) || continue

    [ -n "$CURRENT" ] || continue

    # Read accumulated totals
    { read -r T_CO2 || T_CO2=0; read -r T_DIST || T_DIST=0; } < "$TOTALS_FILE"

    # ──────── awk: compute deltas and update position / seen state ────────
    DELTAS=$(echo "$CURRENT" | awk \
        -v posFile="$POS_FILE" \
        -v posOut="${POS_FILE}.new" \
        -v seenFile="$SEEN_FILE" \
    'BEGIN {
        FS = OFS = "\t"
        PI = 3.14159265358979; R = 6371.0
        maxJ = 50; minM = 0.01

        # Emission factors by ADS-B emitter category (kg CO₂/km)
        # Derived from fleet-average fuel burn × 3.16 (IPCC kerosene factor).
        # See CO2_METHODOLOGY.md §6.2 for rationale.
        ef["A1"]=1.2;  ef["A2"]=3.5;  ef["A3"]=9.0;  ef["A4"]=13.9
        ef["A5"]=22.0; ef["A6"]=22.0; ef["A7"]=0.5
        ef["B1"]=0.0;  ef["B2"]=0.1;  ef["B4"]=0.0;  ef["B6"]=0.1
        ef["C1"]=0.0;  ef["C3"]=0.0
        defEf = 7.0

        # Load previous positions
        while ((getline < posFile) > 0) { plat[$1]=$2+0; plon[$1]=$3+0 }
        close(posFile)

        # Load set of previously-seen hexes
        while ((getline < seenFile) > 0) { seen[$1]=1 }
        close(seenFile)

        dCO2=0; dDist=0; nNew=0
    }

    function hav(la1,lo1,la2,lo2,  dLa,dLo,a) {
        dLa=(la2-la1)*PI/180; dLo=(lo2-lo1)*PI/180
        a=sin(dLa/2)^2 + cos(la1*PI/180)*cos(la2*PI/180)*sin(dLo/2)^2
        return R*2*atan2(sqrt(a),sqrt(1-a))
    }

    {
        h=$1; la=$2+0; lo=$3+0; cat=$4

        # New aircraft?
        if (!(h in seen)) { nNew++; seen[h]=1; print h >> seenFile }

        # Distance from last known position
        if (h in plat) {
            d = hav(plat[h], plon[h], la, lo)
            if (d > minM && d < maxJ) {
                f = (cat != "" && cat in ef) ? ef[cat] : defEf
                dCO2 += d*f;  dDist += d
            }
        }

        # Save new position
        print h, la, lo > posOut
    }

    END {
        close(posOut); close(seenFile)
        printf "%.6f %.6f %d\n", dCO2, dDist, nNew
    }') || continue

    [ -n "$DELTAS" ] || continue

    # Swap position file atomically
    mv -f "${POS_FILE}.new" "$POS_FILE" 2>/dev/null || true

    # Parse delta line
    read -r D_CO2 D_DIST D_NEW <<< "$DELTAS"

    # Update accumulated totals (floating-point add via awk)
    NEW_CO2=$(awk  "BEGIN{printf \"%.6f\", $T_CO2 + $D_CO2}")
    NEW_DIST=$(awk "BEGIN{printf \"%.6f\", $T_DIST + $D_DIST}")
    SEEN_COUNT=$(wc -l < "$SEEN_FILE")

    printf '%s\n%s\n' "$NEW_CO2" "$NEW_DIST" > "$TOTALS_FILE"

    # Publish JSON for the web UI (atomic write via rename)
    NOW=$(date -u +%Y-%m-%dT%H:%M:%SZ)
    printf '{"co2Kg":%s,"distKm":%s,"count":%d,"since":"%s","updated":"%s"}\n' \
        "$NEW_CO2" "$NEW_DIST" "$SEEN_COUNT" "$SINCE" "$NOW" > "${OUTPUT_FILE}.tmp"
    mv -f "${OUTPUT_FILE}.tmp" "$OUTPUT_FILE"
done
