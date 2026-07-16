use std::sync::Arc;

use ahash::AHashMap;
use tokio::sync::Mutex;

use crate::{GrpcClientSettings, GrpcUrl};

/// Resolves grpc urls for pooled grpc clients.
///
/// Same idea as [`GrpcClientSettings`], but it additionally receives the id of the client
/// within the pool, so each pooled instance can be pointed to its own endpoint.
/// The returned url is resolved the same way as for a single client: a value starting with
/// `/` or `~/` connects through a unix socket, everything else through tcp.
#[async_trait::async_trait]
pub trait GrpcClientPoolSettings {
    async fn get_grpc_url(&self, name: &'static str, id: &str) -> GrpcUrl;
}

/// Adapts [`GrpcClientPoolSettings`] to [`GrpcClientSettings`] by capturing the id of the client
/// within the pool.
///
/// Thanks to it the rest of the machinery (grpc channel, ping loop, generated methods) stays
/// id agnostic and keeps working with a plain [`GrpcClientSettings`].
pub struct GrpcClientPoolSettingsAdapter {
    settings: Arc<dyn GrpcClientPoolSettings + Send + Sync + 'static>,
    id: String,
}

impl GrpcClientPoolSettingsAdapter {
    pub fn new(
        settings: Arc<dyn GrpcClientPoolSettings + Send + Sync + 'static>,
        id: String,
    ) -> Self {
        Self { settings, id }
    }

    pub fn get_id(&self) -> &str {
        self.id.as_str()
    }
}

#[async_trait::async_trait]
impl GrpcClientSettings for GrpcClientPoolSettingsAdapter {
    async fn get_grpc_url(&self, name: &'static str) -> GrpcUrl {
        self.settings.get_grpc_url(name, self.id.as_str()).await
    }
}

/// Implemented by grpc clients generated with the `generate_grpc_client_pool` macro, so
/// [`GrpcClientsPool`] is able to instantiate them on demand.
pub trait GrpcPooledClient {
    fn new_pooled(
        settings: Arc<dyn GrpcClientPoolSettings + Send + Sync + 'static>,
        id: String,
    ) -> Self;
}

/// Holds grpc clients of the same service, each one addressed by its own id and pointing to
/// its own endpoint. Clients are created lazily and live their own life until they are
/// garbage collected with [`GrpcClientsPool::gc`].
pub struct GrpcClientsPool<TClient: GrpcPooledClient + Send + Sync + 'static> {
    settings: Arc<dyn GrpcClientPoolSettings + Send + Sync + 'static>,
    clients: Mutex<AHashMap<String, Arc<TClient>>>,
}

impl<TClient: GrpcPooledClient + Send + Sync + 'static> GrpcClientsPool<TClient> {
    pub fn new(settings: Arc<dyn GrpcClientPoolSettings + Send + Sync + 'static>) -> Self {
        Self {
            settings,
            clients: Mutex::new(AHashMap::new()),
        }
    }

    /// Returns the client with the given id, creating it if the pool does not have it yet.
    pub async fn get_grpc_client(&self, id: &str) -> Arc<TClient> {
        let mut clients = self.clients.lock().await;

        if let Some(client) = clients.get(id) {
            return client.clone();
        }

        let client = Arc::new(TClient::new_pooled(self.settings.clone(), id.to_string()));

        clients.insert(id.to_string(), client.clone());

        client
    }

    /// Removes every client whose id is not in `ids`.
    ///
    /// A removed client is dropped once the last `Arc` handed out by
    /// [`GrpcClientsPool::get_grpc_client`] is released, which in turn stops its background
    /// ping loop and closes its channel. Requesting the same id afterwards creates a new client.
    pub async fn gc(&self, ids: &[&str]) {
        let mut clients = self.clients.lock().await;
        clients.retain(|id, _| ids.contains(&id.as_str()));
    }

    /// Ids of the clients which are currently in the pool.
    pub async fn get_ids(&self) -> Vec<String> {
        let clients = self.clients.lock().await;
        clients.keys().cloned().collect()
    }
}
