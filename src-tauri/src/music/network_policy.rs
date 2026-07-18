use std::{
    collections::HashSet,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, ToSocketAddrs},
    time::Duration,
};

use reqwest::{Response, StatusCode, Url, header::LOCATION};
use tokio_util::sync::CancellationToken;

const DNS_TIMEOUT: Duration = Duration::from_secs(10);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const READ_TIMEOUT: Duration = Duration::from_secs(45);
const MAX_REDIRECTS: usize = 5;

pub(crate) enum MediaGetError {
    Cancelled,
    Network,
}

/// Fetch an untrusted media URL without allowing DNS rebinding, redirects to
/// local networks, proxy inheritance, or URL credentials.
pub(crate) async fn safe_media_get(
    mut url: Url,
    cancel: &CancellationToken,
) -> Result<Response, MediaGetError> {
    let mut redirects = 0_usize;

    loop {
        validate_url(&url)?;
        let host = url.host_str().ok_or(MediaGetError::Network)?.to_owned();
        let port = url.port_or_known_default().ok_or(MediaGetError::Network)?;
        let addresses = resolve_once(host.clone(), port, cancel).await?;
        if addresses.is_empty() || addresses.iter().any(|addr| !is_public_ip(addr.ip())) {
            return Err(MediaGetError::Network);
        }

        let pinned_ips = addresses
            .iter()
            .map(|address| normalize_ip(address.ip()))
            .collect::<HashSet<_>>();
        let client = reqwest::Client::builder()
            .https_only(true)
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(CONNECT_TIMEOUT)
            .read_timeout(READ_TIMEOUT)
            .tls_backend_rustls()
            .resolve_to_addrs(&host, &addresses)
            .build()
            .map_err(|_| MediaGetError::Network)?;

        let response = tokio::select! {
            _ = cancel.cancelled() => return Err(MediaGetError::Cancelled),
            result = client.get(url.clone()).send() => {
                result.map_err(|_| MediaGetError::Network)?
            }
        };
        let remote_ip = response
            .remote_addr()
            .map(|address| normalize_ip(address.ip()))
            .ok_or(MediaGetError::Network)?;
        if !pinned_ips.contains(&remote_ip) {
            return Err(MediaGetError::Network);
        }

        if is_followable_redirect(response.status()) {
            if redirects >= MAX_REDIRECTS {
                return Err(MediaGetError::Network);
            }
            let location = response
                .headers()
                .get(LOCATION)
                .and_then(|value| value.to_str().ok())
                .ok_or(MediaGetError::Network)?;
            let next = url.join(location).map_err(|_| MediaGetError::Network)?;
            validate_url(&next)?;
            if url.scheme() == "https" && next.scheme() != "https" {
                return Err(MediaGetError::Network);
            }
            redirects += 1;
            url = next;
            continue;
        }

        return Ok(response);
    }
}

async fn resolve_once(
    host: String,
    port: u16,
    cancel: &CancellationToken,
) -> Result<Vec<SocketAddr>, MediaGetError> {
    let lookup = tokio::task::spawn_blocking(move || {
        (host.as_str(), port)
            .to_socket_addrs()
            .map(|addresses| addresses.collect::<Vec<_>>())
    });
    let addresses = tokio::select! {
        _ = cancel.cancelled() => return Err(MediaGetError::Cancelled),
        result = tokio::time::timeout(DNS_TIMEOUT, lookup) => {
            result
                .map_err(|_| MediaGetError::Network)?
                .map_err(|_| MediaGetError::Network)?
                .map_err(|_| MediaGetError::Network)?
        }
    };

    let mut unique = HashSet::new();
    Ok(addresses
        .into_iter()
        .filter(|address| unique.insert(*address))
        .collect())
}

fn validate_url(url: &Url) -> Result<(), MediaGetError> {
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || serialized_authority(url).is_some_and(|authority| authority.contains('@'))
    {
        return Err(MediaGetError::Network);
    }
    Ok(())
}

fn serialized_authority(url: &Url) -> Option<&str> {
    let remainder = url.as_str().strip_prefix("https://")?;
    let end = remainder.find(['/', '?', '#']).unwrap_or(remainder.len());
    Some(&remainder[..end])
}

fn is_followable_redirect(status: StatusCode) -> bool {
    matches!(
        status,
        StatusCode::MOVED_PERMANENTLY
            | StatusCode::FOUND
            | StatusCode::SEE_OTHER
            | StatusCode::TEMPORARY_REDIRECT
            | StatusCode::PERMANENT_REDIRECT
    )
}

fn normalize_ip(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(ipv6) => ipv6
            .to_ipv4_mapped()
            .map(IpAddr::V4)
            .unwrap_or(IpAddr::V6(ipv6)),
        other => other,
    }
}

fn is_public_ip(ip: IpAddr) -> bool {
    match normalize_ip(ip) {
        IpAddr::V4(ipv4) => is_public_ipv4(ipv4),
        IpAddr::V6(ipv6) => is_public_ipv6(ipv6),
    }
}

fn is_public_ipv4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    if a == 0
        || a == 10
        || a == 127
        || a >= 224
        || (a == 100 && (64..=127).contains(&b))
        || (a == 169 && b == 254)
        || (a == 172 && (16..=31).contains(&b))
        || (a == 192 && b == 0 && c == 0)
        || (a == 192 && b == 0 && c == 2)
        || (a == 192 && b == 88 && c == 99)
        || (a == 192 && b == 168)
        || (a == 198 && (b == 18 || b == 19))
        || (a == 198 && b == 51 && c == 100)
        || (a == 203 && b == 0 && c == 113)
    {
        return false;
    }
    true
}

fn is_public_ipv6(ip: Ipv6Addr) -> bool {
    let segments = ip.segments();
    let global_unicast = segments[0] & 0xe000 == 0x2000;
    let protocol_assignments = segments[0] == 0x2001 && segments[1] & 0xfe00 == 0;
    let documentation = segments[0] == 0x2001 && segments[1] == 0x0db8;
    let six_to_four = segments[0] == 0x2002;
    let former_sixbone = segments[0] == 0x3ffe;
    let documentation_v2 = segments[0] == 0x3fff && segments[1] & 0xf000 == 0;

    global_unicast
        && !protocol_assignments
        && !documentation
        && !six_to_four
        && !former_sixbone
        && !documentation_v2
}
