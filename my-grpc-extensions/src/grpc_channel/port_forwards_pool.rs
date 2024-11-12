use std::collections::HashMap;

use my_ssh::SshSession;
use rust_extensions::remote_endpoint::RemoteEndpoint;
use tokio::sync::Mutex;

lazy_static::lazy_static! {
    pub static ref PORT_FORWARDS_POOL: PortForwardsPool = PortForwardsPool::new();
}

pub struct PortForwardsPool {
    port_forwards: Mutex<HashMap<String, ()>>,
}

impl PortForwardsPool {
    pub fn new() -> Self {
        PortForwardsPool {
            port_forwards: Mutex::new(HashMap::new()),
        }
    }

    pub async fn start_port_forward(
        &self,
        ssh_session: &SshSession,
        unix_socket_name: &str,
        grpc_service_endpoint: RemoteEndpoint<'_>,
    ) {
        let write_access = self.port_forwards.lock().await;
        if write_access.contains_key(unix_socket_name) {
            return;
        }

        let port = grpc_service_endpoint.get_port();

        if port.is_none() {
            println!(
                "Can not start port forward for unix_socket_name: {}. Port is not defined",
                unix_socket_name
            );
            return;
        }

        let result = ssh_session
            .start_port_forward(
                unix_socket_name.to_string(),
                grpc_service_endpoint.get_host().to_string(),
                port.unwrap(),
            )
            .await;

        if let Err(err) = result {
            println!(
                "Can not start port forward for unix_socket_name: {}. Error: {:?}",
                unix_socket_name, err
            );
        }
    }
}
