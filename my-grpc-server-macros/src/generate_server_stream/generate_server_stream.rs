use proc_macro::TokenStream;
use types_reader::TokensObject;

pub fn generate_server_stream(tokens: TokenStream) -> Result<TokenStream, syn::Error> {
    let tokens: proc_macro2::TokenStream = tokens.into();
    let params_list = TokensObject::new(tokens.into())?;

    let stream_name: &str = params_list.get_named_param("stream_name")?.try_into()?;

    let item_name: &str = params_list.get_named_param("item_name")?.try_into()?;

    Ok(crate::generate_stream::generate_stream(stream_name, item_name).into())
}
