use std::fmt::Debug;
use std::net::IpAddr;

pub enum GrpcConnectUrl {
    Tcp {
        /// Exactly what the caller passed - what logs and error messages name.
        raw: String,
        /// `raw` - over ssh, the part behind the tunnel - with a pinned ip taken
        /// out of the authority: `https://domain@10.0.0.7:8080` becomes
        /// `https://domain:8080`. This is the string tonic gets, so h2's
        /// `:authority` carries the server name and a reverse proxy standing on
        /// that ip can route the call by it. Without a pinned ip it is the
        /// source string unchanged.
        host: String,
        /// The ip the socket goes to, when the url pinned one. `Err` when the
        /// `server-name@ip` authority is malformed - the url is kept as it was
        /// and the error is reported when the channel is created, because a
        /// `GrpcConnectUrl` is built through an infallible `Into`.
        resolved_ip: Result<Option<IpAddr>, String>,
        #[cfg(all(unix, feature = "with-ssh"))]
        over_ssh: my_ssh::ssh_settings::OverSshConnectionSettings,
    },
    UnixSocket(String),
}

impl Debug for GrpcConnectUrl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Tcp {
                raw,
                host,
                resolved_ip,
                ..
            } => f
                .debug_struct("Tcp")
                .field("raw", raw)
                .field("host", host)
                .field("resolved_ip", resolved_ip)
                .finish(),
            Self::UnixSocket(arg0) => f.debug_tuple("UnixSocket").field(arg0).finish(),
        }
    }
}

impl GrpcConnectUrl {
    fn new_as_tcp(raw: String) -> Self {
        #[cfg(all(unix, feature = "with-ssh"))]
        let over_ssh = my_ssh::ssh_settings::OverSshConnectionSettings::parse(raw.as_str());

        let (host, resolved_ip) = {
            #[cfg(all(unix, feature = "with-ssh"))]
            let src = over_ssh.remote_resource_string.as_str();
            #[cfg(any(not(unix), not(feature = "with-ssh")))]
            let src = raw.as_str();

            match super::extract_resolved_ip(src) {
                Ok((host, resolved_ip)) => (host.into_owned(), Ok(resolved_ip)),
                Err(err) => (src.to_string(), Err(err)),
            }
        };

        Self::Tcp {
            host,
            resolved_ip,
            #[cfg(all(unix, feature = "with-ssh"))]
            over_ssh,
            raw,
        }
    }

    #[cfg(unix)]
    fn new_as_unix_socket(raw: String) -> Self {
        Self::UnixSocket(raw)
    }

    /// The url tonic connects with. A pinned ip is not part of it: the socket
    /// goes to the ip, `:authority` stays the server name.
    pub fn get_grpc_host(&self) -> &str {
        match self {
            Self::Tcp { host, .. } => host.as_str(),
            Self::UnixSocket(raw) => raw.as_str(),
        }
    }

    /// The ip the socket must go to instead of resolving the host, when the url
    /// pinned one with `server-name@ip`. `Err` carries why a url that looked
    /// like it pinned an ip could not be read.
    pub fn get_resolved_ip(&self) -> Result<Option<IpAddr>, &str> {
        match self {
            Self::Tcp { resolved_ip, .. } => match resolved_ip {
                Ok(resolved_ip) => Ok(*resolved_ip),
                Err(err) => Err(err.as_str()),
            },
            Self::UnixSocket(_) => Ok(None),
        }
    }

    #[cfg(all(unix, feature = "with-ssh"))]
    pub fn get_ssh_credentials(&self) -> Option<&std::sync::Arc<my_ssh::SshCredentials>> {
        match self {
            Self::Tcp { over_ssh, .. } => over_ssh.ssh_credentials.as_ref(),
            Self::UnixSocket(raw) => {
                panic!("Unix socket does not support ssh credentials: {}", raw)
            }
        }
    }

    #[cfg(all(unix, feature = "with-ssh"))]
    pub fn is_over_ssh(&self) -> bool {
        match self {
            Self::Tcp { over_ssh, .. } => over_ssh.ssh_credentials.is_some(),
            Self::UnixSocket(_) => false,
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Tcp { raw, .. } => raw,
            Self::UnixSocket(raw) => raw,
        }
    }

    pub fn is_grpc_tls_endpoint(&self) -> bool {
        let grpc_host = self.get_grpc_host();
        rust_extensions::str_utils::starts_with_case_insensitive(grpc_host, "https")
    }

    #[cfg(unix)]
    pub fn is_unix_socket(&self) -> bool {
        match self {
            Self::UnixSocket(_) => true,
            _ => false,
        }
    }

    #[cfg(not(unix))]
    pub fn is_unix_socket(&self) -> bool {
        false
    }
}

impl Into<GrpcConnectUrl> for String {
    fn into(self) -> GrpcConnectUrl {
        #[cfg(unix)]
        if self.starts_with("/") || self.starts_with("~/") {
            return GrpcConnectUrl::new_as_unix_socket(self);
        }

        GrpcConnectUrl::new_as_tcp(self)
    }
}

#[cfg(test)]
mod tests {
    use super::GrpcConnectUrl;

    #[test]
    fn a_pinned_ip_leaves_the_host_for_the_authority() {
        let url: GrpcConnectUrl = "https://domain.com@10.0.0.7:8080".to_string().into();

        // What tonic gets - and therefore what ends up in h2's `:authority`.
        assert_eq!(url.get_grpc_host(), "https://domain.com:8080");
        assert_eq!(
            url.get_resolved_ip().unwrap(),
            Some("10.0.0.7".parse().unwrap())
        );
        // Logs and errors keep naming the url the caller wrote.
        assert_eq!(url.as_str(), "https://domain.com@10.0.0.7:8080");
        assert!(url.is_grpc_tls_endpoint());
    }

    #[test]
    fn a_url_without_a_pinned_ip_is_untouched() {
        let url: GrpcConnectUrl = "http://localhost:5000".to_string().into();

        assert_eq!(url.get_grpc_host(), "http://localhost:5000");
        assert_eq!(url.get_resolved_ip().unwrap(), None);
        assert!(!url.is_grpc_tls_endpoint());
    }

    #[test]
    fn a_malformed_pin_is_reported_rather_than_guessed() {
        let url: GrpcConnectUrl = "http://domain.com@not-an-ip:8080".to_string().into();

        assert!(url.get_resolved_ip().is_err());
        // The url is left as written, so the error can quote it.
        assert_eq!(url.get_grpc_host(), "http://domain.com@not-an-ip:8080");
    }

    #[cfg(unix)]
    #[test]
    fn a_unix_socket_has_no_pinned_ip() {
        let url: GrpcConnectUrl = "/tmp/test.sock".to_string().into();

        assert!(url.is_unix_socket());
        assert_eq!(url.get_resolved_ip().unwrap(), None);
    }
}
