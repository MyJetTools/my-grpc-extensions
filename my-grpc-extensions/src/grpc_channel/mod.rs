mod grpc_channel;
mod grpc_channel_pool;
mod grpc_channel_pool_inner;
mod request_builder;
mod request_builder_with_input_stream;
mod request_builder_with_input_stream_with_retires;
mod request_builder_with_retries;
mod streamed_response;
pub use grpc_channel::*;
pub use grpc_channel_pool::*;
pub use grpc_channel_pool_inner::*;
pub use request_builder::*;
pub use request_builder_with_input_stream::*;
pub use request_builder_with_input_stream_with_retires::*;
pub use request_builder_with_retries::*;
pub use streamed_response::*;

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
