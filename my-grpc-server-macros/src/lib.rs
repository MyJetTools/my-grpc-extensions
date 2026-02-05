use proc_macro::TokenStream;

mod generate_server;
mod generate_server_stream;
mod with_result_stream;
mod with_telemetry;

mod generate_stream;

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

#[proc_macro_attribute]
pub fn with_result_as_stream(attr: TokenStream, item: TokenStream) -> TokenStream {
    match crate::with_result_stream::generate(attr, item) {
        Ok(result) => result,
        Err(err) => err.into_compile_error().into(),
    }
}

#[proc_macro]
pub fn generate_server(item: TokenStream) -> TokenStream {
    match crate::generate_server::generate(item.into()) {
        Ok(result) => result.into(),
        Err(err) => err.into_compile_error().into(),
    }
}
