use proc_macro::TokenStream;
mod grpc_client;

#[proc_macro_attribute]
pub fn generate_grpc_client(attr: TokenStream, item: TokenStream) -> TokenStream {
    #[cfg(feature = "with-telemetry")]
    match crate::grpc_client::generate(attr, item, true) {
        Ok(result) => result,
        Err(err) => err.into_compile_error().into(),
    }

    #[cfg(not(feature = "with-telemetry"))]
    match crate::grpc_client::generate(attr, item, false) {
        Ok(result) => result,
        Err(err) => err.into_compile_error().into(),
    }
}
