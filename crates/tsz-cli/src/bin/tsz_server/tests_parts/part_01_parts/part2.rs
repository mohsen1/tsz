#[test]
fn test_quickinfo_member_call_property_at_member_start() {
    let mut server = make_server();
    server.open_files.insert(
        "/test.ts".to_string(),
        "interface I {\n    /** Doc */\n    m: () => void;\n}\nfunction f(x: I): void {\n    x.m();\n}\n"
            .to_string(),
    );
    let req = make_request(
        "quickinfo",
        // Cursor between `.` and `m` in `x.m()` (equivalent to fourslash marker `x./**/m()`).
        serde_json::json!({"file": "/test.ts", "line": 6, "offset": 6}),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("quickinfo should return a body");
    assert_eq!(
        body["displayString"].as_str().unwrap_or(""),
        "(property) I.m: () => void"
    );
    assert_eq!(body["documentation"], serde_json::json!("Doc"));

    let req_at_member = make_request(
        "quickinfo",
        serde_json::json!({"file": "/test.ts", "line": 6, "offset": 7}),
    );
    let resp_at_member = server.handle_tsserver_request(req_at_member);
    assert!(resp_at_member.success);
    let body_at_member = resp_at_member
        .body
        .expect("quickinfo at member should return a body");
    assert_eq!(
        body_at_member["displayString"].as_str().unwrap_or(""),
        "(property) I.m: () => void"
    );
    assert_eq!(body_at_member["documentation"], serde_json::json!("Doc"));
}

#[test]
fn test_quickinfo_documentation_is_protocol_string() {
    let mut server = make_server();
    server.open_files.insert(
        "/test.ts".to_string(),
        "/** Adds one. */\nfunction f(a: number): string { return String(a + 1); }\nf(1);\nconst n = 1;\nn;\n"
            .to_string(),
    );

    let documented = server.handle_tsserver_request(make_request(
        "quickinfo",
        serde_json::json!({"file": "/test.ts", "line": 3, "offset": 1}),
    ));
    assert!(documented.success);
    let documented_body = documented.body.expect("quickinfo should return a body");
    assert_eq!(
        documented_body["documentation"],
        serde_json::json!("Adds one.")
    );

    let undocumented = server.handle_tsserver_request(make_request(
        "quickinfo",
        serde_json::json!({"file": "/test.ts", "line": 5, "offset": 1}),
    ));
    assert!(undocumented.success);
    let undocumented_body = undocumented.body.expect("quickinfo should return a body");
    assert_eq!(undocumented_body["documentation"], serde_json::json!(""));
}

#[test]
fn test_quickinfo_new_expression_uses_constructor_signature() {
    let mut server = make_server();
    server.open_files.insert(
        "/test.ts".to_string(),
        "class A<T> {}\nnew A<string>();\n".to_string(),
    );
    let req = make_request(
        "quickinfo",
        // Hover over `A` in `new A<string>()`.
        serde_json::json!({"file": "/test.ts", "line": 2, "offset": 5}),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("quickinfo should return a body");
    assert_eq!(
        body["displayString"].as_str().unwrap_or(""),
        "constructor A<string>(): A<string>"
    );
    assert_eq!(body["kind"].as_str().unwrap_or(""), "constructor");
}

#[test]
fn test_quickinfo_arrow_token_uses_contextual_signature() {
    let mut server = make_server();
    server.open_files.insert(
        "/test.ts".to_string(),
        "// @strict: true\nconst optionals: ((a?: number) => unknown) & ((b?: string) => unknown) = (\n  arg,\n) => {};\n"
            .to_string(),
    );
    let req = make_request(
        "quickinfo",
        // Cursor on `=>` token of the arrow function.
        serde_json::json!({"file": "/test.ts", "line": 4, "offset": 4}),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("quickinfo should return a body");
    assert_eq!(
        body["displayString"].as_str().unwrap_or(""),
        "function(arg: string | number | undefined): void"
    );
    assert_eq!(body["kind"].as_str().unwrap_or(""), "function");
}

#[test]
fn test_quickinfo_parameter_uses_contextual_type() {
    let mut server = make_server();
    server.open_files.insert(
        "/test.ts".to_string(),
        "var c3t11: {(n: number, s: string): string;}[] = [function(/*25*/n, s) { return s; }];\n"
            .to_string(),
    );
    let req = make_request(
        "quickinfo",
        // Cursor on `n` after /*25*/.
        serde_json::json!({"file": "/test.ts", "line": 1, "offset": 66}),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body_on_identifier = resp.body.expect("quickinfo should return a body");
    assert_eq!(
        body_on_identifier["displayString"].as_str().unwrap_or(""),
        "(parameter) n: number"
    );
}

#[test]
fn test_quickinfo_contextual_object_literal_function_parameter() {
    let mut server = make_server();
    let source = "interface IFoo { f(i: number, s: string): string; }\nvar c = <IFoo>({ f: function(/*31*/i, s) { return s; } });\n";
    server
        .open_files
        .insert("/test.ts".to_string(), source.to_string());

    let second_line = source
        .lines()
        .nth(1)
        .expect("source should contain second line");
    let identifier_offset = second_line
        .find("/*31*/i")
        .expect("marker+identifier should exist in source second line")
        as u32
        + "/*31*/".len() as u32
        + 1;

    let req_at_identifier = make_request(
        "quickinfo",
        serde_json::json!({"file": "/test.ts", "line": 2, "offset": identifier_offset}),
    );
    let resp_at_identifier = server.handle_tsserver_request(req_at_identifier);
    assert!(resp_at_identifier.success);
    let body_at_identifier = resp_at_identifier
        .body
        .expect("quickinfo should return a body at identifier");
    assert_eq!(
        body_at_identifier["displayString"].as_str().unwrap_or(""),
        "(parameter) i: number"
    );
}

#[test]
fn test_quickinfo_contextual_object_literal_array_property_name() {
    let mut server = make_server();
    let source = "interface IFoo { a: number[]; }\nvar c = <IFoo>({\n    /*34*/a: []\n});\n";
    server
        .open_files
        .insert("/test.ts".to_string(), source.to_string());

    let third_line = source
        .lines()
        .nth(2)
        .expect("source should contain third line");
    let property_offset = third_line
        .find("/*34*/a")
        .expect("marker+property should exist in source third line")
        as u32
        + "/*34*/".len() as u32
        + 1;

    let req = make_request(
        "quickinfo",
        serde_json::json!({"file": "/test.ts", "line": 3, "offset": property_offset}),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("quickinfo should return a body");
    assert_eq!(
        body["displayString"].as_str().unwrap_or(""),
        "(property) IFoo.a: number[]"
    );
}

#[test]
fn test_quickinfo_contextual_class_property_assignment_function_parameter() {
    let mut server = make_server();
    let source = "class C {\n    foo: (i: number, s: string) => string;\n    constructor() {\n        this.foo = function(/*36*/i, s) {\n            return s;\n        }\n    }\n}\n";
    server
        .open_files
        .insert("/test.ts".to_string(), source.to_string());

    let fourth_line = source
        .lines()
        .nth(3)
        .expect("source should contain assignment line");
    let identifier_offset = fourth_line
        .find("/*36*/i")
        .expect("marker+identifier should exist in assignment line")
        as u32
        + "/*36*/".len() as u32
        + 1;

    let req = make_request(
        "quickinfo",
        serde_json::json!({"file": "/test.ts", "line": 4, "offset": identifier_offset}),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("quickinfo should return a body");
    assert_eq!(
        body["displayString"].as_str().unwrap_or(""),
        "(parameter) i: number"
    );
    assert_eq!(body["kind"].as_str().unwrap_or(""), "parameter");
}

#[test]
fn test_prepare_call_hierarchy_class_property_arrow_function() {
    let mut server = make_server();
    server.open_files.insert(
        "/test.ts".to_string(),
        "class C {\n    caller = () => {\n        this.callee();\n    }\n\n    callee = () => {\n    }\n}\n"
            .to_string(),
    );

    let req = make_request(
        "prepareCallHierarchy",
        serde_json::json!({
            "file": "/test.ts",
            "line": 6,
            "offset": 5
        }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp
        .body
        .expect("prepareCallHierarchy should return a body");
    let items = body
        .as_array()
        .expect("prepareCallHierarchy body should be an array");
    assert!(
        !items.is_empty(),
        "Expected at least one call hierarchy item for class property arrow function"
    );
    let first = &items[0];
    assert_eq!(first["name"].as_str().unwrap_or(""), "callee");
    assert_eq!(first["kind"].as_str().unwrap_or(""), "function");
}

#[test]
fn test_prepare_call_hierarchy_marker_comment_before_interface_method() {
    let mut server = make_server();
    server.open_files.insert(
        "/test.ts".to_string(),
        "interface I {\n    /**/foo(): void;\n}\n\nconst obj: I = { foo() {} };\nobj.foo();\n"
            .to_string(),
    );

    let req = make_request(
        "prepareCallHierarchy",
        // Cursor on the `/` in `/**/foo`.
        serde_json::json!({
            "file": "/test.ts",
            "line": 2,
            "offset": 5
        }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp
        .body
        .expect("prepareCallHierarchy should return a body");
    let items = body
        .as_array()
        .expect("prepareCallHierarchy body should be an array");
    assert!(
        !items.is_empty(),
        "Expected call hierarchy item for interface method marker comment probe"
    );
    let first = &items[0];
    assert_eq!(first["name"].as_str().unwrap_or(""), "foo");
    assert_eq!(first["kind"].as_str().unwrap_or(""), "method");
}
