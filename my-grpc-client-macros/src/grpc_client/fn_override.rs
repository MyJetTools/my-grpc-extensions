use std::collections::HashMap;

use types_reader::TokensObject;

pub struct FnOverride<'s> {
    pub retries: usize,
    pub token_stream: &'s TokensObject,
}

impl<'s> FnOverride<'s> {
    pub fn new(params: &'s TokensObject) -> Result<HashMap<String, Self>, syn::Error> {
        let overrides = params.try_get_named_param("overrides");

        if overrides.is_none() {
            return Ok(HashMap::new());
        }

        let overrides = overrides.unwrap().unwrap_as_vec()?;

        let mut result = HashMap::new();

        for item in overrides.iter() {
            let name: String = item.get_named_param("fn_name")?.try_into()?;

            let retries: usize = item.get_named_param("retries")?.try_into()?;
            result.insert(
                name,
                FnOverride {
                    retries,
                    token_stream: item,
                },
            );
        }

        if result.len() == 0 {
            return Err(params.throw_error_at_param_token("Overrides list can not be empty. Just remove field if you do not want to override any function"));
        }

        Ok(result)
    }
}
