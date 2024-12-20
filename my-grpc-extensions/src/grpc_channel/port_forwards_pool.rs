use std::{collections::HashMap, sync::Arc};

use my_ssh::{SshPortForwardTunnel, SshSession};
use rust_extensions::remote_endpoint::RemoteEndpoint;
use tokio::sync::Mutex;

lazy_static::lazy_static! {
    pub static ref PORT_FORWARDS_POOL: PortForwardsPool = PortForwardsPool::new();
}

pub struct PortForwardInner {
    pub port_forwards: HashMap<String, Arc<SshPortForwardTunnel>>,
}

impl PortForwardInner {
    pub fn new() -> Self {
        PortForwardInner {
            port_forwards: HashMap::new(),
        }
    }
}

pub struct PortForwardsPool {
    port_forwards: Mutex<PortForwardInner>,
}

impl PortForwardsPool {
    pub fn new() -> Self {
        PortForwardsPool {
            port_forwards: Mutex::new(PortForwardInner::new()),
        }
    }

    pub async fn start_port_forward(
        &self,
        ssh_session: Arc<SshSession>,
        unix_socket: &str,
        grpc_service_endpoint: RemoteEndpoint<'_>,
    ) {
        let id = format!(
            "{}->{}",
            ssh_session.get_ssh_credentials().to_string(),
            grpc_service_endpoint.as_str()
        );
        let mut write_access = self.port_forwards.lock().await;
        if write_access.port_forwards.contains_key(id.as_str()) {
            return;
        }

        let port = grpc_service_endpoint.get_port();

        if port.is_none() {
            panic!(
                "Can not start port forward [{}]->[{}]. Port is not defined",
                ssh_session.get_ssh_credentials().to_string(),
                grpc_service_endpoint.as_str()
            );
        }

        println!(
            "Starting port forward :[{}]->[{}]->[{}]",
            unix_socket,
            ssh_session.get_ssh_credentials().to_string(),
            grpc_service_endpoint.get_host_port()
        );

        let result = ssh_session
            .start_port_forward(
                unix_socket.to_string(),
                grpc_service_endpoint.get_host().to_string(),
                port.unwrap(),
            )
            .await;

        if let Err(err) = &result {
            println!(
                "Can not start port forward: [{}]->[{}]->[{}]. Error: {:?}",
                unix_socket,
                ssh_session.get_ssh_credentials().to_string(),
                grpc_service_endpoint.as_str(),
                err
            );
        }

        let result = result.unwrap();

        println!("Listening on {}", unix_socket);
        write_access
            .port_forwards
            .insert(unix_socket.to_string(), result);
    }
}
