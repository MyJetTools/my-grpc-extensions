use rust_extensions::StrOrString;

#[derive(Debug)]
pub enum ParamType<'s> {
    Single(&'s str),
    Stream(&'s str),
}

impl<'s> ParamType<'s> {
    pub fn parse(src: &'s str) -> Option<Self> {
        let mut is_vec = false;

        let mut name = None;

        for param in src.split_ascii_whitespace() {
            if param == "stream" {
                is_vec = true;
            } else {
                name = Some(param);
                break;
            }
        }

        let name = name?;

        if name == "google.protobuf.Empty" {
            return Self::Single("()").into();
        }

        if is_vec {
            Self::Stream(name.split('.').last().unwrap()).into()
        } else {
            Self::Single(name.split('.').last().unwrap()).into()
        }
    }

    pub fn is_stream(&self) -> bool {
        match self {
            Self::Single(_) => false,
            Self::Stream(_) => true,
        }
    }

    pub fn get_name(&self) -> &str {
        match self {
            Self::Single(name) => name,
            Self::Stream(name) => name,
        }
    }

    pub fn get_input_param_invoke(&self) -> &'static str {
        match self {
            Self::Single(_) => "input_data",
            Self::Stream(_) => "input_data.get_consumer()",
        }
    }

    pub fn get_output_param_type(&'s self) -> StrOrString<'s> {
        match self {
            Self::Single(name) => (*name).into(),
            Self::Stream(name) => format!("tonic::Streaming<{}>", name).into(),
        }
    }
}
