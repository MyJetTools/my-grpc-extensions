use std::{env, sync::Arc};

use my_ssh::*;
use rust_extensions::remote_endpoint::RemoteEndpoint;
use tokio::sync::Mutex;

use crate::GrpcConnectUrl;

#[async_trait::async_trait]
pub trait GrpcClientSsh {
    async fn set_ssh_private_key(&self, private_key: String, pass_phrase: Option<String>);
}

#[derive(Clone)]
pub struct SshTargetInner {
    pub private_key: Option<(String, Option<String>)>,
}

impl SshTargetInner {
    pub fn get_ssh_session(&self, ssh_credentials: &SshCredentials) -> Arc<SshSession> {
        let ssh_credentials = if let Some((private_key, pass_phrase)) = self.private_key.as_ref() {
            let host_port = ssh_credentials.get_host_port();
            SshCredentials::PrivateKey {
                ssh_remote_host: host_port.0.to_string(),
                ssh_remote_port: host_port.1,
                ssh_user_name: ssh_credentials.get_user_name().to_string(),
                private_key: private_key.to_string(),
                passphrase: pass_phrase.clone(),
            }
        } else {
            ssh_credentials.clone()
        };

        let ssh_session = my_ssh::SshSession::new(Arc::new(ssh_credentials));
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
            inner: Arc::new(Mutex::new(SshTargetInner { private_key: None })),
        }
    }
    pub async fn set_private_key(&self, private_key: String, passphrase: Option<String>) {
        let mut inner = self.inner.lock().await;
        inner.private_key = Some((private_key, passphrase));
    }

    pub async fn get_value(&self) -> SshTargetInner {
        self.inner.lock().await.clone()
    }
}

pub fn generate_unix_socket_file(
    ssh_credentials: &SshCredentials,
    remote_host: RemoteEndpoint,
) -> GrpcConnectUrl {
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

    let result = format!(
        "{}/grpc-{}-{}_{}--{}_{}.sock",
        root_path,
        ssh_credentials.get_user_name(),
        ssh_host,
        ssh_port,
        r_host,
        r_port
    );

    result.into()
}
