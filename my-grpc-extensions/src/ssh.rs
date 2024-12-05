use std::{env, sync::Arc};

use my_ssh::*;
use rust_extensions::remote_endpoint::RemoteEndpoint;
use tokio::sync::Mutex;

#[async_trait::async_trait]
pub trait GrpcClientSsh {
    async fn set_ssh_security_credentials_resolver(
        &self,
        resolver: Arc<
            dyn my_ssh::ssh_settings::SshSecurityCredentialsResolver + Send + Sync + 'static,
        >,
    );
}

#[derive(Clone)]
pub struct SshTargetInner {
    pub private_key_resolver: Option<
        Arc<dyn my_ssh::ssh_settings::SshSecurityCredentialsResolver + Send + Sync + 'static>,
    >,
}

impl SshTargetInner {
    async fn get_ssh_credentials(&self, ssh_credentials: &SshCredentials) -> SshCredentials {
        if let Some(private_key_resolver) = self.private_key_resolver.as_ref() {
            let ssh_line = ssh_credentials.to_string();
            if let Some(private_key) = private_key_resolver
                .resolve_ssh_private_key(&ssh_line)
                .await
            {
                let host_port = ssh_credentials.get_host_port();
                return SshCredentials::PrivateKey {
                    ssh_remote_host: host_port.0.to_string(),
                    ssh_remote_port: host_port.1,
                    ssh_user_name: ssh_credentials.get_user_name().to_string(),
                    private_key: private_key.content,
                    passphrase: private_key.pass_phrase.clone(),
                };
            }
        }
        ssh_credentials.clone()
    }

    pub async fn get_ssh_session(&self, ssh_credentials: &SshCredentials) -> Arc<SshSession> {
        let ssh_credentials = self.get_ssh_credentials(ssh_credentials).await;
        let ssh_credentials = Arc::new(ssh_credentials);
        let ssh_session = my_ssh::SSH_SESSIONS_POOL
            .get_or_create(&ssh_credentials)
            .await;

        ssh_session
    }
}

#[derive(Clone)]
pub struct SshTarget {
    inner: Arc<Mutex<SshTargetInner>>,
}

impl SshTarget {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(SshTargetInner {
                private_key_resolver: None,
            })),
        }
    }
    pub async fn set_ssh_security_credentials_resolver(
        &self,
        resolver: Arc<dyn ssh_settings::SshSecurityCredentialsResolver + Send + Sync + 'static>,
    ) {
        let mut inner = self.inner.lock().await;
        inner.private_key_resolver = Some(resolver);
    }

    pub async fn get_value(&self) -> SshTargetInner {
        self.inner.lock().await.clone()
    }
}

pub fn generate_unix_socket_file(
    ssh_credentials: &SshCredentials,
    remote_host: RemoteEndpoint,
) -> String {
    let (ssh_host, ssh_port) = ssh_credentials.get_host_port();

    let r_host = remote_host.get_host();
    let r_port = match remote_host.get_port() {
        Some(port) => port.to_string(),
        None => "".to_string(),
    };

    let root_path = match env::var("HOME") {
        Ok(value) => value,
        Err(_) => "/tmp".to_string(),
    };

    format!(
        "{}/grpc-{}-{}_{}--{}_{}.sock",
        root_path,
        ssh_credentials.get_user_name(),
        ssh_host,
        ssh_port,
        r_host,
        r_port
    )
}
