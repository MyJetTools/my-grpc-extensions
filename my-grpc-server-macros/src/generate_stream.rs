use std::str::FromStr;

pub fn generate_stream(stream_name: &str, item_name: &str) -> proc_macro::TokenStream {
    let stream_name = proc_macro2::TokenStream::from_str(stream_name).unwrap();
    let item_name = proc_macro2::TokenStream::from_str(item_name).unwrap();
    quote::quote! {
        type #stream_name = std::pin::Pin<
        Box<
            dyn futures_core::Stream<Item = Result<#item_name, tonic::Status>>
                + Send
                + Sync
                + 'static,
        >,
    >;
    }
    .into()
}
