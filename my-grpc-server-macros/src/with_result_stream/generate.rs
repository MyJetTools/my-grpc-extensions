use types_reader::TokensObject;

pub fn generate(
    attr: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> Result<proc_macro::TokenStream, syn::Error> {
    let tokens: proc_macro2::TokenStream = attr.into();
    let params_list = TokensObject::new(tokens.into())?;

    let item_name: &str = params_list
        .get_value_from_single_or_named("item_name")?
        .try_into()?;

    let content = input.to_string();

    let stream_name = find_stream_name(content.as_str());

    let stream_implementation = crate::generate_stream::generate_stream(stream_name, item_name);

    let input: proc_macro2::TokenStream = input.into();

    let result = quote::quote! {
        #stream_implementation
        #input
    };

    Ok(result.into())
}

fn find_stream_name(content: &str) -> &str {
    let mut self_index = false;
    let mut double_dots_index = false;

    let mut stream_name = None;

    for item in content.split(' ') {
        if !self_index {
            if item == "Self" {
                self_index = true;
                continue;
            }
        }

        if !double_dots_index {
            if item == "::" {
                double_dots_index = true;
                continue;
            }
        }

        if !double_dots_index {
            if item.ends_with("Stream") {
                stream_name = Some(item);
                break;
            }
        }

        self_index = false;
        double_dots_index = false;
    }

    if stream_name.is_none() {
        panic!("tonic::Response<Self::StreamName> is not found");
    }

    stream_name.unwrap()
}

#[cfg(test)]
mod tests {

    #[test]
    fn test_finding_stream() {
        let content = r#"#[allow(clippy :: async_yields_async, clippy :: diverging_sub_expression,
            clippy :: let_unit_value, clippy :: no_effect_underscore_binding, clippy ::
            shadow_same, clippy :: type_complexity, clippy :: type_repetition_in_bounds,
            clippy :: used_underscore_binding)] fn get_active_withdrawals < 'life0,
            'async_trait >
            (& 'life0 self, request : tonic :: Request < GetActiveWithdrawalsRequest >,)
            -> :: core :: pin :: Pin < Box < dyn :: core :: future :: Future < Output =
            Result < tonic :: Response < Self :: GetActiveWithdrawalsStream >, tonic ::
            Status > > + :: core :: marker :: Send + 'async_trait > > where 'life0 :
            'async_trait, Self : 'async_trait
            {
                Box ::
                pin(async move
                {
                    if let :: core :: option :: Option :: Some(__ret) = :: core :: option
                    :: Option :: None :: < Result < tonic :: Response < Self ::
                    GetActiveWithdrawalsStream >, tonic :: Status > > { return __ret ; }
                    let __self = self ; let request = request ; let __ret : Result < tonic
                    :: Response < Self :: GetActiveWithdrawalsStream >, tonic :: Status >
                    = { let request = request.into_inner() ; todo! ("Implement me") } ;
                    #[allow(unreachable_code)] __ret
                })
            }
        "#;

        let stream_name = super::find_stream_name(content);

        assert_eq!(stream_name, "GetActiveWithdrawalsStream");
    }
}
