mod grpc_channel;
mod grpc_client_interceptor;
pub mod grpc_server;
mod grpc_server_telemetry_context;

pub use grpc_server_telemetry_context::*;
pub mod read_grpc_stream;
pub use grpc_channel::*;
pub use grpc_client_interceptor::*;


pub extern crate external_dependencies as prelude;


#[cfg(feature = "grpc-client")]
pub extern crate my_grpc_client_macros as client;


#[cfg(feature = "grpc-server")]
pub extern crate my_grpc_server_macros as server;

