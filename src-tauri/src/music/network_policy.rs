use std::{
    collections::HashSet,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, ToSocketAddrs},
    sync::{LazyLock, Mutex},
    time::Duration,
    time::Instant,
};

use reqwest::{Response, StatusCode, Url, header::LOCATION};
use tokio_util::sync::CancellationToken;

const DNS_TIMEOUT: Duration = Duration::from_secs(10);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const READ_TIMEOUT: Duration = Duration::from_secs(45);
const MAX_REDIRECTS: usize = 5;
const FAKE_IP_PROBE_HOST: &str = "music.gdstudio.xyz";
const FAKE_IP_PROBE_PORT: u16 = 443;
const FAKE_IP_CACHE_TTL: Duration = Duration::from_secs(60);

static FAKE_IP_DNS_CACHE: LazyLock<Mutex<Option<(Instant, bool)>>> =
    LazyLock::new(|| Mutex::new(None));

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
        let contains_fake_ip = addresses.iter().any(|addr| is_fake_ip(addr.ip()));
        let fake_ip_dns_active = if contains_fake_ip && host.parse::<IpAddr>().is_err() {
            detect_fake_ip_dns(cancel).await?
        } else {
            false
        };
        if !resolved_addresses_allowed(&url, &addresses, fake_ip_dns_active) {
            log::warn!(
                "媒体地址解析结果被安全策略拒绝 host={} fake_ip_dns_active={}",
                host,
                fake_ip_dns_active
            );
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

async fn detect_fake_ip_dns(cancel: &CancellationToken) -> Result<bool, MediaGetError> {
    if let Some(active) = cached_fake_ip_dns_state() {
        return Ok(active);
    }

    let addresses = resolve_once(FAKE_IP_PROBE_HOST.to_owned(), FAKE_IP_PROBE_PORT, cancel).await?;
    let active = !addresses.is_empty() && addresses.iter().all(|address| is_fake_ip(address.ip()));
    *FAKE_IP_DNS_CACHE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some((Instant::now(), active));
    if active {
        log::info!("检测到透明代理 Fake-IP DNS，允许经 198.18.0.0/15 获取媒体资源");
    }
    Ok(active)
}

fn cached_fake_ip_dns_state() -> Option<bool> {
    FAKE_IP_DNS_CACHE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .as_ref()
        .filter(|(checked_at, _)| checked_at.elapsed() < FAKE_IP_CACHE_TTL)
        .map(|(_, active)| *active)
}

fn resolved_addresses_allowed(
    url: &Url,
    addresses: &[SocketAddr],
    fake_ip_dns_active: bool,
) -> bool {
    if addresses.is_empty() {
        return false;
    }
    let fake_ip_alias_allowed = fake_ip_dns_active
        && url
            .host_str()
            .is_some_and(|host| host.parse::<IpAddr>().is_err());
    addresses.iter().all(|address| {
        let ip = address.ip();
        is_public_ip(ip) || (fake_ip_alias_allowed && is_fake_ip(ip))
    })
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

fn is_fake_ip(ip: IpAddr) -> bool {
    matches!(normalize_ip(ip), IpAddr::V4(ipv4) if {
        let [a, b, _, _] = ipv4.octets();
        a == 198 && (b == 18 || b == 19)
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    fn socket(ip: &str) -> SocketAddr {
        SocketAddr::new(ip.parse().expect("test IP is valid"), 443)
    }

    #[test]
    fn recognizes_only_the_benchmark_fake_ip_range() {
        assert!(is_fake_ip("198.18.0.1".parse().unwrap()));
        assert!(is_fake_ip("198.19.255.254".parse().unwrap()));
        assert!(!is_fake_ip("198.17.255.255".parse().unwrap()));
        assert!(!is_fake_ip("198.20.0.1".parse().unwrap()));
        assert!(!is_fake_ip("2001:db8::1".parse().unwrap()));
    }

    #[test]
    fn allows_fake_ip_aliases_only_for_domain_names_in_detected_mode() {
        let domain = Url::parse("https://media.example/audio.mp3").unwrap();
        let literal = Url::parse("https://198.18.0.10/audio.mp3").unwrap();
        let fake = [socket("198.18.0.10")];

        assert!(!resolved_addresses_allowed(&domain, &fake, false));
        assert!(resolved_addresses_allowed(&domain, &fake, true));
        assert!(!resolved_addresses_allowed(&literal, &fake, true));
    }

    #[test]
    fn fake_ip_mode_never_relaxes_private_network_blocking() {
        let url = Url::parse("https://media.example/audio.mp3").unwrap();

        assert!(!resolved_addresses_allowed(
            &url,
            &[socket("192.168.1.2")],
            true
        ));
        assert!(!resolved_addresses_allowed(
            &url,
            &[socket("198.18.0.10"), socket("127.0.0.1")],
            true
        ));
        assert!(resolved_addresses_allowed(
            &url,
            &[socket("8.8.8.8")],
            false
        ));
    }
}
