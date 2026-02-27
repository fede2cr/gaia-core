# CO₂ Emission Estimation Methodology

## Gaia Radio ADS-B Tracker — Technical Documentation

**Version:** 1.0
**Date:** 2025-07-22
**Status:** Open for review. Corrections and improved data sources welcome via pull request.

---

## Table of Contents

1. [Purpose & Scope](#1-purpose--scope)
2. [Mathematical Model](#2-mathematical-model)
3. [Kerosene-to-CO₂ Conversion Factor](#3-kerosene-to-co₂-conversion-factor)
4. [Aircraft Fuel-Burn Data Sources](#4-aircraft-fuel-burn-data-sources)
5. [Emission Factor Derivation by Category](#5-emission-factor-derivation-by-category)
6. [Fallback Estimation for Unknown Types](#6-fallback-estimation-for-unknown-types)
7. [What Is NOT Included](#7-what-is-not-included)
8. [Comparison with Other Methodologies](#8-comparison-with-other-methodologies)
9. [Limitations & Caveats](#9-limitations--caveats)
10. [Full Emission Factor Table](#10-full-emission-factor-table)
11. [References](#11-references)

---

## 1. Purpose & Scope

This document describes how **Gaia Radio** estimates CO₂ emissions from aircraft
observed by a local ADS-B receiver. The tracker operates in two modes:

- **Server-side daemon** (`co2daemon.sh`) — runs continuously inside the web
  container, reading position data from readsb every 5 seconds. Uses ADS-B
  emitter category–based emission factors.
- **Client-side module** (`co2tracker.js`) — runs in the browser, providing
  per-aircraft detail with ICAO type code–specific emission factors for ~150
  aircraft types.

### What we estimate

**Total aircraft CO₂ emissions per km of observed flight**, expressed in
kg CO₂/km. This is the total fuel burn of the aircraft converted to CO₂ — it
is **not** a per-passenger figure.

### Why total aircraft, not per-passenger?

An ADS-B receiver cannot determine how many passengers are on board, what cabin
class configuration is in use, or the cargo load. Any per-passenger estimate
would require assumptions (load factors, seat counts) that add uncertainty. We
present the raw total-aircraft figure and leave per-passenger allocation to the
user.

---

## 2. Mathematical Model

### 2.1 Distance Calculation

We use the **Haversine formula** to compute great-circle distance between
consecutive ADS-B position reports:

```
a = sin²(Δlat/2) + cos(lat₁) · cos(lat₂) · sin²(Δlon/2)
d = 2 · R · atan2(√a, √(1−a))
```

Where **R = 6 371 km** (mean Earth radius, WGS-84 volumetric).

**Filters applied:**
- **Stale position:** Reports older than 120 seconds are discarded.
- **Maximum jump:** Distances > 50 km between consecutive reports are rejected
  (likely missing data, not actual flight).
- **Minimum movement:** Distances < 10 m are ignored (GPS jitter).

### 2.2 CO₂ Estimation

For each valid distance increment *d* (km) of an aircraft:

```
CO₂ (kg) = d × EF
```

Where **EF** is the emission factor in **kg CO₂ per km** for that aircraft type.

### 2.3 Emission Factor Derivation

Each emission factor is derived from published fuel-burn data:

```
EF (kg CO₂/km) = F (kg fuel/km) × 3.16
```

Where:
- **F** = total aircraft fuel burn in kg of Jet A-1 per km
- **3.16** = CO₂ emission factor for kerosene combustion (see §3)

---

## 3. Kerosene-to-CO₂ Conversion Factor

The combustion of aviation kerosene (Jet A-1) produces CO₂ according to:

**3.16 kg CO₂ per kg of kerosene**

This factor is used by:
- **IPCC** — 2006 Guidelines for National Greenhouse Gas Inventories, Vol. 2,
  Chapter 3, Table 3.6.4 (jet kerosene default: 71 500 kg CO₂/TJ, equivalent
  to 3.16 kg CO₂/kg at 43.0 MJ/kg net calorific value) [1]
- **ICAO** — Carbon Emissions Calculator (ICEC) methodology [2]
- **myclimate** — Flight emission calculator (via mobitool 2023: 3.16 kg
  CO₂e/kg kerosene for combustion) [3]
- **EMEP/EEA** — Air pollutant emission inventory guidebook 2019, §1.A.3.a [4]
- **UK DEFRA/DESNZ** — Greenhouse gas reporting conversion factors 2024 [5]

The underlying chemistry: Jet A-1 is approximately C₁₂H₂₆ (dodecane).
Complete combustion: C₁₂H₂₆ + 18.5 O₂ → 12 CO₂ + 13 H₂O. Molecular weight
ratio: (12 × 44.01) / 170.34 ≈ 3.10. Real kerosene is a mixture; the
empirically measured value of 3.16 accounts for the actual average carbon
content of Jet A-1.

**Jet fuel density:** 0.80 kg/L (IATA standard) — used when converting volume-
based fuel flows (e.g., USgal/hr from manufacturer specs) to mass. One US
gallon of Jet A-1 ≈ 3.785 L × 0.80 kg/L = 3.028 kg.

---

## 4. Aircraft Fuel-Burn Data Sources

Emission factors were derived from the following published data, in order of
preference:

### 4.1 Primary: Wikipedia "Fuel economy in aircraft"

Wikipedia's comprehensive article [6] compiles fuel-burn data from
manufacturer performance summaries (Boeing, Airbus, Bombardier, ATR, Embraer),
ICCT (International Council on Clean Transportation) reports, Aircraft Commerce
analyses, and airline disclosures. Each entry cites its original source. Data is
presented as **total aircraft fuel burn in kg/km** for specific stage lengths.

We use data for the following stage lengths as most representative of typical
operations observed by a local ADS-B receiver:

| Aircraft category | Stage length used | Rationale |
|---|---|---|
| Turboprops | 500–600 nm (930–1 110 km) | Typical regional operations |
| Regional jets | 500–600 nm (930–1 110 km) | Typical regional routes |
| Narrow-body | 1 000 nm (1 850 km) | Short-haul domestic/regional |
| Wide-body | 3 000–6 000 nm (5 560–11 110 km) | Mixed medium/long-haul |

Specific sub-sources cited by the Wikipedia article include:
- **Boeing performance summaries** (737, 747, 757, 767, 777, 787) [7][8][9][10][11]
- **Aircraft Commerce** magazine owner's & operator's guides (CRJ, ERJ, E-Jet) [12][13][14]
- **ATR** fuel-saving reports and product brochures [15][16]
- **Bombardier** (now De Havilland) Q400 fuel-efficiency manual [17]
- **Saab Aircraft Leasing** data sheets [18][19]
- **Leeham News** analysis (neo/MAX comparisons) [20][21]
- **Air Finance Journal** Air Investor reports [22]
- **Lufthansa Systems Lido/Flight** via Aircraft Commerce (A350 data) [23]

### 4.2 Secondary: Manufacturer Specifications

For business jets and types not covered by the above, we use manufacturer-
published typical cruise fuel flows (in USgal/hr or lb/hr) combined with typical
cruise speeds. The Wikipedia "Fuel economy in aircraft" tables also include
business aircraft data citing SherpaReport and manufacturer brochures [24][25].

### 4.3 Tertiary: Estimation from Similar Types

For types without published data, we estimate by interpolation from similar
aircraft (same class, similar MTOW and engine count). These are marked as
"estimated" in the factor table.

---

## 5. Emission Factor Derivation by Category

### 5.1 Turboprops

Turboprops are the most fuel-efficient fixed-wing aircraft per km. Fuel burn
ranges from ~0.9 kg/km (30-seat types like Saab 340) to ~2.3 kg/km (78-seat
Q400).

| Aircraft | Seats | Fuel kg/km | Source (stage) | CO₂ kg/km |
|---|---|---|---|---|
| ATR 42-600 | 50 | 1.30 | [22] (500 nm) | 4.1 |
| ATR 72-600 | 72 | 1.41 | [22] (500 nm) | 4.5 |
| Dash 8 Q400 | 74–78 | 1.83–2.31 | [17] (500–600 nm) | 6.5 (avg) |
| Saab 340 | 31–32 | 0.95 | [18] (500 nm) | 3.0 |
| Dornier 328 | 31–32 | 1.08 | [18] (600 nm) | 3.4 |
| Beech 1900D | 19 | 1.00 | [6] (226 nm) | 3.2 |

### 5.2 Regional Jets

Regional jets burn 1.4–3.2 kg fuel/km, with the smaller ERJ family at the
lower end and larger E-Jets/CRJ-900+ at the higher end.

| Aircraft | Seats | Fuel kg/km | Source (stage) | CO₂ kg/km |
|---|---|---|---|---|
| CRJ-200 | 50 | 1.80 | [12] (580 nm) | 5.7 |
| CRJ-700 | 70 | 2.45 | [12] (574 nm) | 7.7 |
| CRJ-900 | 88 | 2.78 | [12] (573 nm) | 8.8 |
| ERJ-135 | 37 | 1.44 | [13] (596 nm) | 4.6 |
| ERJ-145 | 50 | 1.55 | [13] (598 nm) | 4.9 |
| E-Jet 170 | 80 | 2.60 | [14] (606 nm) | 8.2 |
| E-Jet 190 | 114 | 3.24 | [14] (607 nm) | 10.2 |
| SSJ-100 | 98 | 2.81 | [6] (500 nm) | 8.9 |

### 5.3 Narrow-Body (Single-Aisle)

Narrow-body jets dominate commercial aviation. Fuel burn ranges from ~2.3 kg/km
(A220-100) to ~4.7 kg/km (757-300). Neo/MAX variants achieve ~10-15% improvement
over their predecessors.

| Aircraft | Seats | Fuel kg/km | Source (stage) | CO₂ kg/km |
|---|---|---|---|---|
| A319 | 124 | 2.93 | [6] (1 000 nm) | 9.3 |
| A320 | 150 | 3.13 | [6] (1 000 nm) | 9.9 |
| A320neo | 180 | 2.79 | [6] (1 000 nm) | 8.8 |
| A321 | 180 | 3.61 | [6] (1 000 nm) | 11.4 |
| A321neo | 220 | 3.47 | [6] (1 000 nm) | 11.0 |
| A220-300 | 135–160 | 2.30–2.56 | [6] (1 000 nm) | 7.7 |
| 737-700 | 126–128 | 2.82 | [8] (1 000 nm) | 8.9 |
| 737-800 | 162 | 3.17 | [8] (1 000 nm) | 10.0 |
| 737 MAX 8 | 162 | 2.71 | [6] (1 000 nm) | 8.6 |
| 737-900ER | 180 | 3.42 | [8] (1 000 nm) | 10.8 |
| 757-200 | 190–200 | 4.16–4.60 | [9] (1 000 nm) | 13.9 |

### 5.4 Wide-Body (Twin-Aisle)

Wide-body jets have the highest absolute fuel burn per km (5–14 kg/km), but
also carry the most passengers, giving them good per-passenger efficiency.

| Aircraft | Seats | Fuel kg/km | Source (stage) | CO₂ kg/km |
|---|---|---|---|---|
| A330-200 | 241–248 | 6.0–6.4 | [6][23] (3 000–6 000 nm) | 19.6 |
| A330-300 | 262–274 | 6.25–6.81 | [6][23] (3 000–6 000 nm) | 20.6 |
| A350-900 | 315–318 | 6.03–7.07 | [23] (5 000–6 500 nm) | 20.7 |
| A380 | 525–544 | 13.78 | [6] (6 000–7 200 nm) | 43.5 |
| 747-400 | 393–487 | 10.77–12.31 | [7][23] (medium–long) | 36.4 |
| 767-300ER | 218–269 | 5.38–5.51 | [10] (3 000 nm) | 17.2 |
| 777-200ER | 301–304 | 6.96–7.57 | [11][23] (3 000–6 000 nm) | 23.0 |
| 777-300ER | 344–382 | 8.49–8.86 | [11][23] (5 500–7 200 nm) | 27.4 |
| 787-8 | 220–291 | 5.11–5.50 | [6][23] (3 400–5 500 nm) | 16.8 |
| 787-9 | 266–304 | 5.63–5.85 | [6][23] (4 650–5 500 nm) | 18.2 |

### 5.5 Business Jets

For business jets, we derived factors from manufacturer-published cruise fuel
flows and typical cruise speeds. Since these aircraft are observed in cruise
by ADS-B, we use cruise-phase data.

Conversion: fuel flow (USgal/hr) × 3.028 (kg/USgal) / cruise speed (km/hr)
= fuel burn (kg/km), then × 3.16 = CO₂ (kg/km).

| Aircraft | Fuel flow (gal/hr) | Cruise (km/hr) | Fuel kg/km | CO₂ kg/km |
|---|---|---|---|---|
| Citation CJ3+ | ~120 | 750 | 0.48 | 1.5 |
| Phenom 300 | ~140 | 830 | 0.51 | 1.6 |
| Learjet 75 | ~209 | 860 | 0.74 | 2.3 |
| Challenger 300 | 266 | 870 | 0.93 | 2.9 |
| Falcon 7X | 318 | 900 | 1.07 | 3.4 |
| G550 | ~560 | 900 | 1.88 | 6.0 |
| Global 6000 | ~499 | 900 | 1.68 | 5.3 |

Source: fuel-flow data from [24] and manufacturer specifications.

### 5.6 Military Transport

Military aircraft emission factors are estimated from published cruise
performance in Jane's All The World's Aircraft [26] and manufacturer specs.
Older Soviet-era types use publicly available performance data.

---

## 6. Fallback Estimation for Unknown Types

When the specific ICAO type code is not in our table, we apply fallback
logic in this priority order:

### 6.1 Wake Turbulence Category (WTC)

ADS-B data may include the ICAO Wake Turbulence Category:

| WTC | Description | Fallback EF (kg CO₂/km) | Rationale |
|---|---|---|---|
| L | Light (< 7 000 kg MTOW) | 1.5 | Typical light aircraft/twin turboprop |
| M | Medium (7 000–136 000 kg) | 8.0 | Weighted avg of narrow-body fleet |
| H | Heavy (> 136 000 kg) | 22.0 | Weighted avg of wide-body fleet |
| J | Super (A380) | 43.5 | A380 is the only Super category |

### 6.2 ADS-B Emitter Category

ADS-B transponders broadcast an emitter category field:

| Category | Description | Fallback EF (kg CO₂/km) |
|---|---|---|
| A1 | Light (< 15 500 lbs) | 1.2 |
| A2 | Small (15 500–75 000 lbs) | 3.5 |
| A3 | Large (75 000–300 000 lbs) | 9.0 |
| A4 | High-Vortex Large (B757) | 13.9 |
| A5 | Heavy | 22.0 |
| A6 | High-Performance / Speed | 22.0 |
| A7 | Rotorcraft | 0.5 |
| B1 | Glider / Sailplane | 0.0 |
| B2 | Lighter-than-air | 0.1 |
| B4 | Skydiver / Parachutist | 0.0 |
| B6 | UAV | 0.1 |
| C1 | Emergency vehicle | 0.0 |
| C3 | Ground obstruction | 0.0 |

### 6.3 Default (Server Daemon Only)

The server-side daemon (`co2daemon.sh`) uses a default of **5.0 kg CO₂/km** when
neither type code, WTC, nor category are available. This represents a rough
mid-fleet average (the global commercial fleet is dominated by A320/737-family
narrow-bodies which average ~9 kg CO₂/km, but lighter aircraft pull the average
down). The client-side module returns `null` for unknown types and does not
count their emissions.

---

## 7. What Is NOT Included

### 7.1 Radiative Forcing (Non-CO₂ Effects)

Aircraft emit nitrogen oxides (NOx), water vapour, soot, and sulphate aerosols
that create additional climate forcing beyond direct CO₂. Contrails and
contrail-induced cirrus clouds also contribute. Lee et al. (2021) [27]
estimate the total radiative forcing from aviation is approximately **3× the
CO₂-only forcing** over a 20–30 year horizon (the "Radiative Forcing Index" or
RFI). myclimate uses a multiplier of 3 [3]; the IPCC has suggested 1.9–4.7 [28].

**We do not apply an RFI multiplier.** Our figures represent direct CO₂ from
kerosene combustion only. Users who wish to account for total climate impact
may multiply our values by 2–3.

### 7.2 Fuel Pre-Production (Well-to-Tank)

The extraction, refining, and transport of jet fuel produces additional emissions
before the fuel is burned. myclimate uses 0.538 kg CO₂e per kg kerosene [3].
UK DEFRA 2024 includes WTT (well-to-tank) factors of approximately 0.6 kg
CO₂e/kg [5]. **We do not include these**; our figures are tank-to-wake (combustion
only).

### 7.3 Aircraft Manufacturing & Infrastructure

Lifecycle emissions from aircraft manufacturing, maintenance, and disposal, as
well as airport infrastructure operations, are not included. myclimate adds
an aircraft factor of 0.00034 and an airport factor of 11.68 kg per flight [3].

### 7.4 Landing & Take-Off (LTO) Cycle Adjustment

We observe the aircraft wherever it happens to be — cruise, climb, descent, or
on the ground (if visible to the receiver). We apply the same emission factor
regardless of flight phase. In reality, fuel burn is higher during climb
(typically 2–3× cruise) and lower during descent. Since we compute emissions
from observed track distance, this introduces some error:

- Aircraft observed mainly in cruise → estimate is close to reality.
- Aircraft observed during climb near the airport → we underestimate (higher
  actual fuel burn per km).
- Aircraft observed during descent → we overestimate (lower actual fuel burn
  per km, glide components).

Over time, for a receiver with decent range, these tend to average out.

### 7.5 Payload & Performance Variation

Fuel burn varies with:
- **Payload** — heavier aircraft burn more fuel. Our factors assume a typical
  load.
- **Altitude** — high-altitude cruise is most efficient; low-level operations
  burn more.
- **Weather** — headwinds increase fuel burn.
- **ATC routing** — actual tracks are often longer than great-circle distance.

We track actual ADS-B positions (capturing routing deviations), but cannot
account for payload or altitude-dependent fuel-burn variation.

---

## 8. Comparison with Other Methodologies

| Feature | Gaia Radio | ICAO ICEC [2] | myclimate [3] | UK DEFRA [5] |
|---|---|---|---|---|
| Base fuel data | Wikipedia/mfr specs | ICAO Fuel Databank | EMEP/EEA guidebook | UK fleet averages |
| CO₂ factor | 3.16 kg/kg | 3.16 kg/kg | 3.16 kg/kg | 3.16 kg/kg |
| Per-passenger | No (total aircraft) | Yes | Yes | Yes |
| RFI multiplier | No | No | Yes (×3) | Optional |
| Pre-production | No | No | Yes (0.538 kg/kg) | Yes (WTT factors) |
| Distance | Observed ADS-B track | Great circle + DC | Great circle + 95 km | Fixed per route |
| Aircraft type | ~150 ICAO codes + fallbacks | ICAO seat/config equiv. | Fleet-weighted avg | Short/long/domestic |
| LTO cycle | Implicit in observation | Amortised into average | Explicit (c coefficient) | Amortised |

**Key differences from per-passenger calculators:**
Our approach tracks **per-aircraft** emissions from **observed flight**, while
ICAO ICEC and myclimate calculate **per-passenger** emissions for a **route**.
They divide aircraft emissions by (seats × load factor × cabin weighting) and
add a distance correction. Our figures should be ~100–200× higher than per-
passenger calculators for a single flight because we show the full aircraft
emission, not one seat's share.

---

## 9. Limitations & Caveats

1. **These are estimates, not measurements.** Real fuel burn depends on dozens
   of variables we cannot observe from ADS-B alone.

2. **Emission factors represent averages.** A 737-800 configured for 189
   passengers (Ryanair) burns differently from one configured for 162
   (two-class). We use an average.

3. **Stage-length dependency.** Our EF values are calibrated for medium stage
   lengths. Very short flights (shuttle/commuter) will have higher real fuel
   burn per km due to the LTO cycle. Very long flights may have higher fuel
   burn per km due to the weight of extra fuel carried.

4. **ADS-B coverage gaps.** If the receiver loses track of an aircraft and
   picks it up again, the distance between the two reports is computed as
   great-circle. The 50 km jump filter catches major gaps, but shorter gaps
   (5–50 km) may slightly under- or over-estimate.

5. **Type code accuracy.** The ICAO type code comes from the aircraft database
   (tar1090-db). A small percentage of aircraft may have incorrect or missing
   type codes, falling through to the WTC/category fallback.

6. **Newer aircraft.** As new types enter service, they need to be added to
   the emission factor table. The fallback system handles them via WTC/category
   in the meantime.

7. **Non-CO₂ effects.** By excluding RFI, our figures represent ~33–50% of the
   total climate impact of the observed flights (Lee et al. 2021 [27]).

---

## 10. Full Emission Factor Table

All values in **kg CO₂ per km** (total aircraft). Derived as fuel burn (kg/km)
× 3.16 unless noted otherwise.

### Turboprops

| ICAO Code | Aircraft | Fuel kg/km | CO₂ kg/km | Source |
|---|---|---|---|---|
| AT43 | ATR 42-300/320 | 1.30 | 4.1 | [22] 500 nm |
| AT45 | ATR 42-500 | 1.26 | 4.0 | [15] 300 nm |
| AT72 | ATR 72-500 | 1.55 (avg) | 4.9 | [15][6] 300–500 nm |
| AT76 | ATR 72-600 | 1.49 (avg) | 4.7 | [16][22] 300–500 nm |
| DH8A | Dash 8-100 | ~1.40 | 4.4 | est. from DH8D |
| DH8B | Dash 8-200 | ~1.50 | 4.7 | est. from DH8D |
| DH8C | Dash 8-300 | ~1.70 | 5.4 | est. from DH8D |
| DH8D | Dash 8 Q400 | 2.07 (avg) | 6.5 | [17][6] 500–600 nm |
| SF34 | Saab 340 | 1.03 (avg) | 3.2 | [18] 500 nm |
| D328 | Dornier 328 | 1.15 (avg) | 3.6 | [19] 600 nm |
| F50 | Fokker 50 | ~1.50 | 4.7 | est. (similar ATR 72) |
| JS41 | Jetstream 41 | ~1.00 | 3.2 | est. (29-seat TP) |
| L410 | Let 410 | ~0.65 | 2.1 | est. (19-seat light TP) |
| AN26 | Antonov An-26 | ~2.50 | 7.9 | est. (military TP) |
| AN24 | Antonov An-24 | ~2.20 | 7.0 | est. (Soviet-era TP) |
| BEH2 | Beech 1900D | 1.00 | 3.2 | [6] 226 nm |

### Regional Jets

| ICAO Code | Aircraft | Fuel kg/km | CO₂ kg/km | Source |
|---|---|---|---|---|
| CRJ1 | CRJ-100 | 1.87 | 5.9 | [12] 577 nm |
| CRJ2 | CRJ-200 | 1.80 | 5.7 | [12] 580 nm |
| CRJ7 | CRJ-700 | 2.45 | 7.7 | [12] 574 nm |
| CRJ9 | CRJ-900 | 2.78 | 8.8 | [12] 573 nm |
| CRJX | CRJ-1000 | 2.66 | 8.4 | [6] 500 nm |
| E135 | ERJ-135 | 1.44 | 4.6 | [13] 596 nm |
| E145 | ERJ-145 | 1.55 | 4.9 | [13] 598 nm |
| E170 | E-Jet 170 | 2.60 | 8.2 | [14] 606 nm |
| E75L | E-Jet 175 | 2.80 | 8.8 | [14] 605 nm |
| E75S | E-Jet 175 (short) | 2.80 | 8.8 | [14] same as E75L |
| E190 | E-Jet 190 | 3.24 | 10.2 | [14] 607 nm |
| E195 | E-Jet 195 | 3.21 | 10.1 | [14] 607 nm |
| E290 | E2-190 | 2.48 | 7.8 | [22] 500 nm |
| E295 | E2-195 | 2.62 | 8.3 | [22] 500 nm |
| F70 | Fokker 70 | ~2.30 | 7.3 | est. (smaller F100) |
| F100 | Fokker 100 | ~2.80 | 8.8 | est. (mid-size RJ) |
| RJ85 | BAe 146/Avro RJ85 | ~3.00 | 9.5 | est. (4-engine RJ) |
| RJ1H | Avro RJ100 | ~3.20 | 10.1 | est. (4-engine RJ) |
| SU95 | Sukhoi SSJ-100 | 2.81 | 8.9 | [6] 500 nm |
| AR85 | ARJ21 | ~2.80 | 8.8 | est. (similar SSJ) |

### Narrow-Body (Single-Aisle)

| ICAO Code | Aircraft | Fuel kg/km | CO₂ kg/km | Source |
|---|---|---|---|---|
| A318 | A318 | ~2.70 | 8.5 | est. (smallest A320 fam) |
| A319 | A319 | 2.93 | 9.3 | [6] 1 000 nm |
| A19N | A319neo | ~2.40 | 7.6 | [6] 1 000 nm |
| A320 | A320ceo | 2.91–3.13 | 9.5 | [6] 1 000–2 000 nm |
| A20N | A320neo | 2.79 | 8.8 | [6] 1 000 nm |
| A321 | A321ceo | 3.61 | 11.4 | [6] 1 000 nm |
| A21N | A321neo | 3.30–3.47 | 10.7 | [6][20] 660–1 000 nm |
| B731 | 737-100 | ~3.00 | 9.5 | est. (early 737) |
| B732 | 737-200 | ~3.20 | 10.1 | est. (early 737) |
| B733 | 737-300 | 3.49 (507 nm) | 10.1 | [6] regional data |
| B734 | 737-400 | ~3.30 | 10.4 | est. (between 733/738) |
| B735 | 737-500 | ~3.00 | 9.5 | est. (short-body 737) |
| B736 | 737-600 | 2.77 | 8.8 | [8] 1 000 nm |
| B737 | 737-700 | 2.82 | 8.9 | [8] 1 000 nm |
| B738 | 737-800 | 3.17 | 10.0 | [8] 1 000 nm |
| B739 | 737-900ER | 3.42 | 10.8 | [8] 1 000 nm |
| B37M | 737 MAX 7 | 2.51 | 7.9 | [6] 1 000 nm |
| B38M | 737 MAX 8 | 2.71 | 8.6 | [6] 1 000 nm |
| B39M | 737 MAX 9 | 2.91 | 9.2 | [6] 1 000 nm |
| B752 | 757-200 | 4.16–4.60 | 13.9 | [9] 1 000 nm |
| B753 | 757-300 | 4.68 | 14.8 | [9] 1 000 nm |
| MD80 | MD-80 series | ~3.50 | 11.1 | est. (MD-81/82/83) |
| MD81 | MD-81 | ~3.40 | 10.7 | est. |
| MD82 | MD-82 | ~3.50 | 11.1 | est. |
| MD83 | MD-83 | ~3.50 | 11.1 | est. |
| MD87 | MD-87 | ~3.20 | 10.1 | est. (short-body MD-80) |
| MD88 | MD-88 | ~3.50 | 11.1 | est. |
| MD90 | MD-90 | ~3.30 | 10.4 | est. |
| BCS1 | A220-100 (CS100) | 2.28 | 7.2 | [6] 1 000 nm |
| BCS3 | A220-300 (CS300) | 2.42 (avg) | 7.7 | [6] 1 000–2 000 nm |
| C919 | COMAC C919 | ~3.10 | 9.8 | est. (A320 class) |
| B712 | 717-200 | ~2.80 | 8.8 | est. (mid-size NB) |
| DC93 | DC-9-30 | ~2.80 | 8.8 | est. (classic NB) |
| DC95 | DC-9-50 | ~3.00 | 9.5 | est. |
| T204 | Tu-204 | ~3.30 | 10.4 | est. (A320 class, older) |
| T154 | Tu-154 | ~5.50 | 17.4 | est. (3-engine, Soviet, notoriously fuel-heavy) |

### Wide-Body (Twin-Aisle)

| ICAO Code | Aircraft | Fuel kg/km | CO₂ kg/km | Source |
|---|---|---|---|---|
| A306 | A300-600 | ~6.50 | 20.5 | est. (early wide-body) |
| A30B | A300B | ~7.00 | 22.1 | est. (first-gen wide-body) |
| A310 | A310 | ~5.50 | 17.4 | est. (smaller A300 deriv.) |
| A332 | A330-200 | 6.20 (avg) | 19.6 | [6][23] 3 000–6 000 nm |
| A333 | A330-300 | 6.53 (avg) | 20.6 | [6][23] 3 000–6 000 nm |
| A338 | A330-800neo | 5.45 | 17.2 | [6] 4 650 nm |
| A339 | A330-900neo | 5.97 (avg) | 18.9 | [6] 3 350–4 650 nm |
| A342 | A340-200 | ~7.00 | 22.1 | est. (4-engine A330) |
| A343 | A340-300 | 7.06 (avg) | 22.3 | [6] 3 000–6 000 nm |
| A345 | A340-500 | ~8.00 | 25.3 | est. (long-range variant) |
| A346 | A340-600 | ~8.50 | 26.9 | est. (stretched A340) |
| A359 | A350-900 | 6.55 (avg) | 20.7 | [23] 5 000–6 500 nm |
| A35K | A350-1000 | 7.52 (avg) | 23.8 | [23] 5 500 nm |
| A388 | A380 | 13.78 | 43.5 | [6] 6 000–7 200 nm |
| B741 | 747-100 | ~12.00 | 37.9 | est. (first-gen 747) |
| B742 | 747-200 | ~12.00 | 37.9 | est. |
| B743 | 747-300 | ~12.00 | 37.9 | est. |
| B744 | 747-400 | 11.54 (avg) | 36.5 | [7][23] mixed stages |
| B748 | 747-8 | 10.45 (avg) | 33.0 | [7] 3 000–6 000 nm |
| B762 | 767-200ER | 4.92 (avg) | 15.5 | [10] 3 000 nm |
| B763 | 767-300ER | 5.45 (avg) | 17.2 | [10] 3 000 nm |
| B764 | 767-400ER | 5.86 (avg) | 18.5 | [10] 3 000 nm |
| B772 | 777-200 | 6.83 | 21.6 | [11] 3 000 nm |
| B77L | 777-200LR | 7.57 | 23.9 | [6] 5 000 nm |
| B77W | 777-300ER | 8.68 (avg) | 27.4 | [11][23] 5 500–7 200 nm |
| B788 | 787-8 | 5.31 (avg) | 16.8 | [6][23] 3 400–5 500 nm |
| B789 | 787-9 | 5.74 (avg) | 18.1 | [6][23] 4 650–5 500 nm |
| B78X | 787-10 | 6.17 (avg) | 19.5 | [23] 5 500 nm |
| DC10 | DC-10 | ~8.50 | 26.9 | est. (trijet) |
| MD11 | MD-11 | ~8.50 | 26.9 | est. (DC-10 derivative) |
| L101 | L-1011 TriStar | ~8.50 | 26.9 | est. (trijet) |
| IL96 | Il-96 | ~9.00 | 28.4 | est. (Soviet wide-body) |
| IL86 | Il-86 | ~10.00 | 31.6 | est. (fuel-heavy) |

### Business / Private Jets

| ICAO Code | Aircraft | Fuel kg/km | CO₂ kg/km | Source |
|---|---|---|---|---|
| C25A | Citation CJ1 | ~0.38 | 1.2 | est. (VLJ class) |
| C25B | Citation CJ2 | ~0.41 | 1.3 | est. |
| C25C | Citation CJ3 | ~0.48 | 1.5 | [24] cruise data |
| C25M | Citation CJ4 | ~0.51 | 1.6 | est. |
| C510 | Citation Mustang | ~0.33 | 1.0 | est. (VLJ) |
| C525 | CitationJet/CJ1 | ~0.38 | 1.2 | est. |
| C500 | Citation I | ~0.35 | 1.1 | est. (light jet) |
| C550 | Citation II | ~0.44 | 1.4 | est. |
| C560 | Citation V/Ultra | ~0.57 | 1.8 | est. |
| C56X | Citation Excel | ~0.63 | 2.0 | est. |
| C680 | Citation Sovereign | ~0.76 | 2.4 | est. |
| C68A | Citation Latitude | ~0.76 | 2.4 | est. |
| C700 | Citation Longitude | ~0.82 | 2.6 | est. |
| C750 | Citation X | 1.11 | 3.5 | [24] 336 gal/hr / 920 km/hr |
| CL30 | Challenger 300 | 0.93 | 2.9 | [24] 266 gal/hr |
| CL35 | Challenger 350 | ~0.93 | 2.9 | est. (same as CL30) |
| CL60 | Challenger 600/604 | ~1.00 | 3.2 | est. (mid-large cabin) |
| GL5T | Global 5500 | ~1.60 | 5.1 | est. (large cabin) |
| GL7T | Global 7500 | ~1.75 | 5.5 | est. (ultra-long range) |
| GLEX | Global Express | ~1.68 | 5.3 | [24] ~499 gal/hr |
| GLF4 | Gulfstream IV | ~1.10 | 3.5 | est. |
| GLF5 | Gulfstream V/G550 | 1.88 | 5.9 | [24] ~560 gal/hr |
| GLF6 | Gulfstream G650 | ~1.70 | 5.4 | est. |
| G150 | Gulfstream G150 | ~0.57 | 1.8 | est. (mid-light) |
| G280 | Gulfstream G280 | ~0.76 | 2.4 | est. (super mid-size) |
| FA50 | Falcon 50 | ~0.70 | 2.2 | est. (trijet BJ) |
| FA7X | Falcon 7X | 1.07 | 3.4 | [24] 318 gal/hr |
| FA8X | Falcon 8X | ~1.10 | 3.5 | est. (8X variant) |
| F900 | Falcon 900 | ~0.85 | 2.7 | est. (trijet) |
| F2TH | Falcon 2000 | ~0.76 | 2.4 | est. |
| E35L | Phenom 300 | 0.51 | 1.6 | [24] ~140 gal/hr |
| E55P | Phenom 300E | 0.51 | 1.6 | [24] same platform |
| LJ35 | Learjet 35 | ~0.63 | 2.0 | est. |
| LJ45 | Learjet 45 | ~0.67 | 2.1 | est. |
| LJ60 | Learjet 60 | ~0.76 | 2.4 | est. |
| LJ75 | Learjet 75 | 0.74 | 2.3 | [24] ~209 gal/hr |
| H25B | Hawker 800 | ~0.80 | 2.5 | est. |
| H25C | Hawker 900XP | 0.96 | 3.0 | [24] 257 gal/hr |
| GALX | Galaxy/G200 | 0.83 | 2.6 | [24] 233 gal/hr |
| ASTR | Astra SPX | ~0.57 | 1.8 | est. (mid-size) |
| PC12 | Pilatus PC-12 | 0.44 | 1.4 | [24] 66 gal/hr |
| PC24 | Pilatus PC-24 | ~0.51 | 1.6 | est. (light jet) |
| TBM7 | TBM 700 | ~0.35 | 1.1 | est. (single TP) |
| TBM8 | TBM 850 | ~0.35 | 1.1 | est. |
| TBM9 | TBM 900/930/960 | ~0.35 | 1.1 | est. |
| PRM1 | Beechjet Premier | ~0.48 | 1.5 | est. |
| P180 | Piaggio Avanti | ~0.44 | 1.4 | est. (efficient pusher TP) |
| BE20 | King Air 200 | ~0.44 | 1.4 | est. (twin TP) |
| BE30 | King Air 350 | 0.61 | 1.9 | [24] 100 gal/hr |
| BE40 | Beechjet 400A | ~0.51 | 1.6 | est. |
| BE4W | Hawker 400XP | ~0.54 | 1.7 | est. |
| EA50 | Eclipse 500 | ~0.30 | 1.0 | est. (VLJ) |

### Military Transport / Tanker

| ICAO Code | Aircraft | Fuel kg/km | CO₂ kg/km | Source |
|---|---|---|---|---|
| C130 | C-130 Hercules | ~4.50 | 14.2 | est. (4-TP military) |
| C30J | C-130J Super Herc | ~4.00 | 12.6 | est. (more efficient) |
| C17 | C-17 Globemaster | ~10.00 | 31.6 | est. (heavy jet transport) |
| C5 | C-5 Galaxy | ~14.50 | 45.8 | est. (heaviest USAF transport) |
| C5M | C-5M Super Galaxy | ~14.50 | 45.8 | est. (re-engined C-5) |
| K35R | KC-135 Stratotanker | ~8.50 | 26.9 | est. (707-based tanker) |
| KC10 | KC-10 Extender | ~8.50 | 26.9 | est. (DC-10-based tanker) |
| A400 | A400M Atlas | ~4.80 | 15.2 | est. (modern TP transport) |
| MRTT | A330 MRTT | ~6.50 | 20.5 | est. (A330-based tanker) |
| A124 | An-124 Ruslan | ~18.00 | 56.9 | est. (heaviest operational) |
| AN12 | An-12 Cub | ~4.50 | 14.2 | est. (4-TP Soviet) |
| IL76 | Il-76 Candid | ~7.50 | 23.7 | est. (heavy jet transport) |
| E3CF | E-3 Sentry (AWACS) | ~8.50 | 26.9 | est. (707-based) |
| E6 | E-6 Mercury | ~8.50 | 26.9 | est. (707-based) |
| P3 | P-3 Orion | ~3.50 | 11.1 | est. (4-TP patrol) |
| P8 | P-8 Poseidon | ~3.50 | 11.1 | est. (737-based patrol) |

---

## 11. References

[1] IPCC, "2006 IPCC Guidelines for National Greenhouse Gas Inventories —
    Volume 2: Energy," Chapter 3, Table 3.6.4. Available:
    https://www.ipcc-nggip.iges.or.jp/public/2006gl/vol2.html

[2] ICAO, "ICAO Carbon Emissions Calculator (ICEC) — Methodology," v13.
    Available: https://applications.icao.int/icec
    (Note: ICEC is described as "the only internationally approved tool to
    estimate carbon emissions from air travel.")

[3] myclimate, "Calculation principles — Flight Emissions Calculator," 2023.
    Available: https://www.myclimate.org/information/about-myclimate/downloads/flight-emission-calculator/
    CO₂ factor: 3.16 kg CO₂e/kg kerosene (mobitool 2023); pre-production:
    0.538 kg CO₂e/kg kerosene; RFI multiplier: 3 (Lee et al. 2021).

[4] EEA, "EMEP/EEA air pollutant emission inventory guidebook 2019 — Part B,
    §1.A.3.a Aviation." Available:
    https://www.eea.europa.eu/publications/emep-eea-guidebook-2019

[5] UK DESNZ (Dept. for Energy Security and Net Zero), "Greenhouse gas
    reporting: conversion factors 2024." Published 8 Jul 2024. Available:
    https://www.gov.uk/government/publications/greenhouse-gas-reporting-conversion-factors-2024

[6] Wikipedia, "Fuel economy in aircraft." Last accessed Jul 2025. Available:
    https://en.wikipedia.org/wiki/Fuel_economy_in_aircraft
    (This article compiles data from manufacturer sources, ICCT reports, and
    industry analyses, each individually cited within the article.)

[7] Boeing, "747 performance summary" and "747-8 performance summary,"
    2007–2010. (Archived PDF, cited in [6].)

[8] Boeing, "737 performance summary," 2007. (Archived PDF, cited in [6].)

[9] Boeing, "757 performance summary," 2007. (PDF, cited in [6].)

[10] Boeing, "767 performance summary," 2006. (Archived PDF, cited in [6].)

[11] Boeing, "777 performance summary," 2009. (Archived PDF, cited in [6].)

[12] Aircraft Commerce, "CRJ family fuel-burn performance," Oct 2009.
     (PDF, cited in [6].)

[13] Aircraft Commerce, "Owner's & Operator's Guide: ERJ-135/-140/-145,"
     Dec 2008. (PDF, cited in [6].)

[14] Aircraft Commerce, "Owner's & Operator's Guide: E-Jets family," Jun 2009.
     (PDF, cited in [6].)

[15] ATR, "Fuel saving," Jan 2011. (PDF, cited in [6].)

[16] ATR, "ATR 72-600 — The first choice for operators," product brochure.
     (PDF, cited in [6].)

[17] Bombardier, "Q400 Fuel efficiency manual," 2014. (PDF, cited in [6].)

[18] Saab Aircraft Leasing, "Saab 340A data sheet," 2009. (Archived, cited in [6].)

[19] Saab Aircraft Leasing / 328 Support Services, "Dornier 328-100"
     data sheet, 2013. (Archived PDF, cited in [6].)

[20] Leeham News, "ANALYSIS: A320neo vs. 737 MAX," Feb 2016. (Cited in [6].)

[21] Leeham News, "Redefining the 757 replacement," Feb 2015. (Cited in [6].)

[22] Air Finance Journal, "Air Investor 2021." (Cited in [6].)

[23] Lufthansa Systems Lido/Flight via Aircraft Commerce, "A350-900/-1000
     fuel burn & operating performance," Dec 2018. (PDF, cited in [6].)

[24] Wikipedia, "Fuel economy in aircraft — Business aircraft" table, citing
     SherpaReport, "Fuel Burn Rates for Private Aircraft," Sep 2015, and
     various manufacturer brochures. (Data in [6].)

[25] Pilatus Aircraft, "PC-12 NG Just The Facts," 2015; Bombardier CRJ /
     Cessna / Gulfstream / Dassault manufacturer specifications.

[26] Jane's All The World's Aircraft (IHS Markit). Reference compendium for
     military aircraft performance data.

[27] D.S. Lee et al., "The contribution of global aviation to anthropogenic
     climate forcing for 2000 to 2018," Atmospheric Environment, vol. 244,
     117834, 2021. doi:10.1016/j.atmosenv.2020.117834

[28] IPCC, "Aviation and the Global Atmosphere," Special Report, Chapter 6:
     "Potential Effects of Aircraft Emissions on Climate," 1999.

[29] B. Graver, K. Zhang, D. Rutherford, "CO₂ emissions from commercial
     aviation, 2018," ICCT, Sep 2019. Available:
     https://theicct.org/publications/co2-emissions-commercial-aviation-2018

[30] Kollmuss, A. & Lane, J., "Carbon Offset Calculators for Air Travel,"
     Stockholm Environment Institute, May 2008.

---

*This document is part of the Gaia Radio project. It is provided for
transparency and may be freely distributed. The emission factors are rough
estimates intended for awareness, not regulatory compliance.*
