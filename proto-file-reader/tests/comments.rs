use proto_file_reader::{ParamType, ProtoServiceDescription, strip_comments};

fn parse(name: &str, content: &str) -> ProtoServiceDescription {
    let path = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("{}.proto", name));
    std::fs::write(&path, content).unwrap();
    ProtoServiceDescription::read_proto_file(path.to_str().unwrap())
}

fn rpc_names(d: &ProtoServiceDescription) -> Vec<&str> {
    d.rpc.iter().map(|itm| itm.name.as_str()).collect()
}

/// A trailing comment is not a full line comment, so it used to leak its tokens into the parser:
/// the `rpc` word inside it started a new method and swallowed the one declared next.
#[test]
fn trailing_line_comment_does_not_eat_the_next_rpc() {
    let d = parse(
        "trailing_line_comment",
        r#"
syntax = "proto3";
service MyGrpcService {
  rpc GetItem(GetRequest) returns (GetResponse); // deprecated rpc GetOld
  rpc RealOne(RealRequest) returns (RealResponse);
}
"#,
    );

    assert_eq!(d.service_name, "MyGrpcService");
    assert_eq!(rpc_names(&d), vec!["GetItem", "RealOne"]);
}

/// The same leak, but the comment sits on a field of the model header and only mentions the word.
#[test]
fn trailing_comment_on_a_field_mentioning_rpc() {
    let d = parse(
        "trailing_comment_on_field",
        r#"
syntax = "proto3";
message GetRequest{
    string Id = 1; // the rpc id of the record
}
service MyGrpcService {
  rpc GetItem(GetRequest) returns (GetResponse);
}
"#,
    );

    assert_eq!(d.service_name, "MyGrpcService");
    assert_eq!(rpc_names(&d), vec!["GetItem"]);
}

#[test]
fn commented_out_rpc_is_not_generated() {
    let d = parse(
        "block_comment",
        r#"
syntax = "proto3";
service MyGrpcService {
  /* rpc Commented(A) returns (B); */
  rpc GetItem(GetRequest) returns (GetResponse);
  /*
     rpc MultilineCommented(A) returns (B);
  */
}
"#,
    );

    assert_eq!(rpc_names(&d), vec!["GetItem"]);
}

#[test]
fn params_survive_comments() {
    let d = parse(
        "params_with_comments",
        r#"
syntax = "proto3";
service MyGrpcService {
  // returns the items
  rpc GetItems(stream GetRequest) returns (stream GetResponse); // streamed rpc
}
"#,
    );

    let rpc = &d.rpc[0];
    assert!(matches!(rpc.get_input_param(), Some(ParamType::Stream("GetRequest"))));
    assert!(matches!(rpc.get_output_param(), Some(ParamType::Stream("GetResponse"))));
}

#[test]
fn strip_comments_keeps_positions_and_strings() {
    // Comments are blanked out, not removed, so the rest of the file keeps its offsets.
    let src = "rpc Get(A) returns (B); // note\nrpc Second(C) returns (D);";
    let result = strip_comments(src);
    assert_eq!(result.len(), src.len());
    assert!(!result.contains("note"));
    assert!(result.contains("rpc Second(C) returns (D);"));

    // `//` inside a string literal is not a comment.
    let src = r#"option (x) = { url: "http://host/path" };"#;
    assert_eq!(strip_comments(src), src);

    let src = "a /* b\n c */ d";
    let result = strip_comments(src);
    assert_eq!(result.len(), src.len());
    assert!(!result.contains('b'));
    assert!(!result.contains('c'));
    assert!(result.contains('a') && result.contains('d'));
    // Line breaks are kept, so line based tokenizing still sees the same lines.
    assert_eq!(result.lines().count(), src.lines().count());
}
