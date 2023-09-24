use proc_macro2::{Group, TokenStream, TokenTree};
use quote::ToTokens;

pub fn insert_inside_token(
    src: TokenStream,
    sequence: &[&str],
    tokens_to_insert: Vec<TokenTree>,
) -> Option<TokenStream> {
    let mut insert_is_done = false;
    let result = iterate_token_stream(
        src.into_token_stream(),
        sequence,
        &tokens_to_insert,
        &mut insert_is_done,
    );

    if !insert_is_done {
        return None;
    }
    quote::quote!(
        #(#result)*
    )
    .into()
}

fn iterate_token_stream(
    token_stream: TokenStream,
    sequence: &[&str],
    tokens_to_insert: &Vec<TokenTree>,
    insert_is_done: &mut bool,
) -> Vec<TokenTree> {
    let mut index_to_insert = 0;

    let mut found_index = 0;

    let mut result = Vec::new();
    for token in token_stream {
        match &token {
            proc_macro2::TokenTree::Group(group) => {
                let tokens = iterate_token_stream(
                    group.stream(),
                    sequence,
                    tokens_to_insert,
                    insert_is_done,
                );

                result.push(TokenTree::Group(Group::new(
                    group.delimiter(),
                    quote::quote!(
                        #(#tokens)*
                    ),
                )));
            }
            _ => {
                if !*insert_is_done {
                    if sequence[found_index] == token.to_string().as_str() {
                        if found_index == sequence.len() - 1 {
                            for token in tokens_to_insert {
                                result.insert(index_to_insert, token.clone());
                                index_to_insert += 1;
                            }

                            *insert_is_done = true;
                        }

                        if found_index == 0 {
                            index_to_insert = result.len();
                        }

                        found_index += 1;
                    } else {
                        found_index = 0;
                    }
                }

                result.push(token);
            }
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use proc_macro2::{TokenStream, TokenTree};
    use quote::ToTokens;

    use super::insert_inside_token;

    #[test]
    fn test() {
        let src = r#"#[allow(clippy :: async_yields_async, clippy :: diverging_sub_expression,
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

        let tokens = TokenStream::from_str(src).unwrap();

        let tokens_to_insert = TokenStream::from_str("let a = 5;").unwrap();

        let tokens_to_insert: Vec<proc_macro2::TokenTree> =
            tokens_to_insert.into_token_stream().into_iter().collect();

        let result = insert_inside_token(
            tokens,
            &["let", "request", "=", "request", ".", "into_inner"],
            tokens_to_insert,
        );

        print_result(result.unwrap());
    }

    fn print_result(result: TokenStream) {
        for token in result {
            match token {
                TokenTree::Group(group) => {
                    println!("Printing group");
                    print_result(group.stream());
                }
                TokenTree::Ident(_) => {
                    println!("Ident: {}", token.to_string());
                }
                TokenTree::Punct(_) => {
                    println!("Punct: {}", token.to_string());
                }
                TokenTree::Literal(_) => {
                    println!("Literal: {}", token.to_string());
                }
            }
        }
    }

    #[test]
    fn test_simple_case() {
        let src = r#"
        pub fn test(){
            let request = request.into_inner();
        }
        "#;

        let tokens = TokenStream::from_str(src).unwrap();

        let tokens_to_insert = TokenStream::from_str("let a = 5;").unwrap();

        let tokens_to_insert: Vec<proc_macro2::TokenTree> =
            tokens_to_insert.into_token_stream().into_iter().collect();

        let result = insert_inside_token(
            tokens,
            &["let", "request", "=", "request", ".", "into_inner"],
            tokens_to_insert,
        );

        println!("{}", result.unwrap().to_string());
    }
}
