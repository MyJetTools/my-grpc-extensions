use proc_macro::TokenStream;

mod generate_server_stream;
mod with_telemetry;

#[proc_macro_attribute]
pub fn with_telemetry(attr: TokenStream, item: TokenStream) -> TokenStream {
    match crate::with_telemetry::generate(attr, item) {
        Ok(result) => result,
        Err(err) => err.into_compile_error().into(),
    }
}

#[proc_macro]
pub fn generate_server_stream(item: TokenStream) -> TokenStream {
    match crate::generate_server_stream::generate_server_stream(item) {
        Ok(result) => result,
        Err(err) => err.into_compile_error().into(),
    }
}
