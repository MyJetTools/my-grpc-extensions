use std::{
    io::{BufRead, BufReader},
    str::FromStr,
};

use super::{proto_tokens_reader::ProtoTokensReader, ParamType};

#[derive(Debug)]
pub struct ProtoRpc {
    pub name: String,
    input_param: String,
    output_param: String,
}

impl ProtoRpc {
    pub fn get_fn_name_as_token(&self) -> proc_macro2::TokenStream {
        proc_macro2::TokenStream::from_str(&into_snake_case(self.name.as_str())).unwrap()
    }

    pub fn get_input_param(&self) -> Option<ParamType> {
        ParamType::parse(&self.input_param)
    }

    pub fn get_output_param(&self) -> Option<ParamType> {
        ParamType::parse(&self.output_param)
    }
}

#[derive(Debug)]
pub struct ProtoServiceDescription {
    pub service_name: String,
    pub rpc: Vec<ProtoRpc>,
}

impl ProtoServiceDescription {
    pub fn get_service_name_as_token(&self) -> proc_macro2::TokenStream {
        proc_macro2::TokenStream::from_str(&self.service_name).unwrap()
    }

    pub fn has_method(&self, method_name: &str) -> bool {
        for rpc in &self.rpc {
            if rpc.name == method_name {
                return true;
            }
        }

        false
    }

    pub fn read_proto_file(file_name: &str) -> Self {
        let file = std::fs::File::open(file_name);

        if let Err(err) = file {
            panic!("Can not open file: {}. Error: {:?}", file_name, err);
        }

        let file = file.unwrap();

        let reader = BufReader::new(file);

        let mut service_name = None;

        let mut current_token = CurrentToken::None;

        let mut rpc_name = None;

        let mut input_param_name = String::new();

        let mut out_param_name = String::new();

        let mut rpc = Vec::new();

        for line in reader.lines() {
            let line = line.unwrap();

            for token in ProtoTokensReader::new(line.as_str()) {
                match current_token {
                    CurrentToken::None => {
                        if token == "service" {
                            current_token = CurrentToken::Service;
                        }

                        if token == "rpc" {
                            current_token = CurrentToken::Rpc;
                        }
                    }
                    CurrentToken::Rpc => {
                        rpc_name = Some(token.to_string());

                        input_param_name.clear();
                        out_param_name.clear();

                        current_token = CurrentToken::RpcExpectingInputParameter;
                    }
                    CurrentToken::RpcExpectingInputParameter => {
                        if token == "(" {
                            continue;
                        }

                        if token == ")" {
                            current_token = CurrentToken::RpcExpectingOutputParameter;
                            continue;
                        }

                        if input_param_name.len() > 0 {
                            input_param_name.push(' ');
                        }
                        input_param_name.push_str(token);
                    }

                    CurrentToken::RpcExpectingOutputParameter => {
                        if token == "returns" {
                            continue;
                        }

                        if token == "(" {
                            continue;
                        }

                        if token == ")" {
                            continue;
                        }

                        if token == ";" {
                            if rpc_name.is_none() {
                                panic!("Somehow rpc_name is null");
                            }

                            let name = rpc_name.as_ref().unwrap();

                            if name != "Ping" {
                                rpc.push(ProtoRpc {
                                    name: name.to_string(),
                                    input_param: input_param_name.to_string(),
                                    output_param: out_param_name.to_string(),
                                });
                            }
                            current_token = CurrentToken::None;
                        }

                        if out_param_name.len() > 0 {
                            out_param_name.push(' ');
                        }
                        out_param_name.push_str(token);
                    }
                    CurrentToken::Service => {
                        service_name = Some(format!("{}Client", token));
                        current_token = CurrentToken::None;
                    }
                }
            }
        }

        if service_name.is_none() {
            panic!("Can not find service name in proto file: {}", file_name);
        }

        Self {
            service_name: service_name.unwrap().to_string(),
            rpc,
        }
    }
}

pub enum CurrentToken {
    None,
    Service,
    Rpc,
    RpcExpectingInputParameter,
    RpcExpectingOutputParameter,
}

pub fn into_snake_case(src: &str) -> String {
    let mut result = String::new();

    for (index, ch) in src.chars().enumerate() {
        if ch.is_uppercase() {
            if index != 0 {
                result.push('_');
            }

            result.push(ch.to_lowercase().next().unwrap());
        } else {
            result.push(ch);
        }
    }

    result
}

#[cfg(test)]
mod tests {

    #[test]
    fn test_into_camel_case() {
        assert_eq!(super::into_snake_case("HelloWorld"), "hello_world");
    }
}
