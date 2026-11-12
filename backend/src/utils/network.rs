//! Outbound network policy for user-supplied database hosts (SSRF defence).
//!
//! Users can point QueryLab at any host, which makes the API a potential
//! proxy into its own network. By default every resolved address must be a
//! public unicast address. `ALLOW_PRIVATE_NETWORKS=true` lifts the address
//! restriction for local development and self-hosted deployments.
//!
//! Residual risk: DNS may change between this check and the TCP connect
//! (rebinding). Production deployments should also restrict egress at the
//! network layer.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use crate::errors::AppError;

/// Ports below this are refused: databases do not listen there, and it keeps
/// the API from being used to probe well-known services.
pub const MIN_PORT: u16 = 1024;

fn is_forbidden_v4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    ip.is_unspecified()          // 0.0.0.0
        || a == 0                // 0.0.0.0/8 "this network"
        || ip.is_loopback()      // 127.0.0.0/8
        || ip.is_private()       // 10/8, 172.16/12, 192.168/16
        || ip.is_link_local()    // 169.254/16 (cloud metadata)
        || (a == 100 && (64..=127).contains(&b)) // 100.64/10 CGNAT
        || (a == 192 && b == 0 && c == 0)        // 192.0.0.0/24 IETF protocol
        || (a == 192 && b == 0 && c == 2)        // TEST-NET-1
        || (a == 198 && (b == 18 || b == 19))    // 198.18/15 benchmarking
        || (a == 198 && b == 51 && c == 100)     // TEST-NET-2
        || (a == 203 && b == 0 && c == 113)      // TEST-NET-3
        || ip.is_multicast()     // 224/4
        || a >= 240 // 240/4 reserved + broadcast
}

fn is_forbidden_v6(ip: Ipv6Addr) -> bool {
    if let Some(v4) = ip.to_ipv4_mapped() {
        return is_forbidden_v4(v4);
    }
    let seg = ip.segments();
    ip.is_unspecified()
        || ip.is_loopback()
        || ip.is_multicast()
        || (seg[0] & 0xfe00) == 0xfc00 // fc00::/7 unique local
        || (seg[0] & 0xffc0) == 0xfe80 // fe80::/10 link local
        || (seg[0] & 0xffc0) == 0xfec0 // fec0::/10 deprecated site local
        || (seg[0] == 0x2001 && seg[1] == 0x0db8) // documentation
        || (seg[0] == 0x0064 && seg[1] == 0xff9b) // NAT64 (could reach v4 internals)
        || (seg[0] == 0x2002) // 6to4 embeds arbitrary IPv4
        || (seg[0] == 0x2001 && seg[1] == 0x0000) // Teredo
        || seg[..6] == [0, 0, 0, 0, 0, 0] // IPv4-compatible (deprecated)
}

/// Whether an address is outside the public internet.
pub fn is_forbidden_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_forbidden_v4(v4),
        IpAddr::V6(v6) => is_forbidden_v6(v6),
    }
}

fn is_internal_hostname(host: &str) -> bool {
    let h = host.trim_end_matches('.').to_ascii_lowercase();
    h == "localhost"
        || [
            ".localhost",
            ".local",
            ".internal",
            ".localdomain",
            ".lan",
            ".home.arpa",
        ]
        .iter()
        .any(|suffix| h.ends_with(suffix))
        || h == "metadata"
        || h.starts_with("metadata.")
}

fn parse_ip_literal(host: &str) -> Option<IpAddr> {
    let inner = host
        .strip_prefix('[')
        .and_then(|h| h.strip_suffix(']'))
        .unwrap_or(host);
    inner.parse().ok()
}

/// Cheap, offline check used when a connection is saved.
pub fn precheck(host: &str, port: u16, allow_private: bool) -> Result<(), AppError> {
    if port < MIN_PORT {
        return Err(AppError::validation(
            "port",
            format!("ports below {MIN_PORT} are not allowed"),
        ));
    }
    if allow_private {
        return Ok(());
    }
    if let Some(ip) = parse_ip_literal(host) {
        if is_forbidden_ip(ip) {
            return Err(AppError::validation(
                "host",
                "private, loopback and link-local addresses are not allowed",
            ));
        }
    } else if is_internal_hostname(host) {
        return Err(AppError::validation(
            "host",
            "internal hostnames are not allowed",
        ));
    }
    Ok(())
}

/// Resolves the host and verifies every address is allowed. Called right
/// before connecting.
pub async fn check_destination(host: &str, port: u16, allow_private: bool) -> Result<(), AppError> {
    precheck(host, port, allow_private).map_err(|e| match e {
        AppError::Validation(fields) => AppError::Forbidden(
            fields
                .first()
                .map(|f| format!("{}: {}", f.field, f.message))
                .unwrap_or_else(|| "destination not allowed".into()),
        ),
        other => other,
    })?;
    if allow_private {
        return Ok(());
    }
    let target = match parse_ip_literal(host) {
        Some(ip) => vec![ip],
        None => {
            let lookup = tokio::time::timeout(
                std::time::Duration::from_secs(5),
                tokio::net::lookup_host((host, port)),
            )
            .await
            .map_err(|_| AppError::TargetUnavailable(format!("DNS lookup for {host} timed out")))?
            .map_err(|_| AppError::TargetUnavailable(format!("could not resolve host {host}")))?;
            lookup.map(|sa| sa.ip()).collect()
        }
    };
    if target.is_empty() {
        return Err(AppError::TargetUnavailable(format!(
            "could not resolve host {host}"
        )));
    }
    if target.iter().any(|ip| is_forbidden_ip(*ip)) {
        tracing::warn!(%host, "blocked connection to non-public address");
        return Err(AppError::Forbidden(
            "the host resolves to a private or reserved address, which is not allowed".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forbidden_ranges() {
        for ip in [
            "0.0.0.0",
            "127.0.0.1",
            "10.1.2.3",
            "172.16.0.1",
            "172.31.255.255",
            "192.168.1.1",
            "169.254.169.254",
            "100.64.0.1",
            "198.18.0.1",
            "224.0.0.1",
            "255.255.255.255",
            "::1",
            "::",
            "fd00:ec2::254",
            "fe80::1",
            "::ffff:127.0.0.1",
            "::ffff:10.0.0.1",
            "64:ff9b::a00:1",
            "2002:7f00:1::",
        ] {
            assert!(
                is_forbidden_ip(ip.parse().unwrap()),
                "{ip} should be blocked"
            );
        }
        for ip in [
            "8.8.8.8",
            "1.1.1.1",
            "172.32.0.1",
            "100.128.0.1",
            "2606:4700::1111",
        ] {
            assert!(
                !is_forbidden_ip(ip.parse().unwrap()),
                "{ip} should be allowed"
            );
        }
    }

    #[test]
    fn precheck_rules() {
        assert!(precheck("db.example.com", 5432, false).is_ok());
        assert!(precheck("8.8.8.8", 6543, false).is_ok());
        for host in [
            "localhost",
            "LOCALHOST.",
            "db.internal",
            "printer.local",
            "metadata.google.internal",
            "127.0.0.1",
            "[::1]",
            "169.254.169.254",
        ] {
            assert!(precheck(host, 5432, false).is_err(), "{host}");
        }
        assert!(precheck("db.example.com", 22, false).is_err());
        assert!(
            precheck("db.example.com", 80, true).is_err(),
            "port rule always applies"
        );
        assert!(precheck("localhost", 5432, true).is_ok());
    }

    #[tokio::test]
    async fn check_destination_blocks_literal_and_localhost() {
        assert!(matches!(
            check_destination("127.0.0.1", 5432, false).await,
            Err(AppError::Forbidden(_))
        ));
        assert!(matches!(
            check_destination("localhost", 5432, false).await,
            Err(AppError::Forbidden(_))
        ));
        assert!(check_destination("127.0.0.1", 5432, true).await.is_ok());
    }
}
