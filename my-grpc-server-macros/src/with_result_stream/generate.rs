pub fn generate(
    attr: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> Result<proc_macro::TokenStream, syn::Error> {
    let content = input.to_string();

    println!("content: {}", content);

    let index = content.find("tonic::Response<Self::");

    if index.is_none() {
        panic!("tonic::Response<Self::StreamName> is not found");
    }

    let index = index.unwrap();

    let content = &content[index..];

    let end = content.find(">");

    if end.is_none() {
        panic!("tonic::Response<Self::StreamName> is not found");
    }

    let end = end.unwrap();

    let stream_name = &content[0..end];

    let params_list = types_reader::ParamsList::new(attr.into(), || None)?;

    let item_name = params_list.get_from_single_or_named("item_name")?;

    Ok(crate::generate_stream::generate_stream(
        stream_name,
        item_name.unwrap_as_string_value()?.as_str(),
    ))
}
