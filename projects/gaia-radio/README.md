# gaia-radio

ADS-B aircraft tracking stack split into three independently deployable containers that discover each other over the local network using mDNS (Avahi).

## Architecture

```
┌─────────────────────┐      ┌─────────────────────────┐      ┌──────────────────────┐
│  gaia-radio-capture │      │  gaia-radio-processing  │      │   gaia-radio-web     │
│                     │      │                         │      │                      │
│  readsb (RTL-SDR)   │─────▶│  readsb (net-only)      │─────▶│  tar1090 + lighttpd  │
│  Decodes ADS-B on   │Beast │  Aggregates Beast feeds │      │  Web UI on port 8080 │
│  the capture device │      │  Writes JSON to         │      │                      │
│  Announces:         │      │   /run/readsb/          │      │  Reads JSON from     │
│ _adsbbeast._tcp     │      │  Discovers:             │      │   /run/readsb/       │
│  :30005             │      │   _adsbbeast._tcp       │      │  (shared volume)     │
│                     │      │  Announces:             │      │                      │
│                     │      │   _readsb._tcp          │      │                      │
└─────────────────────┘      └─────────────────────────┘      └──────────────────────┘
     (e.g. Raspberry Pi)          (any host)                       (any host)
```

Each container uses **mDNS service discovery** to find its upstream dependency — no hardcoded IPs or manual configuration needed. Multiple capture units are supported; each one registers itself with a sequential name (`gaia-radio-capture-01`, `-02`, etc.).

## Prerequisites

- Docker or Podman with compose support
- An RTL-SDR USB dongle (for capture)
- **avahi-daemon running on each host** (for processing and web containers that rely on the host's mDNS stack)
- The kernel DVB driver must be **blacklisted** on the capture host so it doesn't claim the RTL-SDR device

### Blacklist kernel DVB driver (capture host only)

```bash
echo 'blacklist dvb_usb_rtl28xxu
blacklist rtl2832
blacklist rtl2830' | sudo tee /etc/modprobe.d/blacklist-rtlsdr.conf
sudo rmmod dvb_usb_rtl28xxu rtl2832 rtl2830 2>/dev/null || true
```

Unplug/replug the dongle or reboot after this.

### Enable avahi-daemon (processing and web hosts)

```bash
sudo apt install avahi-daemon
sudo systemctl enable --now avahi-daemon
```

## Containers

### gaia-radio-capture

Runs `readsb` with `--device-type rtlsdr` to decode ADS-B data directly on the capture device and expose Beast output over the network. Runs its **own avahi-daemon** inside the container since it typically runs on standalone hardware (e.g. a Raspberry Pi).

**Must be run with `--network host`** so mDNS multicast traffic reaches the LAN.

```bash
# Build
docker build -t gaia-radio-capture ./capture

# Run
docker run -d \
    --name gaia-radio-capture \
    --network host \
    --privileged \
    --device /dev/bus/usb:/dev/bus/usb \
    -e READSB_DEVICE_INDEX=0 \
    -e READSB_NET_BO_PORT=30005 \
    gaia-radio-capture
```

| Environment Variable     | Default   | Description                          |
|--------------------------|-----------|--------------------------------------|
| `READSB_DEVICE_INDEX`    | `0`       | RTL-SDR device index                 |
| `READSB_NET_BO_PORT`     | `30005`   | Beast output port                    |
| `READSB_EXTRA_ARGS`      | (empty)   | Additional arguments for readsb      |

### gaia-radio-processing

Runs `readsb` in network-only mode. Discovers `gaia-radio-capture` over mDNS and connects to its Beast output using `--net-connector`. Announces itself as a `_readsb._tcp` service.

Runs its **own avahi-daemon** inside the container for mDNS discovery and announcement.

**Must be run with `--network host`** so mDNS multicast reaches the LAN and readsb network ports are reachable.

```bash
# Build
docker build -t gaia-radio-processing ./processing

# Run
docker run -d \
    --name gaia-radio-processing \
    --network host \
    -e READSB_NET_BO_PORT=30005 \
    -e READSB_NET_RO_PORT=30002 \
    -e READSB_NET_SBS_PORT=30003 \
    -e DISCOVERY_TIMEOUT=60 \
    gaia-radio-processing
```

| Environment Variable       | Default | Description                             |
|----------------------------|---------|-----------------------------------------|
| `READSB_NET_BO_PORT`      | `30005` | Beast output port                       |
| `READSB_NET_RO_PORT`      | `30002` | Raw output port                         |
| `READSB_NET_SBS_PORT`     | `30003` | SBS/BaseStation output port             |
| `READSB_NET_RI_PORT`      | `30001` | Raw input port                          |
| `READSB_EXTRA_ARGS`       | (empty) | Additional arguments for readsb         |
| `DISCOVERY_TIMEOUT`       | `30`    | Seconds to wait for capture service     |
| `DISCOVERY_RETRY_INTERVAL`| `5`     | Seconds between discovery retries       |

### gaia-radio-web

Runs `tar1090` served by lighttpd. Reads aircraft JSON data from `/run/readsb/`, which must be a **shared volume** with the processing container (where readsb writes its JSON output).

```bash
# Build
docker build -t gaia-radio-web ./web

# Run (must share /run/readsb with processing)
docker run -d \
    --name gaia-radio-web \
    --network host \
    -v readsb-json:/run/readsb:ro \
    -e WEB_PORT=8080 \
    gaia-radio-web
```

The tar1090 web UI is available at `http://<host>:8080`.

| Environment Variable       | Default     | Description                              |
|----------------------------|-------------|------------------------------------------|
| `WEB_PORT`                 | `8080`      | Port for the web UI                      |
| `WAIT_TIMEOUT`             | `60`        | Seconds to wait for JSON data at startup |

## Running with Docker Compose

The included `compose.yaml` runs the processing and web containers on a single host. The capture container is expected to run on separate hardware.

```bash
docker compose up --build
```

Or with Podman:

```bash
podman compose up --build
```

> **Note:** The compose file uses `network_mode: host` for all services. Processing uses mDNS to discover capture on the LAN. Processing and web share a `readsb-json` volume — readsb writes JSON to `/run/readsb/` and lighttpd serves it directly.

## Running capture on a Raspberry Pi

On the Pi:

```bash
# Build and run capture
docker build -t gaia-radio-capture ./capture
docker run -d \
    --name gaia-radio-capture \
    --network host \
    --privileged \
    --device /dev/bus/usb:/dev/bus/usb \
    --restart unless-stopped \
    gaia-radio-capture
```

On the server (processing + web):

```bash
docker compose up --build -d
```

The processing container will automatically discover the capture instance on the Pi via mDNS.

## Multiple capture units

Each capture container auto-registers with a sequential name:

- `gaia-radio-capture-01`
- `gaia-radio-capture-02`
- etc.

The processing container connects to the **first** capture service it discovers. To run multiple processing instances (one per capture), run separate containers.

## Verify mDNS discovery

To check what services are visible on the network:

```bash
# See capture instances
avahi-browse -r _adsbbeast._tcp

# See processing instances
avahi-browse -r _readsb._tcp
```

## CI/CD

The GitHub Actions workflow in `.github/workflows/build-and-publish.yml` builds all three container images for `linux/amd64` and `linux/arm64` and pushes them to Docker Hub.

**Required GitHub repository configuration:**

| Type   | Name                | Value                         |
|--------|---------------------|-------------------------------|
| Secret | `DOCKERHUB_USERNAME`| Your Docker Hub username      |
| Secret | `DOCKERHUB_TOKEN`   | A Docker Hub access token     |

## CO₂ Tracker

The web container includes a hybrid CO₂ emission tracker with **server-side** and **client-side** components.

### Server-side daemon (`co2daemon.sh`)

A background process inside the web container reads `/run/readsb/aircraft.json` every 5 seconds, computes Haversine distances, and estimates CO₂ using ADS-B emitter category factors. This runs **continuously** — no browser needed.

State is stored in `/var/lib/co2tracker/` (the `co2-state` volume in compose). Totals and unique-aircraft counts persist across container restarts.

The daemon publishes `co2data.json` which the web UI fetches.

| Environment Variable | Default | Description                         |
|----------------------|---------|-------------------------------------|
| `CO2_INTERVAL`       | `5`     | Seconds between tracking cycles     |

### Client-side module (`co2tracker.js`)

A floating panel on the map that shows:

- **All time** — from the server daemon (or localStorage fallback if daemon is unreachable)
- **Session** — CO₂ and distance since the current page load, using type-specific emission factors for ~150 ICAO codes
- **Per-aircraft** detail when a plane is selected (distance, CO₂, emission factor source)

A green dot indicates the server daemon is active; grey means browser-only mode.

The panel can be collapsed by clicking its header.

> **Note:** Emission values are estimates based on published fuel-burn data
> (Boeing/Airbus performance summaries, Aircraft Commerce guides, ATR/Bombardier
> specs) multiplied by the IPCC kerosene emission factor of 3.16 kg CO₂/kg.
> The server uses ADS-B category-based factors; the client uses type-specific
> factors for ~150 ICAO codes. Actual emissions vary with payload, altitude,
> weather, and phase of flight. See
> [`web/CO2_METHODOLOGY.md`](web/CO2_METHODOLOGY.md) for the full derivation,
> source references, and known limitations.
