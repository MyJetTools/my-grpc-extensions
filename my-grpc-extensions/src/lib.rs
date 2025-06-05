mod grpc_channel;
#[cfg(feature = "with-telemetry")]
mod grpc_client_interceptor;
pub mod grpc_server;
#[cfg(feature = "with-telemetry")]
mod grpc_server_telemetry_context;
#[cfg(feature = "with-telemetry")]
pub use grpc_server_telemetry_context::*;
pub mod read_grpc_stream;
pub use grpc_channel::*;
#[cfg(feature = "with-telemetry")]
pub use grpc_client_interceptor::*;

pub extern crate external_dependencies as prelude;

#[cfg(feature = "grpc-client")]
pub extern crate my_grpc_client_macros as client;

#[cfg(feature = "grpc-server")]
pub extern crate my_grpc_server_macros as server;

pub extern crate hyper;
pub extern crate tonic;
#[cfg(feature = "with-ssh")]
mod ssh;
#[cfg(feature = "with-ssh")]
pub use ssh::*;
#[cfg(feature = "with-ssh")]
pub extern crate my_ssh;

mod grpc_stream_result;
#[cfg(feature = "grpc-server")]
pub mod server_stream_result;
pub use grpc_stream_result::*;

mod streamed_request;
pub use streamed_request::*;
