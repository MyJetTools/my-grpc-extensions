use std::str::FromStr;

use proc_macro::TokenStream;

pub fn generate_server_stream(tokens: TokenStream) -> Result<TokenStream, syn::Error> {
    let params_list = types_reader::ParamsList::new(tokens.into(), || None)?;

    let stream_name = params_list.get_named_param("stream_name")?;
    let stream_name = stream_name.unwrap_as_string_value()?.as_str();
    let stream_name = proc_macro2::TokenStream::from_str(stream_name).unwrap();

    let item_name = params_list.get_named_param("item_name")?;
    let item_name = item_name.unwrap_as_string_value()?.as_str();
    let item_name = proc_macro2::TokenStream::from_str(item_name).unwrap();

    Ok(quote::quote! {
        type #stream_name = std::pin::Pin<
        Box<
            dyn futures_core::Stream<Item = Result<#item_name, tonic::Status>>
                + Send
                + Sync
                + 'static,
        >,
    >
    }
    .into())
}
