use std::str::FromStr;

use proc_macro::Delimiter;
use proc_macro::Group;
use proc_macro::TokenStream;

pub fn generate(
    _attr: TokenStream,
    input: TokenStream,
) -> Result<proc_macro::TokenStream, syn::Error> {
    let mut result: Vec<proc_macro2::TokenStream> = Vec::new();

    let mut fn_is_engaged = false;
    let mut fn_name = None;
    let mut injection_is_done = false;

    for token in input.into_iter() {
        match &token {
            proc_macro::TokenTree::Ident(ident) => {
                let ident_string = ident.to_string();
                if !injection_is_done {
                    if fn_is_engaged {
                        fn_name = Some(ident_string);
                        fn_is_engaged = false;
                    } else if ident_string.as_str() == "fn" {
                        fn_is_engaged = true;
                    }
                }
            }
            proc_macro::TokenTree::Group(group) => {
                if !injection_is_done {
                    if let Some(fn_name) = &fn_name {
                        if let Delimiter::Brace = group.delimiter() {
                            let token_stream = inject_body(fn_name, group);
                            result.push(token_stream);
                            injection_is_done = true;
                            continue;
                        }
                    }
                }
            }

            _ => {}
        }

        let token_stream = TokenStream::from(token);
        let token_stream: proc_macro2::TokenStream = token_stream.into();

        result.push(token_stream);
    }

    let result = quote::quote! { #(#result)* };

    Ok(result.into())
}

fn inject_body(fn_name: &str, group: &Group) -> proc_macro2::TokenStream {
    let mut as_str = group.to_string();

    let index = as_str.find("let request = request.into_inner()");

    if index.is_none() {
        panic!("Could not find 'let request = request.into_inner()' in fn body");
    }

    let index = index.unwrap();

    let to_inject = quote::quote! {
        let my_telemetry = my_grpc_extensions::get_telemetry(
            &request.metadata(),
            request.remote_addr(),
            #fn_name,
        );

        let my_telemetry = my_telemetry.get_ctx();
    };

    as_str.insert_str(index, to_inject.to_string().as_str());

    match proc_macro2::TokenStream::from_str(as_str.as_str()) {
        Ok(token_stream) => token_stream,
        Err(_) => panic!(
            "Somehow we did inject telemetry line wrong way. After Injection: {}",
            as_str
        ),
    }
}
