mod grpc_channel;
mod grpc_channel_holder;
mod grpc_channel_pool;
#[cfg(feature = "with-ssh")]
mod port_forwards_pool;
mod request_builder;
mod request_builder_with_input_stream;
mod request_builder_with_input_stream_with_retires;
mod request_builder_with_retries;
mod streamed_response;
pub use grpc_channel::*;
pub use grpc_channel_holder::*;
pub use grpc_channel_pool::*;
#[cfg(feature = "with-ssh")]
pub use port_forwards_pool::*;
pub use request_builder::*;
pub use request_builder_with_input_stream::*;
pub use request_builder_with_input_stream_with_retires::*;
pub use request_builder_with_retries::*;
pub use streamed_response::*;
mod grpc_connect_url;
pub use grpc_connect_url::*;

#[cfg(feature = "with-tls")]
fn extract_domain_name(src: &str) -> &str {
    let start = src.find("://").map(|index| index + 3).unwrap_or(0);

    let src = &src[start..];

    let end = src.find(":").unwrap_or(src.len());
    &src[..end]
}

#[cfg(test)]
#[cfg(feature = "with-tls")]
mod tests {

    #[test]
    fn test_extracting_domain_name() {
        assert_eq!(
            super::extract_domain_name("https://localhost:5000"),
            "localhost"
        );

        assert_eq!(super::extract_domain_name("https://localhost"), "localhost");
    }
}
