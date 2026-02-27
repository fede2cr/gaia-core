//! Discover remote Gaia capture nodes on the LAN via mDNS (avahi-browse).
//!
//! Runs `avahi-browse --all -p -r -t` to discover and resolve every mDNS
//! service on the LAN, then filters for Gaia-related entries by **service
//! type** and **instance name prefix**.
//!
//! Key implementation detail: the `-t` (terminate) flag is critical.
//! Without it avahi-browse runs forever and must be killed; SIGKILL does
//! not let libc flush its internal stdout buffer, so small outputs
//! (common on quiet networks) are lost entirely.  With `-t` the process
//! exits naturally after the initial scan, ensuring stdout is flushed.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;

/// A remote capture service discovered via mDNS.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MdnsNode {
    /// The mDNS service type (e.g. `_adsbbeast._tcp`).
    pub service_type: String,
    /// Instance name advertised by the node.
    pub instance: String,
    /// Hostname or IP address.
    pub host: String,
    /// TCP port the service listens on.
    pub port: u16,
    /// Which Gaia project this belongs to.
    pub project_slug: String,
}

/// Known mDNS service types → Gaia project slug mapping.
///
/// `avahi-browse --all` translates well-known types to human-readable
/// names (e.g. `_http._tcp` → `"Web Site"`) so we include both forms
/// where applicable.
const SERVICE_TYPES: &[(&str, &str)] = &[
    // gaia-radio
    ("_adsbbeast._tcp", "radio"),
    ("_readsb._tcp", "radio"),
    // gaia-audio
    ("_gaia-audio-capture._tcp", "audio"),
    ("_gaia-audio-processing._tcp", "audio"),
    ("_gaia-audio-web._tcp", "audio"),
    // Global Meteor Network
    ("_rms._tcp", "gmn"),
];

/// Instance name prefixes → Gaia project slug mapping.
/// This catches services even if they use an unexpected service type
/// (e.g. BirdNET-Pi advertising as `_http._tcp` / `"Web Site"`).
const INSTANCE_PREFIXES: &[(&str, &str)] = &[
    // gaia-radio
    ("gaia-radio-", "radio"),
    // gaia-audio
    ("gaia-audio-", "audio"),
    ("capture-", "audio"),
    ("processing-", "audio"),
    ("web-", "audio"),
    // BirdNET instances (detected as gaia-audio capture sources)
    ("birdnet", "audio"),
    // GMN / RMS
    ("gaia-gmn-", "gmn"),
    ("rms-", "gmn"),
];

/// Maximum time to wait for `avahi-browse -t` to complete its scan.
/// This is a safety net — with `-t` the process usually finishes in a
/// few seconds.  We set it generously for slow or overlay networks
/// (e.g. ZeroTier).
const BROWSE_TIMEOUT: Duration = Duration::from_secs(15);

/// Browse the LAN for all Gaia mDNS services.
pub async fn discover_all() -> Vec<MdnsNode> {
    match browse_all().await {
        Ok(nodes) => nodes,
        Err(e) => {
            tracing::warn!("mDNS discovery failed: {e}");
            Vec::new()
        }
    }
}

/// Run `avahi-browse --all -p -r -t` and parse the resolved output.
///
/// Flags:
/// - `--all`  browse every service type on the network
/// - `-p`     parseable semicolon-delimited output
/// - `-r`     resolve (include address + port)
/// - `-t`     **terminate** after the initial scan — crucial so the
///            process exits naturally and flushes its stdout buffer
async fn browse_all() -> Result<Vec<MdnsNode>, String> {
    // Sanity-check: can we talk to avahi at all?
    let sanity = Command::new("avahi-browse")
        .args(["-a", "-t", "-p"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| format!("avahi-browse sanity check failed: {e}"))?;

    let sanity_out = String::from_utf8_lossy(&sanity.stdout);
    let sanity_lines = sanity_out.lines().count();
    if sanity_lines == 0 && !sanity.status.success() {
        let stderr = String::from_utf8_lossy(&sanity.stderr);
        tracing::warn!(
            "avahi-browse returned no output — is avahi-daemon reachable? stderr: {stderr}"
        );
    } else {
        tracing::debug!("avahi-browse sanity check OK ({sanity_lines} lines)");
    }

    // Main browse: all service types, parsable, resolved, terminate.
    let mut child = Command::new("avahi-browse")
        .args(["--all", "-p", "-r", "-t"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("avahi-browse spawn error: {e}"))?;

    // Take the stdout/stderr handles so we can read them after killing.
    let child_stdout = child.stdout.take();
    let child_stderr = child.stderr.take();

    // With `-t` avahi-browse terminates naturally after the initial scan,
    // which properly flushes stdout.  The timeout is a safety net for
    // networks where resolution hangs.
    let timed_out = match tokio::time::timeout(BROWSE_TIMEOUT, child.wait()).await {
        Ok(Ok(status)) => {
            tracing::debug!("avahi-browse exited with {status}");
            false
        }
        Ok(Err(e)) => {
            tracing::warn!("avahi-browse wait error: {e}");
            false
        }
        Err(_) => {
            tracing::warn!(
                "avahi-browse did not terminate within {sec}s — killing",
                sec = BROWSE_TIMEOUT.as_secs()
            );
            let _ = child.kill().await;
            let _ = child.wait().await;
            true
        }
    };

    // Read all captured output.
    let stdout = if let Some(mut out) = child_stdout {
        let mut buf = Vec::new();
        tokio::io::AsyncReadExt::read_to_end(&mut out, &mut buf)
            .await
            .ok();
        String::from_utf8_lossy(&buf).into_owned()
    } else {
        String::new()
    };

    if let Some(mut err) = child_stderr {
        let mut buf = Vec::new();
        tokio::io::AsyncReadExt::read_to_end(&mut err, &mut buf)
            .await
            .ok();
        let stderr = String::from_utf8_lossy(&buf);
        if !stderr.is_empty() {
            tracing::debug!("avahi-browse stderr: {stderr}");
        }
    }

    let total_lines = stdout.lines().count();
    let resolved_lines = stdout.lines().filter(|l| l.starts_with('=')).count();
    tracing::info!(
        "avahi-browse output: {total_lines} total lines, {resolved_lines} resolved entries{}",
        if timed_out { " (timed out)" } else { "" }
    );

    // ── Parse resolved entries ──────────────────────────────────────
    // Parsable format:  =;iface;protocol;name;type;domain;hostname;addr;port;txt
    //
    // A service can appear multiple times (IPv4 + IPv6, multiple
    // interfaces).  We keep only Gaia-related entries and prefer IPv4
    // per (instance, type).

    let mut best: HashMap<(String, String), MdnsNode> = HashMap::new();

    for line in stdout.lines() {
        if !line.starts_with('=') {
            continue;
        }
        let fields: Vec<&str> = line.split(';').collect();
        if fields.len() < 9 {
            continue;
        }

        let protocol = fields[2]; // "IPv4" or "IPv6"
        let instance_raw = fields[3];
        let stype = fields[4].to_string();
        let host = fields[7].to_string();
        let port = fields[8].parse::<u16>().unwrap_or(0);

        // Decode avahi's `\DDD` octal escapes (e.g. `\032` → space).
        let instance = decode_avahi_escapes(instance_raw);

        // Determine the project slug from service type OR instance name.
        let project_slug = classify(&instance, &stype);
        let Some(project_slug) = project_slug else {
            tracing::debug!("mDNS: skipping non-Gaia service: {instance} ({stype})");
            continue;
        };

        let key = (instance.clone(), stype.clone());

        // Prefer IPv4 over IPv6 (avoids link-local %iface issues).
        if let Some(existing) = best.get(&key) {
            if protocol == "IPv4" && existing.host.contains(':') {
                tracing::debug!(
                    "mDNS: upgrading {instance} ({stype}) from IPv6 to IPv4 ({host})"
                );
            } else {
                continue; // keep existing
            }
        } else {
            tracing::info!(
                "mDNS discovered: {instance} ({stype}) at {host}:{port} [{protocol}]"
            );
        }

        best.insert(
            key,
            MdnsNode {
                service_type: stype,
                instance,
                host,
                port,
                project_slug,
            },
        );
    }

    let nodes: Vec<MdnsNode> = best.into_values().collect();
    tracing::info!("mDNS discovery complete: {} Gaia node(s) found", nodes.len());
    Ok(nodes)
}

/// Classify a discovered service as belonging to a Gaia project.
///
/// Returns `Some(slug)` if it matches a known service type or instance
/// name prefix, `None` otherwise.
fn classify(instance: &str, service_type: &str) -> Option<String> {
    // 1. Match by service type (most authoritative).
    for &(stype, slug) in SERVICE_TYPES {
        if service_type == stype {
            return Some(slug.to_string());
        }
    }
    // 2. Fall back to instance name prefix.
    let lower = instance.to_lowercase();
    for &(prefix, slug) in INSTANCE_PREFIXES {
        if lower.starts_with(prefix) {
            return Some(slug.to_string());
        }
    }
    None
}

/// Decode avahi's `\DDD` octal escape sequences in service instance names.
///
/// In avahi-browse's parseable output, special characters are encoded as
/// a backslash followed by exactly three decimal digits (e.g. `\032` for
/// space).  This function replaces those sequences with the actual
/// character.
fn decode_avahi_escapes(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 3 < bytes.len() {
            // Try to parse the next 3 bytes as a decimal number 0-255.
            if let Ok(code) = std::str::from_utf8(&bytes[i + 1..i + 4])
                .ok()
                .and_then(|s| s.parse::<u8>().ok())
                .ok_or(())
            {
                out.push(code as char);
                i += 4;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}
