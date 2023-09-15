use std::collections::HashMap;

use proc_macro2::TokenStream;
use types_reader::ParamsList;

pub struct FnOverride<'s> {
    pub retries: usize,
    pub token_stream: &'s TokenStream,
}

impl<'s> FnOverride<'s> {
    pub fn new(attributes: &'s ParamsList) -> Result<HashMap<String, Self>, syn::Error> {
        let overrides = attributes.try_get_named_param("overrides");

        if overrides.is_none() {
            return Ok(HashMap::new());
        }

        let tokens_list = overrides.unwrap();

        let overrides = tokens_list.unwrap_as_object_list()?;

        let mut result = HashMap::new();

        for item in overrides.iter() {
            let name = item
                .get_named_param("fn_name")?
                .unwrap_as_string_value()?
                .to_string();
            result.insert(
                name,
                FnOverride {
                    retries: item
                        .get_named_param("retries")?
                        .unwrap_as_number_value()?
                        .as_usize(),
                    token_stream: item.get_token_stream(),
                },
            );
        }

        if result.len() == 0 {
            return Err(tokens_list.throw_error("Overrides list can not be empty. Just remove field if you do not want to override any function"));
        }

        Ok(result)
    }
}
