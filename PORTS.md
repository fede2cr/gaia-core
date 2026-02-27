# Gaia — TCP Port Allocation

This document lists every TCP port used across all Gaia containers to prevent
conflicts when running multiple projects on the same host.

## Port Map

| Port  | Protocol    | Project       | Container / Service      | Description                                      |
|-------|-------------|---------------|--------------------------|--------------------------------------------------|
| 3100  | HTTP        | **Gaia Core** | `gaia-core`              | Core web interface (Leptos SSR) + reverse proxy   |
| 3000  | HTTP        | Gaia Audio    | `gaia-audio-web`         | Audio detections web dashboard (Leptos SSR)        |
| 8089  | HTTP        | Gaia Audio    | `gaia-audio-capture`     | Capture server — serves recorded audio             |
| 8080  | HTTP        | Gaia Radio    | `gaia-radio-web`         | tar1090 flight map + CO₂ tracker (lighttpd)        |
| 30001 | TCP (raw)   | Gaia Radio    | `gaia-radio-processing`  | Raw input port for feeding ADS-B data              |
| 30002 | TCP (raw)   | Gaia Radio    | `gaia-radio-processing`  | Raw output — decoded ADS-B messages                |
| 30003 | TCP (SBS)   | Gaia Radio    | `gaia-radio-processing`  | SBS/BaseStation format output (CSV aircraft data)  |
| 30005 | TCP (Beast) | Gaia Radio    | `gaia-radio-capture`     | Beast binary output (capture → processing)         |
| 30005 | TCP (Beast) | Gaia Radio    | `gaia-radio-processing`  | Beast binary output (processing → consumers)       |
| 554   | RTSP        | GMN (RMS)     | `rms`                    | IP camera video stream                             |

## Reserved Ranges

To simplify future expansion each project "owns" a port range:

| Range         | Project       |
|---------------|---------------|
| 3100–3199     | Gaia Core     |
| 3000–3099     | Gaia Audio    |
| 8080–8099     | Gaia Radio (HTTP) |
| 30000–30099   | Gaia Radio (ADS-B protocols) |
| 8180–8199     | GMN (RMS) — future web interface |

## Notes

* **mDNS** (UDP 5353) is used by Gaia Radio (and will be adopted by all projects)
  for automatic node discovery on the LAN.  It is a multicast UDP port shared by
  all services via `network_mode: host`.
* All containers are built for both **ARM64** and **AMD64**.
* Ports listed here are *host-side* mappings.  Container-internal ports may differ
  when a compose file remaps them.
