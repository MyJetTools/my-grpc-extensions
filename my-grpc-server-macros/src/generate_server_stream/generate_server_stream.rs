use proc_macro::TokenStream;

pub fn generate_server_stream(tokens: TokenStream) -> Result<TokenStream, syn::Error> {
    let params_list = types_reader::ParamsList::new(tokens.into(), || None)?;

    let stream_name = params_list.get_named_param("stream_name")?;

    let item_name = params_list.get_named_param("item_name")?;

    Ok(crate::generate_stream::generate_stream(
        stream_name.unwrap_as_string_value()?.as_str(),
        item_name.unwrap_as_string_value()?.as_str(),
    ))
}
