use proc_macro::TokenStream;
mod grpc_client;

#[proc_macro_attribute]
pub fn generate_grpc_client(attr: TokenStream, item: TokenStream) -> TokenStream {
    #[cfg(feature = "with-telemetry")]
    let with_telemetry = true;

    #[cfg(not(feature = "with-telemetry"))]
    let with_telemetry = false;

    #[cfg(feature = "with-ssh")]
    let with_ssh = true;

    #[cfg(not(feature = "with-ssh"))]
    let with_ssh = false;

    match crate::grpc_client::generate(attr, item, with_telemetry, with_ssh) {
        Ok(result) => result,
        Err(err) => err.into_compile_error().into(),
    }
}
