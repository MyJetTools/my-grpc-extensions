use std::fmt::Debug;

pub enum GrpcConnectUrl {
    Tcp {
        raw: String,
        #[cfg(feature = "with-ssh")]
        over_ssh: my_ssh::ssh_settings::OverSshConnectionSettings,
    },
    UnixSocket(String),
}

impl Debug for GrpcConnectUrl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Tcp { raw, over_ssh: _ } => f.debug_struct("Tcp").field("raw", raw).finish(),
            Self::UnixSocket(arg0) => f.debug_tuple("UnixSocket").field(arg0).finish(),
        }
    }
}

impl GrpcConnectUrl {
    fn new_as_tcp(raw: String) -> Self {
        Self::Tcp {
            #[cfg(feature = "with-ssh")]
            over_ssh: my_ssh::ssh_settings::OverSshConnectionSettings::parse(raw.as_str()),
            raw,
        }
    }

    #[cfg(unix)]
    fn new_as_unix_socket(raw: String) -> Self {
        Self::UnixSocket(raw)
    }

    #[cfg(not(feature = "with-ssh"))]
    pub fn get_grpc_host(&self) -> &str {
        match self {
            Self::Tcp { raw } => raw.as_str(),
            Self::UnixSocket(raw) => &raw,
        }
    }

    #[cfg(feature = "with-ssh")]
    pub fn get_grpc_host(&self) -> &str {
        match self {
            Self::Tcp { over_ssh, .. } => over_ssh.remote_resource_string.as_str(),

            Self::UnixSocket(raw) => &raw,
        }
    }
    #[cfg(feature = "with-ssh")]
    pub fn get_ssh_credentials(&self) -> Option<&std::sync::Arc<my_ssh::SshCredentials>> {
        match self {
            Self::Tcp { over_ssh, .. } => over_ssh.ssh_credentials.as_ref(),
            Self::UnixSocket(raw) => {
                panic!("Unix socket does not support ssh credentials: {}", raw)
            }
        }
    }

    #[cfg(feature = "with-ssh")]
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
        if self.starts_with("/") || self.starts_with("~/") {
            #[cfg(unix)]
            return GrpcConnectUrl::new_as_unix_socket(self);

            #[cfg(not(unix))]
            panic!("Detected Unix socket [{}] is not supported.", self);
        }

        GrpcConnectUrl::new_as_tcp(self)
    }
}
