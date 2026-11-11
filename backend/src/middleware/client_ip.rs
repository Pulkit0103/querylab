//! Client IP resolution.
//!
//! `X-Forwarded-For` is only trusted for a configured number of proxy hops
//! (e.g. 1 behind a single load balancer). The address is taken from the
//! right, so a client cannot spoof it by sending its own header.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use axum::{
    extract::{ConnectInfo, FromRequestParts},
    http::{request::Parts, HeaderMap},
};

use crate::{errors::AppError, state::AppState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClientIp(pub IpAddr);

pub fn resolve(headers: &HeaderMap, peer: Option<IpAddr>, trusted_hops: usize) -> IpAddr {
    let fallback = peer.unwrap_or(IpAddr::V4(Ipv4Addr::UNSPECIFIED));
    if trusted_hops == 0 {
        return fallback;
    }
    let chain: Vec<IpAddr> = headers
        .get_all("x-forwarded-for")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(','))
        .filter_map(|s| s.trim().parse().ok())
        .collect();
    // With N trusted proxies, the client is the N-th address from the right.
    chain
        .len()
        .checked_sub(trusted_hops)
        .and_then(|i| chain.get(i).copied())
        .unwrap_or(fallback)
}

impl FromRequestParts<AppState> for ClientIp {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let peer = parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .map(|ci| ci.0.ip());
        Ok(ClientIp(resolve(
            &parts.headers,
            peer,
            state.config.trusted_proxy_hops,
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(xff: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert("x-forwarded-for", xff.parse().unwrap());
        h
    }

    #[test]
    fn ignores_header_without_trusted_proxies() {
        let peer: IpAddr = "10.0.0.5".parse().unwrap();
        assert_eq!(resolve(&headers("1.2.3.4"), Some(peer), 0), peer);
    }

    #[test]
    fn picks_from_the_right() {
        let peer: IpAddr = "10.0.0.5".parse().unwrap();
        // Client spoofs "6.6.6.6"; the real client was appended by our proxy.
        let h = headers("6.6.6.6, 203.0.113.9");
        assert_eq!(
            resolve(&h, Some(peer), 1),
            "203.0.113.9".parse::<IpAddr>().unwrap()
        );
        assert_eq!(
            resolve(&h, Some(peer), 2),
            "6.6.6.6".parse::<IpAddr>().unwrap()
        );
        // Fewer entries than hops: fall back to the socket peer.
        assert_eq!(resolve(&h, Some(peer), 3), peer);
    }
}
