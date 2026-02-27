// co2tracker.js — Gaia Radio CO₂ emission tracker for tar1090
//
// Hybrid architecture:
//   • Server-side daemon (co2daemon.sh) tracks CO₂ continuously and
//     publishes totals to /co2data.json.  These are the authoritative
//     "all time" numbers.
//   • This client-side module provides real-time per-aircraft detail and
//     a "session" counter while the page is open.
//   • If the server daemon is unreachable, falls back to localStorage.
(function () {
  'use strict';

  // ======================== Configuration ========================
  var UPDATE_INTERVAL   = 5000;   // ms between client tracking cycles
  var SERVER_POLL       = 10000;  // ms between server data fetches
  var STALE_POS_SEC     = 120;
  var MAX_JUMP_KM       = 50;
  var MIN_MOVE_KM       = 0.01;
  var CLEANUP_MAX_AGE   = 3600000;
  var STORAGE_KEY       = 'gaia_co2_tracker';

  // ======================== Emission Factors ========================
  // kg CO₂ per km (total aircraft, combustion only).
  //
  // Derived as:  fuel burn (kg/km)  ×  3.16  (IPCC kerosene CO₂ factor)
  //
  // Fuel-burn data sourced primarily from Wikipedia "Fuel economy in
  // aircraft" [W] which compiles Boeing/Airbus/ATR/Bombardier performance
  // summaries, Aircraft Commerce owner's guides, and ICCT studies.
  // Business-jet values from manufacturer cruise fuel-flow ÷ cruise speed.
  // Values marked (est.) interpolated from similar types.
  //
  // Full derivation:  CO2_METHODOLOGY.md
  var EF = {
    // ── Turboprops ─────────────────────────────────────────────────
    AT43: 4.1,  AT45: 4.0,  AT72: 4.9,  AT76: 4.7,   // ATR [W]
    DH8A: 4.4,  DH8B: 4.7,  DH8C: 5.4,  DH8D: 6.5,   // Dash 8 [W]
    SF34: 3.2,  D328: 3.6,  F50:  4.7,  JS41: 3.2,    // Saab/Dornier [W]
    L410: 2.1,  AN26: 7.9,  AN24: 7.0,  BEH2: 3.2,    // misc tp
    // ── Regional jets ──────────────────────────────────────────────
    CRJ1: 5.9,  CRJ2: 5.7,  CRJ7: 7.7,  CRJ9: 8.8,  CRJX: 8.4, // [W]
    E135: 4.6,  E145: 4.9,  E170: 8.2,  E75L: 8.8,  E75S: 8.8, // [W]
    E190: 10.2, E195: 10.1, E290: 7.8,  E295: 8.3,               // [W]
    F70:  7.3,  F100: 8.8,  RJ85: 9.5,  RJ1H: 10.1,             // est.
    SU95: 8.9,  AR85: 8.8,                                        // [W]/est.
    // ── Narrow-body (single-aisle) ─────────────────────────────────
    A318: 8.5,  A319: 9.3,  A19N: 7.6,                            // [W]
    A320: 9.5,  A20N: 8.8,                                        // [W]
    A321: 11.4, A21N: 10.7,                                       // [W]
    B731: 9.5,  B732: 10.1, B733: 10.1,                           // [W]/est.
    B734: 10.4, B735: 9.5,  B736: 8.8,                            // [W]/est.
    B737: 8.9,  B738: 10.0, B739: 10.8,                           // [W]
    B37M: 7.9,  B38M: 8.6,  B39M: 9.2,                            // [W]
    B752: 13.9, B753: 14.8,                                       // [W]
    MD80: 11.1, MD81: 10.7, MD82: 11.1, MD83: 11.1,              // est.
    MD87: 10.1, MD88: 11.1, MD90: 10.4,                           // est.
    BCS1: 7.2,  BCS3: 7.7,                                        // [W]
    C919: 9.8,  B712: 8.8,                                        // est.
    DC93: 8.8,  DC95: 9.5,                                        // est.
    T204: 10.4, T154: 17.4,                                       // est.
    // ── Wide-body (twin-aisle) ─────────────────────────────────────
    A306: 20.5, A30B: 22.1, A310: 17.4,                           // est.
    A332: 19.6, A333: 20.6,                                       // [W]
    A338: 17.2, A339: 18.9,                                       // [W]
    A342: 22.1, A343: 22.3, A345: 25.3, A346: 26.9,              // [W]/est.
    A359: 20.7, A35K: 23.8, A388: 43.5,                           // [W]
    B741: 37.9, B742: 37.9, B743: 37.9,                           // est.
    B744: 36.5, B748: 33.0,                                       // [W]
    B762: 15.5, B763: 17.2, B764: 18.5,                           // [W]
    B772: 21.6, B77L: 23.9, B77W: 27.4,                           // [W]
    B788: 16.8, B789: 18.1, B78X: 19.5,                           // [W]
    DC10: 26.9, MD11: 26.9,                                       // est.
    L101: 26.9, IL96: 28.4, IL86: 31.6,                           // est.
    // ── Business / private jets ────────────────────────────────────
    C25A: 1.2,  C25B: 1.3,  C25C: 1.5,  C25M: 1.6,              // mfr cruise
    C510: 1.0,  C525: 1.2,                                        // est.
    C500: 1.1,  C550: 1.4,  C560: 1.8,  C56X: 2.0,              // est.
    C680: 2.4,  C68A: 2.4,  C700: 2.6,  C750: 3.5,              // mfr cruise
    CL30: 2.9,  CL35: 2.9,  CL60: 3.2,                           // [W]
    GL5T: 5.1,  GL7T: 5.5,  GLEX: 5.3,                            // [W]
    GLF4: 3.5,  GLF5: 5.9,  GLF6: 5.4,                            // [W]
    G150: 1.8,  G280: 2.4,                                        // est.
    FA50: 2.2,  FA7X: 3.4,  FA8X: 3.5,  F900: 2.7,  F2TH: 2.4, // [W]/est.
    E35L: 1.6,  E55P: 1.6,                                        // [W]
    LJ35: 2.0,  LJ45: 2.1,  LJ60: 2.4,  LJ75: 2.3,             // [W]/est.
    H25B: 2.5,  H25C: 3.0,                                        // [W]
    GALX: 2.6,  ASTR: 1.8,                                        // [W]
    PC12: 1.4,  PC24: 1.6,  TBM7: 1.1,  TBM8: 1.1,  TBM9: 1.1, // mfr cruise
    PRM1: 1.5,  P180: 1.4,                                        // est.
    BE20: 1.4,  BE30: 1.9,  BE40: 1.6,  BE4W: 1.7,              // [W]/est.
    EA50: 1.0,                                                     // est.
    // ── Military transport / tanker ────────────────────────────────
    C130: 14.2, C30J: 12.6, C17:  31.6, C5:   45.8, C5M: 45.8,  // est.
    K35R: 26.9, KC10: 26.9, A400: 15.2, MRTT: 20.5,             // est.
    A124: 56.9, AN12: 14.2, IL76: 23.7,                           // est.
    E3CF: 26.9, E6:   26.9, P3:   11.1, P8:   11.1              // est.
  };

  // Fallbacks when ICAO type code is unknown — see CO2_METHODOLOGY.md §6
  var WTC_FALLBACK = { L: 1.5, M: 8.0, H: 22.0, J: 43.5 };
  var CAT_FALLBACK = {
    A1: 1.2, A2: 3.5, A3: 9.0, A4: 13.9, A5: 22.0, A6: 22.0, A7: 0.5,
    B1: 0.0, B2: 0.1, B4: 0.0, B6: 0.1, C1: 0.0, C3: 0.0
  };

  // ======================== Haversine ========================
  function toRad(deg) { return deg * Math.PI / 180; }

  function haversineKm(lat1, lon1, lat2, lon2) {
    var dLat = toRad(lat2 - lat1);
    var dLon = toRad(lon2 - lon1);
    var a = Math.sin(dLat / 2) * Math.sin(dLat / 2) +
            Math.cos(toRad(lat1)) * Math.cos(toRad(lat2)) *
            Math.sin(dLon / 2) * Math.sin(dLon / 2);
    return 6371.0 * 2 * Math.atan2(Math.sqrt(a), Math.sqrt(1 - a));
  }

  // ======================== State ========================
  var tracked     = {};      // per-aircraft client-side tracking
  var sessionCO2  = 0;       // kg – this page session
  var sessionDist = 0;       // km – this page session
  var sessionCount = 0;      // aircraft seen this session
  var seenHexes   = {};

  // Server-side data (fetched from co2data.json)
  var serverData  = null;    // { co2Kg, distKm, count, since, updated }
  var serverOK    = false;   // true when server daemon is reachable

  // Fallback localStorage totals (used only when server is down)
  var lsCO2   = 0;
  var lsDist  = 0;
  var lsCount = 0;

  // ======================== Emission lookup ========================
  function getEmissionFactor(type, wtc, cat) {
    if (type && EF[type] !== undefined) return EF[type];
    if (wtc && WTC_FALLBACK[wtc] !== undefined) return WTC_FALLBACK[wtc];
    if (cat && CAT_FALLBACK[cat] !== undefined) return CAT_FALLBACK[cat];
    return null;
  }

  // ======================== Server data fetch ========================
  function fetchServerData() {
    var xhr = new XMLHttpRequest();
    xhr.open('GET', 'co2data.json?_=' + Date.now(), true);
    xhr.timeout = 5000;
    xhr.onload = function () {
      if (xhr.status === 200) {
        try {
          serverData = JSON.parse(xhr.responseText);
          serverOK = true;
        } catch (e) { serverOK = false; }
      } else {
        serverOK = false;
      }
    };
    xhr.onerror = xhr.ontimeout = function () { serverOK = false; };
    xhr.send();
  }

  // ======================== localStorage fallback ========================
  function loadLS() {
    try {
      var s = JSON.parse(localStorage.getItem(STORAGE_KEY) || '{}');
      if (s.v === 2) { lsCO2 = s.co2 || 0; lsDist = s.dist || 0; lsCount = s.cnt || 0; }
    } catch (e) {}
  }

  function saveLS() {
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify({
        v: 2, co2: lsCO2 + sessionCO2, dist: lsDist + sessionDist,
        cnt: lsCount + sessionCount
      }));
    } catch (e) {}
  }

  // ======================== Core client-side tracking ========================
  function processPlane(plane) {
    if (!plane.position) return;
    if (plane.seen_pos > STALE_POS_SEC) return;

    var hex = plane.icao;
    var lat = plane.position[1];
    var lon = plane.position[0];
    var posTime = plane.position_time;

    if (!seenHexes[hex]) { seenHexes[hex] = true; sessionCount++; }

    var ac = tracked[hex];
    if (!ac) {
      tracked[hex] = {
        lastLat: lat, lastLon: lon, lastPosTime: posTime,
        distKm: 0, co2Kg: 0,
        type: plane.icaoType || null,
        wtc: plane.wtc || null,
        cat: plane.category || null,
        lastUpdate: Date.now()
      };
      return;
    }

    ac.lastUpdate = Date.now();
    if (posTime && posTime === ac.lastPosTime) return;
    if (ac.lastLat === null) {
      ac.lastLat = lat; ac.lastLon = lon; ac.lastPosTime = posTime;
      return;
    }

    var dist = haversineKm(ac.lastLat, ac.lastLon, lat, lon);

    if (dist > MAX_JUMP_KM) {
      ac.lastLat = lat; ac.lastLon = lon; ac.lastPosTime = posTime;
      return;
    }

    if (dist > MIN_MOVE_KM) {
      ac.distKm += dist;
      sessionDist += dist;

      if (plane.icaoType) ac.type = plane.icaoType;
      if (plane.wtc) ac.wtc = plane.wtc;
      if (plane.category) ac.cat = plane.category;

      var factor = getEmissionFactor(ac.type, ac.wtc, ac.cat);
      if (factor !== null) {
        var co2 = dist * factor;
        ac.co2Kg += co2;
        sessionCO2 += co2;
      }
    }

    ac.lastLat = lat; ac.lastLon = lon; ac.lastPosTime = posTime;
  }

  function tick() {
    if (typeof g === 'undefined' || !g.planes) return;

    for (var hex in g.planes) { processPlane(g.planes[hex]); }

    // Cleanup stale
    var now = Date.now();
    for (var h in tracked) {
      if (now - tracked[h].lastUpdate > CLEANUP_MAX_AGE) delete tracked[h];
    }

    updatePanel();
    updateSelectedInfo();
  }

  // ======================== Formatting helpers ========================
  function fmtCO2(kg) {
    if (kg >= 1000) return (kg / 1000).toFixed(2) + ' t';
    if (kg >= 10) return kg.toFixed(1) + ' kg';
    return kg.toFixed(2) + ' kg';
  }

  function fmtDist(km) {
    if (km >= 1000) return (km / 1000).toFixed(1) + 'k km';
    if (km >= 10) return km.toFixed(1) + ' km';
    return km.toFixed(2) + ' km';
  }

  function fmtAge(iso) {
    if (!iso) return '';
    var d = new Date(iso);
    if (isNaN(d)) return '';
    var s = Math.floor((Date.now() - d.getTime()) / 1000);
    if (s < 60) return s + 's ago';
    if (s < 3600) return Math.floor(s / 60) + 'm ago';
    if (s < 86400) return Math.floor(s / 3600) + 'h ago';
    return Math.floor(s / 86400) + 'd ago';
  }

  // ======================== UI – Summary Panel ========================
  function createPanel() {
    var style = document.createElement('style');
    style.textContent = '\
      #co2-panel {\
        position: fixed;\
        bottom: 12px;\
        left: 12px;\
        z-index: 1000;\
        background: rgba(24, 24, 28, 0.88);\
        color: #e0e0e0;\
        border-radius: 8px;\
        padding: 0;\
        font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;\
        font-size: 12px;\
        line-height: 1.5;\
        min-width: 180px;\
        box-shadow: 0 2px 12px rgba(0,0,0,0.4);\
        backdrop-filter: blur(8px);\
        -webkit-backdrop-filter: blur(8px);\
        border: 1px solid rgba(255,255,255,0.08);\
        overflow: hidden;\
        user-select: none;\
      }\
      #co2-panel-header {\
        display: flex;\
        align-items: center;\
        justify-content: space-between;\
        padding: 7px 10px;\
        cursor: pointer;\
        background: rgba(255,255,255,0.04);\
        border-bottom: 1px solid rgba(255,255,255,0.06);\
      }\
      #co2-panel-header:hover { background: rgba(255,255,255,0.08); }\
      #co2-panel-title {\
        font-weight: 600;\
        font-size: 11px;\
        text-transform: uppercase;\
        letter-spacing: 0.5px;\
        color: #81c784;\
      }\
      #co2-panel-toggle {\
        font-size: 10px;\
        color: #888;\
        transition: transform 0.2s;\
      }\
      #co2-panel.collapsed #co2-panel-toggle { transform: rotate(180deg); }\
      #co2-panel-body { padding: 8px 10px; }\
      #co2-panel.collapsed #co2-panel-body { display: none; }\
      .co2-row {\
        display: flex;\
        justify-content: space-between;\
        align-items: baseline;\
        padding: 2px 0;\
      }\
      .co2-label { color: #999; font-size: 11px; }\
      .co2-value { font-weight: 600; font-variant-numeric: tabular-nums; color: #fff; }\
      .co2-value-big { color: #81c784; font-size: 14px; }\
      .co2-divider { border: none; border-top: 1px solid rgba(255,255,255,0.06); margin: 4px 0; }\
      .co2-source {\
        font-size: 9px;\
        color: #666;\
        text-align: right;\
        padding-top: 2px;\
      }\
      .co2-dot {\
        display: inline-block;\
        width: 6px; height: 6px;\
        border-radius: 50%;\
        margin-right: 4px;\
        vertical-align: middle;\
      }\
      .co2-dot-on  { background: #81c784; }\
      .co2-dot-off { background: #666; }\
      #co2-selected {\
        margin-top: 6px;\
        padding: 6px 8px;\
        background: rgba(129,199,132,0.08);\
        border: 1px solid rgba(129,199,132,0.15);\
        border-radius: 6px;\
        font-size: 11px;\
        color: #ccc;\
        display: none;\
      }\
      #co2-selected .co2-sel-title { font-weight: 600; color: #81c784; margin-bottom: 3px; font-size: 11px; }\
      #co2-selected .co2-sel-row { display: flex; justify-content: space-between; padding: 1px 0; }\
      #co2-selected .co2-sel-label { color: #999; }\
      #co2-selected .co2-sel-value { color: #fff; font-weight: 500; }\
    ';
    document.head.appendChild(style);

    var panel = document.createElement('div');
    panel.id = 'co2-panel';
    panel.innerHTML = '\
      <div id="co2-panel-header">\
        <span id="co2-panel-title">\u{1F33F} CO\u2082 Tracker</span>\
        <span id="co2-panel-toggle">\u25BC</span>\
      </div>\
      <div id="co2-panel-body">\
        <div class="co2-row">\
          <span class="co2-label">All time</span>\
          <span class="co2-value co2-value-big" id="co2-alltime">—</span>\
        </div>\
        <div class="co2-row">\
          <span class="co2-label">Total distance</span>\
          <span class="co2-value" id="co2-alldist">—</span>\
        </div>\
        <div class="co2-source" id="co2-source"></div>\
        <hr class="co2-divider">\
        <div class="co2-row">\
          <span class="co2-label">Session CO\u2082</span>\
          <span class="co2-value" id="co2-session">0 kg</span>\
        </div>\
        <div class="co2-row">\
          <span class="co2-label">Session dist</span>\
          <span class="co2-value" id="co2-sessdist">0 km</span>\
        </div>\
        <hr class="co2-divider">\
        <div class="co2-row">\
          <span class="co2-label">Aircraft seen</span>\
          <span class="co2-value" id="co2-count">0</span>\
        </div>\
        <div id="co2-selected">\
          <div class="co2-sel-title" id="co2-sel-title">Selected</div>\
          <div class="co2-sel-row">\
            <span class="co2-sel-label">Distance</span>\
            <span class="co2-sel-value" id="co2-sel-dist">\u2014</span>\
          </div>\
          <div class="co2-sel-row">\
            <span class="co2-sel-label">CO\u2082 est.</span>\
            <span class="co2-sel-value" id="co2-sel-co2">\u2014</span>\
          </div>\
          <div class="co2-sel-row">\
            <span class="co2-sel-label">Factor</span>\
            <span class="co2-sel-value" id="co2-sel-factor">\u2014</span>\
          </div>\
        </div>\
      </div>\
    ';
    document.body.appendChild(panel);

    // Toggle collapse
    document.getElementById('co2-panel-header').addEventListener('click', function () {
      panel.classList.toggle('collapsed');
      try { localStorage.setItem(STORAGE_KEY + '_c', panel.classList.contains('collapsed') ? '1' : '0'); } catch (e) {}
    });
    try {
      if (localStorage.getItem(STORAGE_KEY + '_c') === '1') panel.classList.add('collapsed');
    } catch (e) {}
  }

  function updatePanel() {
    var el;

    // "All time" row — prefer server, fall back to localStorage + session
    if (serverOK && serverData) {
      el = document.getElementById('co2-alltime');
      if (el) el.textContent = fmtCO2(serverData.co2Kg);
      el = document.getElementById('co2-alldist');
      if (el) el.textContent = fmtDist(serverData.distKm);
      el = document.getElementById('co2-count');
      if (el) el.textContent = serverData.count;
      el = document.getElementById('co2-source');
      if (el) el.innerHTML = '<span class="co2-dot co2-dot-on"></span>Server \u00b7 updated ' + fmtAge(serverData.updated);
    } else {
      var allCO2 = lsCO2 + sessionCO2;
      var allDist = lsDist + sessionDist;
      var allCount = lsCount + sessionCount;
      el = document.getElementById('co2-alltime');
      if (el) el.textContent = fmtCO2(allCO2);
      el = document.getElementById('co2-alldist');
      if (el) el.textContent = fmtDist(allDist);
      el = document.getElementById('co2-count');
      if (el) el.textContent = allCount;
      el = document.getElementById('co2-source');
      if (el) el.innerHTML = '<span class="co2-dot co2-dot-off"></span>Browser only';
    }

    // Session row — always client-side
    el = document.getElementById('co2-session');
    if (el) el.textContent = fmtCO2(sessionCO2);
    el = document.getElementById('co2-sessdist');
    if (el) el.textContent = fmtDist(sessionDist);
  }

  // ======================== UI – Selected aircraft info ========================
  function updateSelectedInfo() {
    var selBlock = document.getElementById('co2-selected');
    if (!selBlock) return;

    var sp = (typeof SelectedPlane !== 'undefined') ? SelectedPlane : null;
    if (!sp || !tracked[sp.icao]) {
      selBlock.style.display = 'none';
      return;
    }

    var ac = tracked[sp.icao];
    selBlock.style.display = 'block';

    var title = sp.flight ? sp.flight.trim() : sp.icao.toUpperCase();
    if (ac.type) title += ' (' + ac.type + ')';
    document.getElementById('co2-sel-title').textContent = title;
    document.getElementById('co2-sel-dist').textContent = fmtDist(ac.distKm);

    var factor = getEmissionFactor(ac.type, ac.wtc, ac.cat);
    if (factor !== null && ac.co2Kg > 0) {
      document.getElementById('co2-sel-co2').textContent = fmtCO2(ac.co2Kg);
      var src = ac.type && EF[ac.type] !== undefined ? ac.type : (ac.wtc ? 'WTC-' + ac.wtc : 'cat');
      document.getElementById('co2-sel-factor').textContent = factor.toFixed(1) + ' kg/km (' + src + ')';
    } else {
      document.getElementById('co2-sel-co2').textContent = 'Unknown type';
      document.getElementById('co2-sel-factor').textContent = '\u2014';
    }
  }

  // ======================== Init ========================
  function init() {
    loadLS();
    createPanel();
    fetchServerData();
    setInterval(tick, UPDATE_INTERVAL);
    setInterval(fetchServerData, SERVER_POLL);
    // Periodic localStorage save (fallback)
    setInterval(saveLS, 30000);
    console.log('[co2tracker] CO\u2082 tracker initialised (hybrid mode)');
  }

  function waitForTar1090() {
    if (typeof g !== 'undefined' && g.planes) {
      init();
    } else {
      setTimeout(waitForTar1090, 500);
    }
  }

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', waitForTar1090);
  } else {
    waitForTar1090();
  }

})();
