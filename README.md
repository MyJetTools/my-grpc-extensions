# my-grpc-extensions

Utilities and proc-macros that simplify building gRPC clients and servers on top of `tonic`. It provides connection pooling, retry-aware request builders, streaming helpers, optional telemetry propagation, SSH tunneling, and TLS integration.

## Crates in this workspace
- `my-grpc-extensions` – core helpers (channels, request builders with retries/background ping, streaming utilities, telemetry hooks, SSH/TLS support).
- `my-grpc-client-macros` – `#[generate_grpc_client]` macro that builds strongly typed clients from your `.proto` with configurable retries/timeouts and optional per-method overrides.
- `my-grpc-server-macros` – server-side macros (e.g., `#[with_telemetry]`) that inject telemetry context before you handle the request and helpers to send collections/streams.

## Install
Add from Git with the features you need:

```toml
[dependencies]
my-grpc-extensions = { tag = "x.x.x", git = "https://github.com/MyJetTools/my-grpc-extensions.git", features = [
    "grpc-client",     # enable client macro re-exports
    "grpc-server",     # enable server macro re-exports
    "with-telemetry",  # pass telemetry context through metadata/remote addr
] }
```

Feature flags:
- `grpc-client` – re-export `my-grpc-client-macros`.
- `grpc-server` – re-export `my-grpc-server-macros`.
- `with-telemetry` – enables telemetry extraction/injection (requires `my-telemetry`).
- `with-ssh` – connect through SSH port-forwarding using `my-ssh`.
- `with-tls` – enable TLS support via `my-tls`.
- `adjust-server-stream` – customize gRPC server stream channel size/send timeout.

## Client macro quickstart

```rust
use my_grpc_client_macros::generate_grpc_client;

#[generate_grpc_client(
    proto_file: "./proto/KeyValueFlows.proto",
    crate_ns: "crate::keyvalue_grpc",
    retries: 3,
    request_timeout_sec: 5,
    ping_timeout_sec: 5,
    ping_interval_sec: 5,
    overrides: [{ fn_name: "Get", retries: 2 }],
)]
pub struct KeyValueGrpcClient;
```

Parameters:
- `proto_file` – path to your proto file; `crate_ns` – module where tonic-generated code lives.
- `retries` – reconnect/retry attempts on disconnect; `request_timeout_sec` – per-request timeout.
- `ping_timeout_sec` / `ping_interval_sec` – background ping used to detect drops and reconnect.
- `overrides` – per-method retry/timeouts if needed.

Implement `GrpcClientSettings` to provide service URLs:

```rust
#[async_trait::async_trait]
impl my_grpc_extensions::GrpcClientSettings for SettingsReader {
    async fn get_grpc_url(&self, name: &'static str) -> String {
        if name == KeyValueGrpcClient::get_service_name() {
            let read = self.settings.read().await;
            return read.key_value_grpc_url.clone();
        }
        panic!("Unknown grpc service name: {}", name)
    }
}
```

## Server macro quickstart

Wrap handlers with telemetry:

```rust
#[with_telemetry]
async fn get(
    &self,
    request: tonic::Request<GetDocumentsRequest>,
) -> Result<tonic::Response<Self::GetStream>, tonic::Status> {
    let request = request.into_inner(); // telemetry is injected before this line
    let result = crate::flows::get_docs(&self.app, request.client_id, request.doc_ids, my_telemetry).await;
    my_grpc_extensions::grpc_server::send_vec_to_stream(result, |dto| dto).await
}
```

Streaming helpers (server):
- `send_single_item_to_stream`, `send_from_iterator`, `create_empty_stream`.
- Enable `adjust-server-stream` to configure channel size and send timeouts.

## Best Practice: When to Use `tokio::spawn` in gRPC Handlers

**CRITICAL RULE**: Only use `tokio::spawn` when implementing gRPC functions that return **streaming responses**. For non-streaming responses, **generally** await the operation directly.

**Note**: There may be exceptions to this rule for non-streaming responses. Any such exceptions will be documented at the application level (e.g., in application-specific documentation or rules).

### Why This Matters

For streaming responses, you must return the stream handle immediately to establish the connection. The actual data production happens asynchronously in the spawned task. For non-streaming responses, you should typically await the operation to ensure proper error handling and response timing. However, specific application requirements may necessitate spawning tasks for non-streaming responses in certain cases.

### Streaming Response (Use `tokio::spawn`)

The pattern for streaming responses uses `StreamedResponseWriter`:

1. **Create** `StreamedResponseWriter` with a buffer size
2. **Extract** the producer using `get_stream_producer()`
3. **Pass** the producer to the spawned function
4. **Return** the result using `get_result()` to establish the stream connection immediately

```rust
#[with_telemetry]
async fn get_candles_by_instrument(
    &self,
    request: tonic::Request<GetCandlesHistoryGrpcRequest>,
) -> Result<tonic::Response<Self::GetCandlesByInstrumentStream>, tonic::Status> {
    let request = request.into_inner();
    
    // Step 1: Create StreamedResponseWriter
    let mut result = StreamedResponseWriter::new(1024);
    
    // Step 2: Extract the producer
    let producer = result.get_stream_producer();
    
    // Step 3: Spawn the async work and pass the producer
    tokio::spawn(crate::flows::get_candles(
        self.app.clone(),
        request.instrument_id,
        request.from_key,
        request.to_key,
        request.is_bid,
        candle_type,
        request.limit.map(|x| x as usize),
        producer,  // Producer passed to spawned function
        my_telemetry.clone(),
    ));
    
    // Step 4: Return the result immediately to establish stream connection
    result.get_result()
}
```

### Non-Streaming Response (Do NOT use `tokio::spawn`)

```rust
#[with_telemetry]
async fn update_cache_from_db(
    &self,
    request: tonic::Request<UpdateCacheFromDbGrpcRequest>,
) -> Result<tonic::Response<()>, tonic::Status> {
    let request = request.into_inner();
    
    // Await directly - no tokio::spawn needed
    crate::scripts::update_cache_from_db(self.app.as_ref(), &request.instrument_id)
        .await
        .map_err(|err| tonic::Status::internal(err))?;
    
    Ok(tonic::Response::new(()))
}
```

### Summary

- ✅ **Streaming responses**: Use `tokio::spawn` to return the stream handle immediately
- ✅ **Non-streaming responses (general rule)**: Await the operation directly - do NOT use `tokio::spawn`
- ⚠️ **Non-streaming responses (exceptions)**: May use `tokio::spawn` in specific cases - check application-level documentation for exceptions

## Connecting to gRPC over SSH

Enable `with-ssh` and configure credentials/pool from `my-ssh`:

```toml
my-grpc-extensions = { tag = "x.x.x", git = "https://github.com/MyJetTools/my-grpc-extensions.git", features = [
    "grpc-client",
    "with-ssh",
] }
```

```rust
let grpc_settings = GrpcLogSettings::new(over_ssh_connection.remote_resource_string);
let grpc_client = MyLoggerGrpcClient::new(Arc::new(grpc_settings));

let ssh_credentials = my_grpc_extensions::my_ssh::SshCredentials::SshAgent {
    ssh_remote_host: "ssh_host".to_string(),
    ssh_remote_port: 22,
    ssh_user_name: "user".to_string(),
};

grpc_client.set_ssh_credentials(Arc::new(ssh_credentials)).await;
grpc_client.set_ssh_sessions_pool(ssh_sessions_pool.clone()).await; // keep sessions reused
```

SSH uses UNIX socket port-forwarding under the hood to reach the target gRPC endpoint behind the tunnel.

