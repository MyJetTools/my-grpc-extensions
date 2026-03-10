# my-grpc-extensions

Utilities and proc-macros that simplify building gRPC clients and servers on top of `tonic`. It provides connection pooling, retry-aware request builders, streaming helpers, optional telemetry propagation, SSH tunneling, and TLS integration.

## Crates in this workspace
- `my-grpc-extensions` – core helpers (channels, request builders with retries/background ping, streaming utilities, telemetry hooks, SSH/TLS support).
- `my-grpc-client-macros` – `#[generate_grpc_client]` macro that builds strongly typed clients from your `.proto` with configurable retries/timeouts and optional per-method overrides.
- `my-grpc-server-macros` – server-side macros that inject telemetry context and helpers to send collections/streams.

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

## Server quickstart

### 1. Module setup

`src/grpc_server/mod.rs`:
```rust
mod my_service_grpc_server;
pub use my_service_grpc_server::*;

service_sdk::macros::use_grpc_server!();
```

### 2. Server implementation file

`src/grpc_server/my_service_grpc_server.rs`:
```rust
use std::sync::Arc;
use crate::{app::AppContext, models::FlowError};

service_sdk::macros::use_grpc_server!();

// Macro reads the proto file and generates the server boilerplate.
// You implement one plain async fn per RPC method.
generate_server!(proto_file: "./proto/MyService.proto", crate_ns: "crate::my_service_grpc");

// Proto method name (PascalCase) → Rust function name (snake_case)
// rpc ClosePosition(...) → async fn close_position(...)
// rpc GetInstruments(...) → async fn get_instruments(...)
// rpc PostBidAsk(...) → async fn post_bid_ask(...)

// Non-streaming: return the response type directly
async fn close_position(
    app: &Arc<AppContext>,
    request: ClosePositionGrpcRequest,  // request fields unwrapped — no tonic::Request<T>
) -> ClosePositionGrpcResponse {
    // await directly, no tokio::spawn
    todo!()
}

// Streaming output (server → client): return StreamedResponseWriter<T>
async fn get_instruments(
    app: &Arc<AppContext>,
    _request: (),  // google.protobuf.Empty → ()
) -> StreamedResponseWriter<InstrumentGrpcModel> {
    let result = StreamedResponseWriter::new(1024);
    let producer = result.get_stream_producer();
    tokio::spawn(crate::flows::get_instruments(app.clone(), producer));
    result
}

// Streaming input (client → server): receive StreamedRequestReader<T>
async fn post_bid_ask(
    app: &Arc<AppContext>,
    request: StreamedRequestReader<BidAskGrpcModel>,
) {
    let items = request.into_vec().await.unwrap();
    crate::flows::handle_bid_ask(app, items).await;
}
```

### 3. Include proto module and register in `main.rs`

```rust
mod my_service_grpc {
    tonic::include_proto!("my_service");  // package name from proto file
}

use my_service_grpc::my_service_server::*;

// In main():
service_context.configure_grpc_server(|builder| {
    builder.add_grpc_service(MyServiceServer::new(SdkGrpcService::new(app.clone())))
});
```

### Key rules
- Function names are **snake_case** of the proto RPC name
- First param is always `app: &Arc<AppContext>`
- Request type is the proto message directly (no `tonic::Request<T>` wrapper)
- `google.protobuf.Empty` input → `_request: ()`
- Return type is the proto message directly (no `tonic::Response<T>` wrapper)

---

## When to use `tokio::spawn` in gRPC handlers

**Rule: only use `tokio::spawn` for streaming output responses.**

| Case | `tokio::spawn`? |
|------|----------------|
| Streaming output (server → client) | **Yes** — must return stream handle immediately |
| Streaming input (client → server) | No — collect with `request.into_vec().await` |
| Non-streaming response | No — await directly |

### Streaming output — use `tokio::spawn`

```rust
async fn get_instruments(
    app: &Arc<AppContext>,
    _request: (),
) -> StreamedResponseWriter<InstrumentGrpcModel> {
    let result = StreamedResponseWriter::new(1024);
    let producer = result.get_stream_producer();
    tokio::spawn(crate::flows::get_instruments(app.clone(), producer));
    result
}
```

The spawned function receives a `StreamedResponseProducer<T>`:

```rust
pub async fn get_instruments(
    app: Arc<AppContext>,
    producer: StreamedResponseProducer<InstrumentGrpcModel>,
) {
    let cache = app.trading_data_holder.lock().await;
    for instrument in cache.instruments.get_all() {
        producer.send(instrument.into()).await.unwrap();
    }
}
```

### Streaming input — no `tokio::spawn`

```rust
async fn post_bid_ask(
    app: &Arc<AppContext>,
    request: StreamedRequestReader<BidAskGrpcModel>,
) {
    let items = request.into_vec().await.unwrap();
    crate::flows::handle_bid_ask(app, items).await;
}
```

### Non-streaming — no `tokio::spawn`

```rust
async fn close_position(
    app: &Arc<AppContext>,
    request: ClosePositionGrpcRequest,
) -> ClosePositionGrpcResponse {
    let result = crate::flows::close_position(app, request.account_id.into()).await;
    match result {
        Ok(_) => ClosePositionGrpcResponse { result: MarginEngineGrpcResult::Ok.into() },
        Err(err) => ClosePositionGrpcResponse { result: err.into() },
    }
}
```

---

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

---

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
grpc_client.set_ssh_sessions_pool(ssh_sessions_pool.clone()).await;
```

SSH uses UNIX socket port-forwarding under the hood to reach the target gRPC endpoint behind the tunnel.
