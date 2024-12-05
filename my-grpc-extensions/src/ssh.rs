use std::{env, sync::Arc};

use my_ssh::*;
use rust_extensions::remote_endpoint::RemoteEndpoint;
use tokio::sync::Mutex;

#[async_trait::async_trait]
pub trait GrpcClientSsh {
    async fn set_ssh_private_key(&mut self, private_key: String, pass_phrase: Option<String>);
}

#[derive(Clone)]
pub struct SshTargetInner {
    pub credentials: Option<Arc<SshCredentials>>,
}

impl SshTargetInner {
    pub async fn get_ssh_session(&self) -> Arc<SshSession> {
        if self.credentials.is_none() {
            panic!("Ssh credentials are not set")
        }

        let ssh_credentials = self.credentials.as_ref().unwrap();

        let ssh_session = my_ssh::SshSession::new(ssh_credentials.clone());
        std::sync::Arc::new(ssh_session)
    }
}

#[derive(Clone)]
pub struct SshTarget {
    inner: Arc<Mutex<SshTargetInner>>,
}

impl SshTarget {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(SshTargetInner { credentials: None })),
        }
    }
    pub async fn set_private_key(&self, private_key: String, passphrase: Option<String>) {
        let mut inner = self.inner.lock().await;
        if let Some(ssh_credentials) = &inner.credentials {
            inner.credentials = Some(
                ssh_credentials
                    .as_ref()
                    .into_with_private_key(private_key, passphrase)
                    .into(),
            );
        }
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
