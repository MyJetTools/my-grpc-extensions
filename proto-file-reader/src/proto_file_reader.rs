use super::{ParamType, proto_tokens_reader::ProtoTokensReader};

#[derive(Debug)]
pub struct ProtoRpc {
    pub name: String,
    input_param: String,
    output_param: String,
}

impl ProtoRpc {
    pub fn get_fn_name<'s>(&'s self) -> ProtoString<'s> {
        ProtoString(&self.name)
    }

    pub fn get_input_param<'s>(&'s self) -> Option<ParamType<'s>> {
        ParamType::parse(&self.input_param)
    }

    pub fn get_output_param<'s>(&'s self) -> Option<ParamType<'s>> {
        ParamType::parse(&self.output_param)
    }
}

#[derive(Debug)]
pub struct ProtoServiceDescription {
    pub service_name: String,
    pub rpc: Vec<ProtoRpc>,
}

impl ProtoServiceDescription {
    pub fn get_service_name<'s>(&'s self) -> ProtoString<'s> {
        ProtoString::new(self.service_name.as_str())
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
        let content = std::fs::read_to_string(file_name);

        if let Err(err) = content {
            panic!("Can not open file: {}. Error: {:?}", file_name, err);
        }

        let content = super::strip_comments(content.unwrap().as_str());

        let mut service_name = None;

        let mut current_token = CurrentToken::None;

        let mut rpc_name = None;

        let mut input_param_name = String::new();

        let mut out_param_name = String::new();

        let mut rpc = Vec::new();

        for token in ProtoTokensReader::new(content.as_str()) {
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
                    service_name = Some(format!("{}", token));
                    current_token = CurrentToken::None;
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

    let chars: Vec<char> = src.chars().collect();

    for (index, &ch) in chars.iter().enumerate() {
        if ch.is_uppercase() {
            if index != 0 {
                let prev_upper = chars[index - 1].is_uppercase();
                let next_is_lower = index + 1 < chars.len() && chars[index + 1].is_lowercase();

                if !prev_upper || next_is_lower {
                    result.push('_');
                }
            }

            result.push(ch.to_lowercase().next().unwrap());
        } else {
            result.push(ch);
        }
    }

    result
}

pub struct ProtoString<'s>(&'s str);

impl<'s> ProtoString<'s> {
    pub fn new(str: &'s str) -> Self {
        Self(str)
    }
    pub fn as_snake_case(&self) -> String {
        into_snake_case(self.0)
    }

    pub fn as_str(&'s self) -> &'s str {
        &self.0
    }

    pub fn as_formatted_string(&'s self) -> String {
        format_proto_param_name(self.0)
    }
}


pub fn format_proto_param_name(src: &str)->String{
    let mut result = String::with_capacity(src.len());

    let mut prev: Option<char> = None;

    let mut amount_of_upper_key = 0;
    for c in src.chars(){

        match prev{
            Some(prev)=>{
                
                if prev.is_ascii_uppercase(){
                    
                    if c.is_ascii_lowercase(){
                        if amount_of_upper_key>1{
                        let c_prev = result.pop().unwrap();
                        result.push(c_prev.to_ascii_uppercase());
                    }
                    }
                    

                    result.push(c.to_ascii_lowercase());
                }else{                  

                    result.push(c);    
                }
           
            },
            None=>{
                result.push(c);
            }
        }

        if c.is_ascii_uppercase(){
            amount_of_upper_key +=1;
        }else{
            amount_of_upper_key +=0;
        }

         prev = Some(c);
        
    }


    result

}

#[cfg(test)]
mod tests {

    #[test]
    fn test_into_camel_case() {
        assert_eq!(super::into_snake_case("HelloWorld"), "hello_world");
    }

    #[test]
    fn test_several_capital() {
        assert_eq!(super::into_snake_case("CreateCRMStatus"), "create_crm_status");
    }

    #[test]
    fn test_format_param_name() {
        assert_eq!(super::format_proto_param_name("CreateCRMStatus"), "CreateCrmStatus");

        assert_eq!(super::format_proto_param_name("HelloWorld"), "HelloWorld");
    }
}
