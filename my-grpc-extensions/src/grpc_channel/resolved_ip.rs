//! `scheme://<server-name>@<ip>[:port]` — a url that names both the host the
//! request is for and the address to reach it at, so no DNS lookup happens.
//!
//! Only the ip is taken out of the url. The server name stays the host, so it is
//! what tonic writes into h2's `:authority` — and `:authority` is what a reverse
//! proxy listening on that ip routes the call by. The ip is used solely to open
//! the socket.

use std::borrow::Cow;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// Splits `https://domain.com@15.0.0.5:8443` into `https://domain.com:8443` and
/// `15.0.0.5`. A url with no '@' in its authority comes back unchanged, with no
/// ip.
///
/// Unix-socket urls never reach here — they are detected by their leading '/' or
/// '~/' before a `GrpcConnectUrl` is built as tcp — so a '@' in a socket file
/// name is not this function's problem.
pub fn extract_resolved_ip(url: &str) -> Result<(Cow<'_, str>, Option<IpAddr>), String> {
    let authority_start = match url.find("://") {
        Some(index) => index + 3,
        None => 0,
    };

    let rest = &url[authority_start..];
    // '@' further on belongs to the path or the query, not to the authority.
    let authority_len = rest
        .find(|c| matches!(c, '/' | '?' | '#'))
        .unwrap_or(rest.len());
    let authority = &rest[..authority_len];

    let Some((server_name, ip_and_port)) = authority.split_once('@') else {
        return Ok((Cow::Borrowed(url), None));
    };

    if server_name.is_empty() {
        return Err(invalid_url(url, "the server name before '@' is empty"));
    }

    if server_name.contains(':') {
        return Err(invalid_url(
            url,
            "the part before '@' must be a bare server name; the port goes after the ip",
        ));
    }

    let Some((ip, port)) = parse_ip_and_port(ip_and_port) else {
        return Err(invalid_url(
            url,
            "expected an ip address after '@' (ipv6 in brackets), optionally followed by :port",
        ));
    };

    let mut result = String::with_capacity(url.len());
    result.push_str(&url[..authority_start]);
    result.push_str(server_name);
    result.push_str(port);
    result.push_str(&rest[authority_len..]);

    Ok((Cow::Owned(result), Some(ip)))
}

/// `15.0.0.5`, `15.0.0.5:8443`, `[::1]`, `[::1]:8443` → the ip and the `:port`
/// suffix as written (empty when there is none).
fn parse_ip_and_port(src: &str) -> Option<(IpAddr, &str)> {
    let (ip, port) = match src.strip_prefix('[') {
        Some(bracketed) => {
            let end = bracketed.find(']')?;
            let ip: Ipv6Addr = bracketed[..end].parse().ok()?;
            (IpAddr::V6(ip), &bracketed[end + 1..])
        }
        None => {
            let ip_len = src.find(':').unwrap_or(src.len());
            let ip: Ipv4Addr = src[..ip_len].parse().ok()?;
            (IpAddr::V4(ip), &src[ip_len..])
        }
    };

    if !port.is_empty() {
        port.strip_prefix(':')?.parse::<u16>().ok()?;
    }

    Some((ip, port))
}

fn invalid_url(url: &str, reason: &str) -> String {
    format!("Invalid url '{}': {}", url, reason)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extract(url: &str) -> (String, Option<IpAddr>) {
        let (url, ip) = extract_resolved_ip(url).unwrap();
        (url.into_owned(), ip)
    }

    #[test]
    fn server_name_and_ipv4() {
        assert_eq!(
            extract("https://domain.com@15.0.0.5"),
            (
                "https://domain.com".to_string(),
                Some("15.0.0.5".parse().unwrap())
            )
        );
    }

    #[test]
    fn port_after_the_ip_stays_with_the_url() {
        assert_eq!(
            extract("https://domain.com@15.0.0.5:8443"),
            (
                "https://domain.com:8443".to_string(),
                Some("15.0.0.5".parse().unwrap())
            )
        );
    }

    #[test]
    fn ipv6_in_brackets() {
        assert_eq!(
            extract("https://domain.com@[2001:db8::1]:8443"),
            (
                "https://domain.com:8443".to_string(),
                Some("2001:db8::1".parse().unwrap())
            )
        );
        assert_eq!(
            extract("http://domain.com@[::1]"),
            ("http://domain.com".to_string(), Some("::1".parse().unwrap()))
        );
    }

    #[test]
    fn a_url_without_an_ip_is_left_alone() {
        for url in [
            "https://domain.com:8443",
            "http://localhost:5000",
            "https://domain.com/users/@me",
        ] {
            let (result, ip) = extract_resolved_ip(url).unwrap();
            assert!(matches!(result, Cow::Borrowed(_)), "{url}");
            assert_eq!(result, url);
            assert_eq!(ip, None, "{url}");
        }
    }

    #[test]
    fn invalid_forms_are_rejected() {
        for url in [
            "https://@15.0.0.5",
            "https://domain.com:443@15.0.0.5",
            "https://user:password@15.0.0.5",
            "https://domain.com@backend.internal",
            "https://domain.com@15.0.0.5:abc",
            "https://domain.com@15.0.0.5:99999",
            "https://domain.com@::1",
            "https://domain.com@[::1",
            "https://a@b@15.0.0.5",
        ] {
            assert!(
                extract_resolved_ip(url).is_err(),
                "{url} must be rejected"
            );
        }
    }
}
