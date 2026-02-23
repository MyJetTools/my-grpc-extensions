pub struct ProtoTokensReader<'s> {
    content: Vec<&'s [u8]>,
    pos: usize,
}

impl<'s> ProtoTokensReader<'s> {
    pub fn new(content: &'s str) -> Self {
        let content = clean_up_comments(content);
        Self { content, pos: 0 }
    }

    pub fn get_next(&mut self) -> Option<&'s str> {
        while self.content.len() > 0 {
            let first_line = *self.content.first()?;

            if let Some(result) = iterate_through_line(first_line, self.pos) {
                self.pos = result.1;
                return Some(result.0);
            }

            self.content.remove(0);
            self.pos = 0;
        }

        None
    }
}

fn iterate_through_line(line: &[u8], mut pos: usize) -> Option<(&str, usize)> {
    let mut start_pos = None;
    while pos < line.len() {
        let b = line[pos];
        if b <= 32 {
            if let Some(start_pos) = start_pos {
                let result = std::str::from_utf8(&line[start_pos..pos]).unwrap();
                return Some((result, pos));
            }
            pos += 1;
            continue;
        }

        if b == b'(' || b == b')' || b == b';' || b == b'{' || b == b'}' {
            if let Some(start_pos) = start_pos {
                let result = std::str::from_utf8(&line[start_pos..pos]).unwrap();
                return Some((result, pos));
            }

            let result = std::str::from_utf8(&line[pos..pos + 1]).unwrap();

            pos += 1;

            return Some((result, pos));
        }

        if start_pos.is_none() {
            start_pos = Some(pos);
        }

        pos += 1;
    }

    if let Some(start_pos) = start_pos {
        let result = std::str::from_utf8(&line[start_pos..pos]).unwrap();
        return Some((result, pos));
    }

    None
}

impl<'s> Iterator for ProtoTokensReader<'s> {
    type Item = &'s str;

    fn next(&mut self) -> Option<Self::Item> {
        self.get_next()
    }
}

fn clean_up_comments(src: &str) -> Vec<&[u8]> {
    let mut result = Vec::new();
    for itm in src.split('\n') {
        if itm.trim().starts_with("//") {
            continue;
        }
        result.push(itm.as_bytes());
    }

    result
}

#[cfg(test)]
mod tests {
    use super::ProtoTokensReader;

    #[test]
    fn test_basic_parse() {
        let src = "service KeyValueFlowsGrpcService { rpc Get(stream keyvalue.GetKeyValueGrpcRequestModel) returns (stream keyvalue.GetKeyValueGrpcResponseModel);}";

        let result = ProtoTokensReader::new(src).collect::<Vec<_>>();

        assert_eq!(result.len(), 16);

        let mut pos = 0;
        assert_eq!(result[pos], "service");

        pos += 1;
        assert_eq!(result[pos], "KeyValueFlowsGrpcService");
        pos += 1;
        assert_eq!(result[pos], "{");
        pos += 1;
        assert_eq!(result[pos], "rpc");
        pos += 1;
        assert_eq!(result[pos], "Get");
        pos += 1;
        assert_eq!(result[pos], "(");
        pos += 1;
        assert_eq!(result[pos], "stream");
        pos += 1;
        assert_eq!(result[pos], "keyvalue.GetKeyValueGrpcRequestModel");
        pos += 1;
        assert_eq!(result[pos], ")");
        pos += 1;
        assert_eq!(result[pos], "returns");
        pos += 1;
        assert_eq!(result[pos], "(");
        pos += 1;
        assert_eq!(result[pos], "stream");
        pos += 1;
        assert_eq!(result[pos], "keyvalue.GetKeyValueGrpcResponseModel");
        pos += 1;
        assert_eq!(result[pos], ")");
        pos += 1;
        assert_eq!(result[pos], ";");
        pos += 1;
        assert_eq!(result[pos], "}");
    }
}
