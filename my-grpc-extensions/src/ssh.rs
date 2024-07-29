use std::{env, sync::Arc};

use my_ssh::*;
use tokio::sync::Mutex;
#[derive(Clone)]
pub struct SshTargetInner {
    pub credentials: Option<Arc<SshCredentials>>,
    pub sessions_pool: Option<Arc<SshSessionsPool>>,
}

impl SshTargetInner {
    pub async fn get_ssh_session(&self) -> Arc<SshSession> {
        if self.credentials.is_none() {
            panic!("Ssh credentials are not set")
        }

        let ssh_credentials = self.credentials.as_ref().unwrap();

        if let Some(pool) = self.sessions_pool.as_ref() {
            return pool.get_or_create(ssh_credentials).await;
        }
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
            inner: Arc::new(Mutex::new(SshTargetInner {
                credentials: None,
                sessions_pool: None,
            })),
        }
    }

    pub async fn set_credentials(&self, value: Arc<SshCredentials>) {
        let mut inner = self.inner.lock().await;
        inner.credentials = Some(value);
    }

    pub async fn set_sessions_pool(&self, value: Arc<SshSessionsPool>) {
        let mut inner = self.inner.lock().await;
        inner.sessions_pool = Some(value);
    }

    pub async fn get_value(&self) -> SshTargetInner {
        self.inner.lock().await.clone()
    }
}

pub fn generate_unix_socket_file(
    ssh_credentials: &SshCredentials,
    remote_host: rust_extensions::url_utils::HostEndpoint,
) -> String {
    let (ssh_host, ssh_port) = ssh_credentials.get_host_port();

    let r_host = remote_host.host;
    let r_port = match remote_host.port {
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
