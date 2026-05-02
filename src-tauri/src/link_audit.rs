// Modulo: src-tauri/src/link_audit.rs
// Descricao: Public URL extraction + audit (HEAD/GET probe + private-network
// IP blocklist) extracted from lib.rs in v0.3.31 per `docs/code-split-plan.md`
// migration step 5.
//
// What's here (9 functions):
//   - `run_link_audit` — top-level entry that builds the HTTP client (15s
//     timeout, 5-redirect cap), extracts public URLs from the operator's
//     text, and probes each one. Returns aggregate counts (urls_found /
//     checked / ok / failed) + per-row tone.
//   - `extract_public_urls` — regex-based URL extraction with cap of 30
//     unique URLs out of the first 80 matches; trailing punctuation
//     (`.,;:`) stripped.
//   - `is_public_http_url` — schema gate (http/https only) + private-network
//     IP filter (loopback, private ranges, link-local, multicast, etc.).
//   - `is_blocked_link_audit_ip`, `is_blocked_link_audit_ipv4`,
//     `is_blocked_link_audit_ipv6` — RFC 1918 / RFC 6890 / RFC 4193 / RFC
//     5737 / RFC 6598 ranges + IPv6 reserved/link-local/ULA/multicast.
//   - `probe_public_url` — HEAD probe with 405/403 fallback to GET.
//   - `probe_public_url_with_get` — fallback GET probe.
//   - `link_audit_row` — small `LinkAuditRow` builder with sanitization.
//
// What stays in lib.rs (consumed via `pub(crate)` imports):
//   - `LinkAuditRequest`, `LinkAuditRow`, `LinkAuditResult` (the structs;
//     v0.3.31 upgrades fields to pub(crate)).
//   - `sanitize_short`, `sanitize_text` (already pub(crate)).
//
// v0.3.31 is a pure move: every signature, regex, format string, and IP
// range is identical to the v0.3.30 lib.rs source (commit 91aa863).

use std::collections::BTreeSet;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::time::Duration;

use regex::Regex;
use reqwest::{blocking::Client, redirect::Policy, Url};

use crate::{sanitize_short, sanitize_text, LinkAuditResult, LinkAuditRow};

pub(crate) fn run_link_audit(text: &str) -> LinkAuditResult {
    let urls = extract_public_urls(text);
    let client = match Client::builder()
        .timeout(Duration::from_secs(15))
        .redirect(Policy::limited(5))
        .user_agent(format!(
            "Maestro Editorial AI/{}",
            env!("CARGO_PKG_VERSION")
        ))
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            return LinkAuditResult {
                urls_found: urls.len(),
                checked: 0,
                ok: 0,
                failed: urls.len(),
                rows: vec![link_audit_row(
                    "http-client",
                    format!("cliente HTTP falhou: {error}"),
                    "error",
                )],
            };
        }
    };

    let rows = urls
        .iter()
        .map(|url| probe_public_url(&client, url))
        .collect::<Vec<_>>();
    let ok = rows.iter().filter(|row| row.tone == "ok").count();
    let failed = rows
        .iter()
        .filter(|row| row.tone == "error" || row.tone == "blocked")
        .count();

    LinkAuditResult {
        urls_found: urls.len(),
        checked: rows.len(),
        ok,
        failed,
        rows,
    }
}

pub(crate) fn extract_public_urls(text: &str) -> Vec<String> {
    let Some(regex) = Regex::new(r#"https?://[^\s<>"')\]]+"#).ok() else {
        return Vec::new();
    };

    let mut urls = BTreeSet::new();
    for matched in regex.find_iter(text).take(80) {
        let cleaned = matched
            .as_str()
            .trim_end_matches(['.', ',', ';', ':'])
            .to_string();
        if is_public_http_url(&cleaned) {
            urls.insert(cleaned);
        }
        if urls.len() >= 30 {
            break;
        }
    }
    urls.into_iter().collect()
}

pub(crate) fn is_public_http_url(value: &str) -> bool {
    let Ok(url) = Url::parse(value) else {
        return false;
    };
    if !matches!(url.scheme(), "http" | "https") {
        return false;
    }
    let Some(host) = url.host_str().map(|host| host.to_ascii_lowercase()) else {
        return false;
    };

    if matches!(host.as_str(), "localhost" | "localhost.localdomain")
        || host.ends_with(".localhost")
        || host.ends_with(".local")
    {
        return false;
    }

    let host_for_ip = host.trim_start_matches('[').trim_end_matches(']');
    if let Ok(ip) = host_for_ip.parse::<IpAddr>() {
        return !is_blocked_link_audit_ip(ip);
    }

    true
}

fn is_blocked_link_audit_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ipv4) => is_blocked_link_audit_ipv4(ipv4),
        IpAddr::V6(ipv6) => is_blocked_link_audit_ipv6(ipv6),
    }
}

fn is_blocked_link_audit_ipv4(ip: Ipv4Addr) -> bool {
    let octets = ip.octets();
    octets[0] == 0
        || octets[0] == 10
        || octets[0] == 127
        || (octets[0] == 100 && (64..=127).contains(&octets[1]))
        || (octets[0] == 169 && octets[1] == 254)
        || (octets[0] == 172 && (16..=31).contains(&octets[1]))
        || (octets[0] == 192 && octets[1] == 168)
        || (octets[0] == 192 && octets[1] == 0 && octets[2] == 0)
        || (octets[0] == 192 && octets[1] == 0 && octets[2] == 2)
        || (octets[0] == 198 && (18..=19).contains(&octets[1]))
        || (octets[0] == 198 && octets[1] == 51 && octets[2] == 100)
        || (octets[0] == 203 && octets[1] == 0 && octets[2] == 113)
        || octets[0] >= 224
}

fn is_blocked_link_audit_ipv6(ip: Ipv6Addr) -> bool {
    if ip.is_loopback() || ip.is_unspecified() {
        return true;
    }

    if let Some(mapped) = ip.to_ipv4_mapped() {
        return is_blocked_link_audit_ipv4(mapped);
    }

    let segments = ip.segments();
    if segments[0..5].iter().all(|segment| *segment == 0)
        && (segments[5] == 0 || segments[5] == 0xffff)
    {
        let [a, b] = segments[6].to_be_bytes();
        let [c, d] = segments[7].to_be_bytes();
        return is_blocked_link_audit_ipv4(Ipv4Addr::new(a, b, c, d));
    }

    let first_segment = segments[0];
    (first_segment & 0xfe00) == 0xfc00
        || (first_segment & 0xffc0) == 0xfe80
        || (first_segment & 0xff00) == 0xff00
        || (segments[0] == 0x2001 && segments[1] == 0x0db8)
}

fn probe_public_url(client: &Client, url: &str) -> LinkAuditRow {
    let head = client.head(url).send();
    match head {
        Ok(response) if response.status().is_success() || response.status().is_redirection() => {
            link_audit_row(url, format!("HTTP {}", response.status().as_u16()), "ok")
        }
        Ok(response) if response.status().as_u16() == 405 || response.status().as_u16() == 403 => {
            probe_public_url_with_get(client, url)
        }
        Ok(response) => link_audit_row(
            url,
            format!("HTTP {}", response.status().as_u16()),
            if response.status().is_client_error() || response.status().is_server_error() {
                "error"
            } else {
                "warn"
            },
        ),
        Err(_) => probe_public_url_with_get(client, url),
    }
}

fn probe_public_url_with_get(client: &Client, url: &str) -> LinkAuditRow {
    match client.get(url).send() {
        Ok(response) if response.status().is_success() || response.status().is_redirection() => {
            link_audit_row(url, format!("HTTP {}", response.status().as_u16()), "ok")
        }
        Ok(response) => link_audit_row(
            url,
            format!("HTTP {}", response.status().as_u16()),
            if response.status().is_client_error() || response.status().is_server_error() {
                "error"
            } else {
                "warn"
            },
        ),
        Err(error) => link_audit_row(url, format!("falha HTTP: {error}"), "error"),
    }
}

fn link_audit_row(
    url: impl Into<String>,
    status: impl Into<String>,
    tone: impl Into<String>,
) -> LinkAuditRow {
    LinkAuditRow {
        url: sanitize_text(&url.into(), 240),
        status: sanitize_text(&status.into(), 160),
        tone: sanitize_short(&tone.into(), 16),
    }
}
