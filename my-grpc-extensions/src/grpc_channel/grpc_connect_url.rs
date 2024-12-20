pub enum GrpcConnectUrl {
    Tcp {
        raw: String,
        #[cfg(feature = "with-ssh")]
        over_ssh: my_ssh::ssh_settings::OverSshConnectionSettings,
    },
    #[cfg(feature = "with-unix-socket")]
    UnixSocket(String),
}
impl GrpcConnectUrl {
    fn new_as_tcp(raw: String) -> Self {
        Self::Tcp {
            #[cfg(feature = "with-ssh")]
            over_ssh: my_ssh::ssh_settings::OverSshConnectionSettings::parse(raw.as_str()),
            raw,
        }
    }
    #[cfg(feature = "with-unix-socket")]
    fn new_as_unix_socket(raw: String) -> Self {
        Self::Tcp {
            #[cfg(feature = "with-ssh")]
            over_ssh: my_ssh::ssh_settings::OverSshConnectionSettings::parse(raw.as_str()),
            raw,
        }
    }

    #[cfg(not(feature = "with-ssh"))]
    pub fn get_grpc_host(&self) -> &str {
        match self {
            Self::Tcp { raw } => raw.as_str(),
            #[cfg(feature = "with-unix-socket")]
            Self::UnixSocket(raw) => &raw,
        }
    }

    #[cfg(feature = "with-ssh")]
    pub fn get_grpc_host(&self) -> &str {
        match self {
            Self::Tcp { over_ssh, .. } => over_ssh.remote_resource_string.as_str(),
            #[cfg(feature = "with-unix-socket")]
            Self::UnixSocket(raw) => &raw,
        }
    }
    #[cfg(feature = "with-ssh")]
    pub fn get_ssh_credentials(&self) -> Option<&std::sync::Arc<my_ssh::SshCredentials>> {
        match self {
            Self::Tcp { over_ssh, .. } => over_ssh.ssh_credentials.as_ref(),
            #[cfg(feature = "with-unix-socket")]
            Self::UnixSocket(raw) => {
                panic!("Unix socket does not support ssh credentials: {}", raw)
            }
        }
    }

    #[cfg(feature = "with-ssh")]
    pub fn is_over_ssh(&self) -> bool {
        match self {
            Self::Tcp { over_ssh, .. } => over_ssh.ssh_credentials.is_some(),
            #[cfg(feature = "with-unix-socket")]
            Self::UnixSocket(_) => false,
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Tcp { raw, .. } => raw,
            #[cfg(feature = "with-unix-socket")]
            Self::UnixSocket(raw) => raw,
        }
    }

    pub fn is_grpc_tls_endpoint(&self) -> bool {
        let grpc_host = self.get_grpc_host();
        rust_extensions::str_utils::starts_with_case_insensitive(grpc_host, "https")
    }

    #[cfg(feature = "with-unix-socket")]
    pub fn is_unix_socket(&self) -> bool {
        match self {
            Self::UnixSocket(_) => true,
            _ => false,
        }
    }
}

impl Into<GrpcConnectUrl> for String {
    fn into(self) -> GrpcConnectUrl {
        if self.starts_with("/") || self.starts_with("~/") {
            #[cfg(feature = "with-unix-socket")]
            return GrpcConnectUrl::new_as_unix_socket(self);

            #[cfg(not(feature = "with-unix-socket"))]
            panic!("Detected Unix socket [{}] which is not supported. Please enable feature with-unix-socket", self);
        }

        GrpcConnectUrl::new_as_tcp(self)
    }
}
