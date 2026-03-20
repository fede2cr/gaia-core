#!/usr/bin/env bash
# ── Gaia Daily Backup ─────────────────────────────────────────────────
#
# Backs up ALL persistent data across all Gaia projects:
#   - Databases (SQLite, with safe WAL checkpoint)
#   - Audio recordings + extracted clips (gaia-audio)
#   - Images, video segments + crops (gaia-light)
#   - CO₂ / flight tracking state (gaia-radio)
#   - Meteor observations (GMN / RMS)
#   - Gaia Core config + assignments
#
# Usage:
#   ./backup.sh                    # one-shot backup
#   ./backup.sh --install-cron     # install daily 3 AM cron job
#   ./backup.sh --restore <file>   # restore from a backup archive
#
# The backup is a compressed tar archive stored in BACKUP_DIR (default:
# ~/gaia-backups/).  Old backups beyond KEEP_DAYS are automatically
# pruned.
#
# Databases are backed up using `sqlite3 .backup` which safely handles
# WAL mode — it checkpoints pending WAL data and produces a clean copy
# without locking the live database.

set -euo pipefail

# ── Configuration ─────────────────────────────────────────────────────

BACKUP_DIR="${GAIA_BACKUP_DIR:-$HOME/gaia-backups}"
KEEP_DAYS="${GAIA_BACKUP_KEEP_DAYS:-30}"
CONTAINER_RUNTIME="${CONTAINER_RUNTIME:-podman}"
DATE=$(date +%Y-%m-%d_%H%M%S)
HOSTNAME=$(hostname -s)
ARCHIVE_NAME="gaia-backup_${HOSTNAME}_${DATE}.tar.zst"
TMPDIR=$(mktemp -d -t gaia-backup.XXXXXX)

# Volume → mount-point mapping (matches compose.yaml)
declare -A VOLUMES=(
    [gaia-core-data]="/app/data"
    [gaia-audio-data]="/data"
    [gaia-light-data]="/data"
    [gaia-gmn-data]="/data"
    [co2-state]="/var/lib/co2tracker"
    [rms-data]="/home/rms/RMS_data"
)

# Databases inside each volume (relative to mount point)
declare -A DATABASES=(
    [gaia-core-data]="gaia-core.db"
    [gaia-audio-data]="detections.db"
    [gaia-light-data]="detections.db light.db"
    [co2-state]="co2.db"
)

cleanup() {
    rm -rf "$TMPDIR"
}
trap cleanup EXIT

log() {
    echo "[$(date '+%H:%M:%S')] $*"
}

err() {
    echo "[$(date '+%H:%M:%S')] ERROR: $*" >&2
}

# ── Cron installation ─────────────────────────────────────────────────

if [[ "${1:-}" == "--install-cron" ]]; then
    SCRIPT_PATH=$(readlink -f "$0")
    CRON_LINE="0 3 * * * $SCRIPT_PATH >> $BACKUP_DIR/backup.log 2>&1"
    if crontab -l 2>/dev/null | grep -qF "$SCRIPT_PATH"; then
        echo "Cron job already installed."
    else
        (crontab -l 2>/dev/null; echo "$CRON_LINE") | crontab -
        echo "Installed daily backup cron job (3:00 AM):"
        echo "  $CRON_LINE"
    fi
    exit 0
fi

# ── Restore ───────────────────────────────────────────────────────────

if [[ "${1:-}" == "--restore" ]]; then
    RESTORE_FILE="${2:-}"
    if [[ -z "$RESTORE_FILE" || ! -f "$RESTORE_FILE" ]]; then
        echo "Usage: $0 --restore <backup-archive.tar.zst>"
        exit 1
    fi
    echo "=== Gaia Restore ==="
    echo "Archive: $RESTORE_FILE"
    echo ""
    echo "This will extract backup data into $TMPDIR for manual review."
    echo "Databases and media are NOT automatically overwritten."
    echo ""
    echo "Contents:"
    zstd -d < "$RESTORE_FILE" | tar tf - | head -40
    echo "..."
    echo ""
    echo "To extract fully: zstd -d < $RESTORE_FILE | tar xf - -C /tmp/gaia-restore"
    exit 0
fi

# ── Pre-flight checks ────────────────────────────────────────────────

mkdir -p "$BACKUP_DIR"

if ! command -v zstd &>/dev/null; then
    err "zstd not found. Install with: sudo apt install zstd"
    exit 1
fi

if ! command -v sqlite3 &>/dev/null; then
    log "sqlite3 not found — database backups will use file copy (less safe)"
    HAS_SQLITE3=false
else
    HAS_SQLITE3=true
fi

log "=== Gaia Daily Backup ==="
log "Backup dir:  $BACKUP_DIR"
log "Archive:     $ARCHIVE_NAME"
log "Retention:   $KEEP_DAYS days"
log "Temp dir:    $TMPDIR"
echo ""

# ── Helper: resolve volume path on host ───────────────────────────────

# Podman/Docker named volumes are stored at a predictable location.
# We try `volume inspect` first, then fall back to common paths.
volume_path() {
    local vol="$1"
    local mp
    mp=$($CONTAINER_RUNTIME volume inspect "$vol" --format '{{.Mountpoint}}' 2>/dev/null) || true
    if [[ -n "$mp" && -d "$mp" ]]; then
        echo "$mp"
        return 0
    fi
    return 1
}

# ── Helper: safe database backup ──────────────────────────────────────

backup_db() {
    local src="$1"
    local dst="$2"

    if [[ ! -f "$src" ]]; then
        return 1
    fi

    if $HAS_SQLITE3; then
        # sqlite3 .backup safely checkpoints WAL and creates a clean copy.
        sqlite3 "$src" ".backup '$dst'" 2>/dev/null
    else
        cp "$src" "$dst"
        # Also copy WAL/SHM if present
        [[ -f "${src}-wal" ]] && cp "${src}-wal" "${dst}-wal"
        [[ -f "${src}-shm" ]] && cp "${src}-shm" "${dst}-shm"
    fi
}

# ── Phase 1: Database backups ─────────────────────────────────────────

log "Phase 1: Backing up databases..."

DB_COUNT=0
for vol in "${!DATABASES[@]}"; do
    vol_dir=$(volume_path "$vol") || continue
    for db_name in ${DATABASES[$vol]}; do
        src="$vol_dir/$db_name"
        if [[ -f "$src" ]]; then
            dest_dir="$TMPDIR/databases/$vol"
            mkdir -p "$dest_dir"
            if backup_db "$src" "$dest_dir/$db_name"; then
                size=$(du -sh "$dest_dir/$db_name" | cut -f1)
                log "  ✓ $vol/$db_name ($size)"
                DB_COUNT=$((DB_COUNT + 1))
            else
                err "  ✗ Failed to backup $vol/$db_name"
            fi
        fi
    done
done
log "  $DB_COUNT database(s) backed up"

# ── Phase 2: Media & recordings ───────────────────────────────────────

log "Phase 2: Backing up media files..."

MEDIA_COUNT=0

# gaia-audio: recordings + extracted clips + spectrograms
audio_dir=$(volume_path "gaia-audio-data") || true
if [[ -n "$audio_dir" && -d "$audio_dir" ]]; then
    dest="$TMPDIR/media/gaia-audio"
    mkdir -p "$dest"

    # StreamData (raw recordings) — large, use hardlinks when possible
    if [[ -d "$audio_dir/StreamData" ]]; then
        cp -al "$audio_dir/StreamData" "$dest/StreamData" 2>/dev/null || \
        cp -a  "$audio_dir/StreamData" "$dest/StreamData"
        count=$(find "$dest/StreamData" -type f | wc -l)
        log "  ✓ gaia-audio StreamData ($count files)"
        MEDIA_COUNT=$((MEDIA_COUNT + count))
    fi

    # Extracted (clips + spectrograms)
    for subdir in Extracted extracted By_Date; do
        if [[ -d "$audio_dir/$subdir" ]]; then
            cp -al "$audio_dir/$subdir" "$dest/$subdir" 2>/dev/null || \
            cp -a  "$audio_dir/$subdir" "$dest/$subdir"
            count=$(find "$dest/$subdir" -type f | wc -l)
            log "  ✓ gaia-audio $subdir ($count files)"
            MEDIA_COUNT=$((MEDIA_COUNT + count))
        fi
    done
fi

# gaia-light: video segments + crops
light_dir=$(volume_path "gaia-light-data") || true
if [[ -n "$light_dir" && -d "$light_dir" ]]; then
    dest="$TMPDIR/media/gaia-light"
    mkdir -p "$dest"

    for subdir in StreamData Extracted processed; do
        if [[ -d "$light_dir/$subdir" ]]; then
            cp -al "$light_dir/$subdir" "$dest/$subdir" 2>/dev/null || \
            cp -a  "$light_dir/$subdir" "$dest/$subdir"
            count=$(find "$dest/$subdir" -type f | wc -l)
            log "  ✓ gaia-light $subdir ($count files)"
            MEDIA_COUNT=$((MEDIA_COUNT + count))
        fi
    done
fi

# gaia-radio: CO₂ state files
co2_dir=$(volume_path "co2-state") || true
if [[ -n "$co2_dir" && -d "$co2_dir" ]]; then
    dest="$TMPDIR/media/gaia-radio"
    mkdir -p "$dest"
    cp -a "$co2_dir/"* "$dest/" 2>/dev/null || true
    count=$(find "$dest" -type f | wc -l)
    log "  ✓ gaia-radio CO₂ state ($count files)"
    MEDIA_COUNT=$((MEDIA_COUNT + count))
fi

# GMN data
gmn_dir=$(volume_path "gaia-gmn-data") || true
if [[ -n "$gmn_dir" && -d "$gmn_dir" ]]; then
    dest="$TMPDIR/media/gaia-gmn"
    cp -al "$gmn_dir" "$dest" 2>/dev/null || \
    cp -a  "$gmn_dir" "$dest"
    count=$(find "$dest" -type f | wc -l)
    log "  ✓ gaia-gmn data ($count files)"
    MEDIA_COUNT=$((MEDIA_COUNT + count))
fi

# RMS data
rms_dir=$(volume_path "rms-data") || true
if [[ -n "$rms_dir" && -d "$rms_dir" ]]; then
    dest="$TMPDIR/media/rms"
    cp -al "$rms_dir" "$dest" 2>/dev/null || \
    cp -a  "$rms_dir" "$dest"
    count=$(find "$dest" -type f | wc -l)
    log "  ✓ RMS data ($count files)"
    MEDIA_COUNT=$((MEDIA_COUNT + count))
fi

log "  $MEDIA_COUNT media file(s) backed up"

# ── Phase 3: Config files ────────────────────────────────────────────

log "Phase 3: Backing up configuration..."

config_dir="$TMPDIR/config"
mkdir -p "$config_dir"

# gaia-core config
core_dir=$(volume_path "gaia-core-data") || true
if [[ -n "$core_dir" && -d "$core_dir" ]]; then
    cp -a "$core_dir/"*.json "$config_dir/" 2>/dev/null || true
    cp -a "$core_dir/"*.toml "$config_dir/" 2>/dev/null || true
fi

# Copy compose.yaml for reference
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
[[ -f "$SCRIPT_DIR/compose.yaml" ]] && cp "$SCRIPT_DIR/compose.yaml" "$config_dir/"
[[ -f "$SCRIPT_DIR/containers.toml" ]] && cp "$SCRIPT_DIR/containers.toml" "$config_dir/"

log "  ✓ Config files saved"

# ── Phase 4: Create compressed archive ───────────────────────────────

log "Phase 4: Creating archive..."

tar -C "$TMPDIR" -cf - . | zstd -T0 -3 > "$BACKUP_DIR/$ARCHIVE_NAME"

archive_size=$(du -sh "$BACKUP_DIR/$ARCHIVE_NAME" | cut -f1)
log "  ✓ $BACKUP_DIR/$ARCHIVE_NAME ($archive_size)"

# ── Phase 5: Prune old backups ────────────────────────────────────────

log "Phase 5: Pruning backups older than $KEEP_DAYS days..."

pruned=0
while IFS= read -r -d '' old_file; do
    rm -f "$old_file"
    pruned=$((pruned + 1))
done < <(find "$BACKUP_DIR" -name "gaia-backup_*.tar.zst" -mtime +"$KEEP_DAYS" -print0)

if [[ $pruned -gt 0 ]]; then
    log "  Removed $pruned old backup(s)"
fi

# ── Summary ───────────────────────────────────────────────────────────

echo ""
log "=== Backup Complete ==="
log "  Databases: $DB_COUNT"
log "  Media files: $MEDIA_COUNT"
log "  Archive: $BACKUP_DIR/$ARCHIVE_NAME ($archive_size)"

# Show remaining backups
remaining=$(find "$BACKUP_DIR" -name "gaia-backup_*.tar.zst" | wc -l)
total_size=$(du -sh "$BACKUP_DIR" | cut -f1)
log "  Total backups: $remaining ($total_size)"
