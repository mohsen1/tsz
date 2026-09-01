use std::ffi::OsString;
use std::io::{Cursor, Read, Write};
use std::process::{Command, Stdio};

use serde_json::{Value, json};

#[test]
fn process_flags_preserve_the_harness_surface() {
    let arguments = [
        "--project",
        "fixture",
        "--noEmit",
        "--pretty",
        "false",
        "--strict",
        "--extendedDiagnostics",
    ]
    .into_iter()
    .map(OsString::from)
    .collect::<Vec<_>>();
    let invocation = tsz_cli::driver::parse_arguments(&arguments).unwrap();
    assert_eq!(invocation.project.unwrap().to_string_lossy(), "fixture");
    assert_eq!(invocation.options.no_emit, Some(true));
    assert_eq!(invocation.options.strict, Some(true));
    assert!(!invocation.pretty);
    assert!(invocation.extended_diagnostics);
    assert!(invocation.command_line_diagnostics.is_empty());
}

#[test]
fn behavior_options_survive_cli_parsing_as_typed_deferred_inputs() {
    use tsz::{DeferredCompilerOption as OptionKey, DeferredCompilerOptionValue as OptionValue};

    let arguments = [
        "--alwaysStrict=true",
        "--moduleResolution",
        "node16",
        "--outFile",
        "./bundle.js",
        "--jsxFactory",
        "renamedFactory",
    ]
    .into_iter()
    .map(OsString::from)
    .collect::<Vec<_>>();
    let invocation = tsz_cli::driver::parse_arguments(&arguments).unwrap();

    assert_eq!(
        invocation
            .options
            .deferred_options
            .get(&OptionKey::AlwaysStrict),
        Some(&OptionValue::Boolean(true))
    );
    assert_eq!(
        invocation
            .options
            .deferred_options
            .get(&OptionKey::ModuleResolution),
        Some(&OptionValue::String("node16".to_string()))
    );
    assert_eq!(
        invocation.options.deferred_options.get(&OptionKey::OutFile),
        Some(&OptionValue::Path(std::path::PathBuf::from("./bundle.js")))
    );
    assert_eq!(
        invocation
            .options
            .deferred_options
            .get(&OptionKey::JsxFactory),
        Some(&OptionValue::String("renamedFactory".to_string()))
    );
    assert!(invocation.command_line_diagnostics.is_empty());
}

#[test]
fn tsserver_uses_content_length_and_reports_unsupported_commands_honestly() {
    let requests = [
        json!({
            "seq": 1,
            "type": "request",
            "command": "configure",
            "arguments": {"preferences": {"quotePreference": "single"}}
        }),
        json!({
            "seq": 2,
            "type": "request",
            "command": "compilerOptionsForInferredProjects",
            "arguments": {"options": {"strict": true}}
        }),
        json!({
            "seq": 3,
            "type": "request",
            "command": "open",
            "arguments": {"file": "case.ts", "fileContent": "const x: number = 'bad';"}
        }),
        json!({
            "seq": 4,
            "type": "request",
            "command": "semanticDiagnosticsSync",
            "arguments": {"file": "case.ts"}
        }),
        json!({
            "seq": 5,
            "type": "request",
            "command": "quickinfo",
            "arguments": {"file": "case.ts", "line": 1, "offset": 7}
        }),
        json!({
            "seq": 6,
            "type": "request",
            "command": "notImplemented",
            "arguments": {}
        }),
        json!({
            "seq": 7,
            "type": "request",
            "command": "tsz/reset",
            "arguments": {}
        }),
    ];
    let mut input = Vec::new();
    for request in requests {
        let body = serde_json::to_vec(&request).unwrap();
        input.extend_from_slice(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes());
        input.extend_from_slice(&body);
    }
    let mut output = Vec::new();
    tsz_cli::tsserver::run_tsserver(Cursor::new(input), &mut output).unwrap();
    let responses = decode_messages(&output);
    assert_eq!(responses.len(), 7);
    assert_eq!(responses[0]["success"], true);
    assert!(responses[0].get("body").is_none());
    assert_eq!(responses[1]["success"], true);
    assert_eq!(responses[1]["body"], true);
    assert_eq!(responses[2]["success"], true);
    assert!(responses[2].get("body").is_none());
    assert_eq!(responses[3]["body"][0]["code"], 2322);
    assert_eq!(responses[4]["body"]["displayString"], "const x: number");
    assert_eq!(responses[5]["success"], false);
    assert_eq!(responses[6]["command"], "tsz/reset");
    assert_eq!(responses[6]["request_seq"], 7);
    assert_eq!(responses[6]["success"], true);
}

#[test]
fn tsserver_preserves_native_check_js_file_modes_and_completion() {
    let requests = [
        json!({
            "seq": 1, "type": "request", "command": "compilerOptionsForInferredProjects",
            "arguments": {"options": {"checkJs": true}}
        }),
        json!({
            "seq": 2, "type": "request", "command": "configure",
            "arguments": {
                "preferences": {"quotePreference": "single"},
                "compilerOptions": {"allowJs": false, "checkJs": false}
            }
        }),
        json!({
            "seq": 3, "type": "request", "command": "open",
            "arguments": {"file": "checked.js", "fileContent": "MissingChecked;"}
        }),
        json!({
            "seq": 4, "type": "request", "command": "semanticDiagnosticsSync",
            "arguments": {"file": "checked.js"}
        }),
        json!({
            "seq": 5, "type": "request", "command": "compilerOptionsForInferredProjects",
            "arguments": {"options": {"allowJs": true, "checkJs": false}}
        }),
        json!({
            "seq": 6, "type": "request", "command": "open",
            "arguments": {
                "file": "Foo.js",
                "fileContent": concat!(
                    "/** @param {function ({OwnerID:string,AwayID:string}):void} x\n",
                    "  * @param {function (string):void} y */\n",
                    "function fn(x, y) { }",
                )
            }
        }),
        json!({
            "seq": 7, "type": "request", "command": "semanticDiagnosticsSync",
            "arguments": {"file": "Foo.js"}
        }),
        json!({
            "seq": 8, "type": "request", "command": "open",
            "arguments": {"file": "consumer.ts", "fileContent": "const typed: string = 1;"}
        }),
        json!({
            "seq": 9, "type": "request", "command": "semanticDiagnosticsSync",
            "arguments": {"file": "consumer.ts"}
        }),
    ];
    let mut input = Vec::new();
    for request in requests {
        let body = serde_json::to_vec(&request).unwrap();
        input.extend_from_slice(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes());
        input.extend_from_slice(&body);
    }
    let mut output = Vec::new();
    tsz_cli::tsserver::run_tsserver(Cursor::new(input), &mut output).unwrap();
    let responses = decode_messages(&output);

    assert_eq!(responses.len(), 9);
    assert_eq!(responses[0]["body"], true);
    assert!(responses[1].get("body").is_none());
    assert_eq!(responses[3]["success"], true);
    assert_eq!(responses[3]["body"][0]["code"], 2304);
    assert_eq!(responses[4]["body"], true);
    assert_eq!(responses[6]["success"], true);
    assert_eq!(responses[6]["body"], json!([]));
    assert_eq!(responses[8]["success"], true);
    assert_eq!(responses[8]["body"][0]["code"], 2322);
}

#[test]
fn tsserver_exposes_the_exact_open_snapshot_for_harness_consistency_checks() {
    let path = "nested/renamed-consistency.ts";
    let initial = "const icon = \"😀\";\nlet value = 1;";
    let changed = "const icon = \"é\";\nlet value = 1;";
    let requests = [
        json!({
            "seq": 1,
            "type": "request",
            "command": "open",
            "arguments": {"file": path, "fileContent": initial}
        }),
        json!({
            "seq": 2,
            "type": "request",
            "command": "tsz/text",
            "arguments": {"file": path}
        }),
        json!({
            "seq": 3,
            "type": "request",
            "command": "change",
            "arguments": {
                "file": path,
                "line": 1,
                "offset": 15,
                "endLine": 1,
                "endOffset": 17,
                "insertString": "é"
            }
        }),
        json!({
            "seq": 4,
            "type": "request",
            "command": "tsz/text",
            "arguments": {"file": path}
        }),
        json!({
            "seq": 5,
            "type": "request",
            "command": "tsz/reset",
            "arguments": {}
        }),
        json!({
            "seq": 6,
            "type": "request",
            "command": "tsz/text",
            "arguments": {"file": path}
        }),
    ];
    let mut input = Vec::new();
    for request in requests {
        let body = serde_json::to_vec(&request).unwrap();
        input.extend_from_slice(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes());
        input.extend_from_slice(&body);
    }
    let mut output = Vec::new();
    tsz_cli::tsserver::run_tsserver(Cursor::new(input), &mut output).unwrap();
    let responses = decode_messages(&output);

    assert_eq!(responses.len(), 6);
    assert_eq!(responses[1]["success"], true);
    assert_eq!(responses[1]["body"], initial);
    assert_eq!(responses[3]["success"], true);
    assert_eq!(responses[3]["body"], changed);
    assert_eq!(responses[5]["success"], false);
    assert_eq!(responses[5]["message"], format!("File is not open: {path}"));
}

#[test]
fn tsserver_diagnostics_select_the_exact_session_client_wire_shape() {
    let diagnostic_path = "renamed/nested/wire-contract.ts";
    let clean_path = "renamed/nested/empty-control.ts";
    let requests = [
        json!({
            "seq": 1,
            "type": "request",
            "command": "open",
            "arguments": {
                "file": diagnostic_path,
                "fileContent": "const icon = \"😀\";\nconst count: number = \"wrong\";"
            }
        }),
        json!({
            "seq": 2,
            "type": "request",
            "command": "semanticDiagnosticsSync",
            "arguments": {"file": diagnostic_path, "includeLinePosition": true}
        }),
        json!({
            "seq": 3,
            "type": "request",
            "command": "semanticDiagnosticsSync",
            "arguments": {"file": diagnostic_path, "includeLinePosition": false}
        }),
        json!({
            "seq": 4,
            "type": "request",
            "command": "semanticDiagnosticsSync",
            "arguments": {"file": diagnostic_path}
        }),
        json!({
            "seq": 5,
            "type": "request",
            "command": "semanticDiagnosticsSync",
            "arguments": {"file": diagnostic_path, "includeLinePosition": null}
        }),
        json!({
            "seq": 6,
            "type": "request",
            "command": "semanticDiagnosticsSync",
            "arguments": {"file": diagnostic_path, "includeLinePosition": 0}
        }),
        json!({
            "seq": 7,
            "type": "request",
            "command": "semanticDiagnosticsSync",
            "arguments": {"file": diagnostic_path, "includeLinePosition": ""}
        }),
        json!({
            "seq": 8,
            "type": "request",
            "command": "semanticDiagnosticsSync",
            "arguments": {"file": diagnostic_path, "includeLinePosition": "malformed"}
        }),
        json!({
            "seq": 9,
            "type": "request",
            "command": "open",
            "arguments": {
                "file": clean_path,
                "fileContent": "const café: number = 1;"
            }
        }),
        json!({
            "seq": 10,
            "type": "request",
            "command": "semanticDiagnosticsSync",
            "arguments": {"file": clean_path, "includeLinePosition": true}
        }),
        json!({
            "seq": 11,
            "type": "request",
            "command": "syntacticDiagnosticsSync",
            "arguments": {"file": clean_path, "includeLinePosition": false}
        }),
    ];
    let mut input = Vec::new();
    for request in requests {
        let body = serde_json::to_vec(&request).unwrap();
        input.extend_from_slice(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes());
        input.extend_from_slice(&body);
    }

    let mut output = Vec::new();
    tsz_cli::tsserver::run_tsserver(Cursor::new(input), &mut output).unwrap();
    let responses = decode_messages(&output);
    assert_eq!(responses.len(), 11);

    let with_line_position = json!([{
        "message": "Type 'string' is not assignable to type 'number'.",
        "start": 25,
        "length": 5,
        "startLocation": {"line": 2, "offset": 7},
        "endLocation": {"line": 2, "offset": 12},
        "category": "error",
        "code": 2322,
    }]);
    let event_form = json!([{
        "start": {"line": 2, "offset": 7},
        "end": {"line": 2, "offset": 12},
        "text": "Type 'string' is not assignable to type 'number'.",
        "category": "error",
        "code": 2322,
    }]);

    assert_eq!(responses[1]["body"], with_line_position);
    assert_eq!(responses[2]["body"], event_form);
    assert_eq!(responses[3]["body"], event_form);
    assert_eq!(responses[4]["body"], event_form);
    assert_eq!(responses[5]["body"], event_form);
    assert_eq!(responses[6]["body"], event_form);
    assert_eq!(responses[7]["body"], with_line_position);
    assert_eq!(responses[9]["body"], json!([]));
    assert_eq!(responses[10]["body"], json!([]));
}

#[test]
fn tsserver_flattens_nested_semantic_message_chains_in_both_wire_shapes() {
    let path = "renamed/nested/message-chain.ts";
    let source = concat!(
        "const icon=\"😀\"; const nested=[[\"other\"]]; ",
        "const target:\"seed\"[][]=nested;",
    );
    let requests = [
        json!({
            "seq": 1, "type": "request", "command": "open",
            "arguments": {"file": path, "fileContent": source}
        }),
        json!({
            "seq": 2, "type": "request", "command": "semanticDiagnosticsSync",
            "arguments": {"file": path, "includeLinePosition": true}
        }),
        json!({
            "seq": 3, "type": "request", "command": "semanticDiagnosticsSync",
            "arguments": {"file": path, "includeLinePosition": false}
        }),
    ];
    let mut input = Vec::new();
    for request in requests {
        let body = serde_json::to_vec(&request).unwrap();
        input.extend_from_slice(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes());
        input.extend_from_slice(&body);
    }
    let mut output = Vec::new();
    tsz_cli::tsserver::run_tsserver(Cursor::new(input), &mut output).unwrap();
    let responses = decode_messages(&output);
    let expected_message = concat!(
        "Type 'string[][]' is not assignable to type '\"seed\"[][]'.\n",
        "  Type 'string[]' is not assignable to type '\"seed\"[]'.\n",
        "    Type 'string' is not assignable to type '\"seed\"'.",
    );
    let diagnostic_byte = source.find("target").unwrap();
    let diagnostic_utf16 = source[..diagnostic_byte].encode_utf16().count() as u64;

    assert_eq!(responses.len(), 3);
    assert_eq!(responses[1]["body"][0]["message"], expected_message);
    assert_eq!(responses[2]["body"][0]["text"], expected_message);
    assert_eq!(responses[1]["body"][0]["start"], diagnostic_utf16);
    assert_eq!(
        responses[1]["body"][0]["startLocation"],
        json!({"line": 1, "offset": diagnostic_utf16 + 1})
    );
    assert_eq!(
        responses[2]["body"][0]["start"],
        json!({"line": 1, "offset": diagnostic_utf16 + 1})
    );
}

#[test]
fn tsserver_semantic_diagnostics_fail_without_fabricating_a_diagnostic_body() {
    let source = "const text:string=''; const textSize:number=text.length; \
                  const values:number[]=[]; const count:number=values.length;";
    let requests = [
        json!({
            "seq": 1,
            "type": "request",
            "command": "open",
            "arguments": {"file": "case.ts", "fileContent": source}
        }),
        json!({
            "seq": 2,
            "type": "request",
            "command": "syntacticDiagnosticsSync",
            "arguments": {"file": "case.ts"}
        }),
        json!({
            "seq": 3,
            "type": "request",
            "command": "semanticDiagnosticsSync",
            "arguments": {"file": "case.ts"}
        }),
    ];
    let mut input = Vec::new();
    for request in requests {
        let body = serde_json::to_vec(&request).unwrap();
        input.extend_from_slice(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes());
        input.extend_from_slice(&body);
    }

    let mut output = Vec::new();
    tsz_cli::tsserver::run_tsserver(Cursor::new(input), &mut output).unwrap();
    let responses = decode_messages(&output);
    assert_eq!(responses.len(), 3);
    assert_eq!(responses[1]["success"], true);
    assert_eq!(responses[1]["body"], json!([]));
    assert_eq!(responses[2]["success"], false);
    assert!(responses[2].get("body").is_none());
    assert_eq!(
        responses[2]["message"],
        "TSZ semantic diagnostics incomplete: deferred"
    );
}

#[test]
fn tsserver_syntactic_diagnostics_fail_for_generic_and_decorator_recovery() {
    let requests = [
        json!({
            "seq": 1,
            "type": "request",
            "command": "open",
            "arguments": {
                "file": "generic.ts",
                "fileContent": "declare const use: (...items:any[])=>void; use(<Cedar,>(): => 1);"
            }
        }),
        json!({
            "seq": 2,
            "type": "request",
            "command": "syntacticDiagnosticsSync",
            "arguments": {"file": "generic.ts"}
        }),
        json!({
            "seq": 3,
            "type": "request",
            "command": "open",
            "arguments": {
                "file": "decorator.ts",
                "fileContent": "class Holder { @decorate(\"x\", true) method() {} }"
            }
        }),
        json!({
            "seq": 4,
            "type": "request",
            "command": "syntacticDiagnosticsSync",
            "arguments": {"file": "decorator.ts"}
        }),
    ];
    let mut input = Vec::new();
    for request in requests {
        let body = serde_json::to_vec(&request).unwrap();
        input.extend_from_slice(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes());
        input.extend_from_slice(&body);
    }

    let mut output = Vec::new();
    tsz_cli::tsserver::run_tsserver(Cursor::new(input), &mut output).unwrap();
    let responses = decode_messages(&output);
    assert_eq!(responses.len(), 4);
    for response in [&responses[1], &responses[3]] {
        assert_eq!(response["success"], false);
        assert!(response.get("body").is_none());
        assert_eq!(
            response["message"],
            "TSZ syntactic diagnostics incomplete: deferred"
        );
    }
}

#[test]
fn tsserver_keeps_malformed_namespace_syntax_separate_from_semantic_nonclaim() {
    let source = "namespace Container; type Kept = MissingSibling;";
    let requests = [
        json!({
            "seq": 1, "type": "request", "command": "open",
            "arguments": {"file": "host.ts", "fileContent": source}
        }),
        json!({
            "seq": 2, "type": "request", "command": "syntacticDiagnosticsSync",
            "arguments": {"file": "host.ts"}
        }),
        json!({
            "seq": 3, "type": "request", "command": "semanticDiagnosticsSync",
            "arguments": {"file": "host.ts"}
        }),
    ];
    let mut input = Vec::new();
    for request in requests {
        let body = serde_json::to_vec(&request).unwrap();
        input.extend_from_slice(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes());
        input.extend_from_slice(&body);
    }

    let mut output = Vec::new();
    tsz_cli::tsserver::run_tsserver(Cursor::new(input), &mut output).unwrap();
    let responses = decode_messages(&output);
    assert_eq!(responses.len(), 3);
    assert_eq!(responses[1]["success"], true);
    assert_eq!(
        responses[1]["body"],
        json!([{
            "start": {"line": 1, "offset": 20},
            "end": {"line": 1, "offset": 21},
            "text": "'{' expected.",
            "category": "error",
            "code": 1005,
        }]),
    );
    assert_eq!(responses[2]["success"], false);
    assert!(responses[2].get("body").is_none());
    assert_eq!(
        responses[2]["message"],
        "TSZ semantic diagnostics incomplete: deferred",
    );
}

#[test]
fn tsserver_quickinfo_frames_exact_object_shapes_and_complete_any_fallbacks() {
    let requests = [
        json!({
            "seq": 1,
            "type": "request",
            "command": "open",
            "arguments": {
                "file": "case.ts",
                "fileContent": "const item = { count: 1 };\nconst unresolved = create();"
            }
        }),
        json!({
            "seq": 2,
            "type": "request",
            "command": "quickinfo",
            "arguments": {"file": "case.ts", "line": 1, "offset": 7}
        }),
        json!({
            "seq": 3,
            "type": "request",
            "command": "quickinfo",
            "arguments": {"file": "case.ts", "line": 2, "offset": 7}
        }),
    ];
    let mut input = Vec::new();
    for request in requests {
        let body = serde_json::to_vec(&request).unwrap();
        input.extend_from_slice(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes());
        input.extend_from_slice(&body);
    }

    let mut output = Vec::new();
    tsz_cli::tsserver::run_tsserver(Cursor::new(input), &mut output).unwrap();
    let responses = decode_messages(&output);
    assert_eq!(responses.len(), 3);
    assert_eq!(responses[0]["success"], true);
    assert_eq!(responses[1]["success"], true);
    assert_eq!(
        responses[1]["body"]["displayString"],
        "const item: { count: number; }"
    );
    assert_eq!(responses[2]["success"], true);
    assert_eq!(
        responses[2]["body"]["displayString"],
        "const unresolved: any"
    );
}

#[test]
fn tsserver_navigation_commands_share_bound_identity_and_protocol_shapes() {
    let source = "const shared = 1;\nfunction wrap(shared: number) { return shared; }\nshared;\n";
    let requests = [
        json!({
            "seq": 1,
            "type": "request",
            "command": "open",
            "arguments": {"file": "a.ts", "fileContent": source}
        }),
        json!({
            "seq": 2,
            "type": "request",
            "command": "open",
            "arguments": {"file": "b.ts", "fileContent": "shared;\n"}
        }),
        json!({
            "seq": 3,
            "type": "request",
            "command": "definitionAndBoundSpan",
            "arguments": {"file": "b.ts", "line": 1, "offset": 2}
        }),
        json!({
            "seq": 4,
            "type": "request",
            "command": "definition",
            "arguments": {"file": "b.ts", "line": 1, "offset": 2}
        }),
        json!({
            "seq": 5,
            "type": "request",
            "command": "references-full",
            "arguments": {"file": "a.ts", "line": 1, "offset": 8}
        }),
        json!({
            "seq": 6,
            "type": "request",
            "command": "documentHighlights",
            "arguments": {
                "file": "a.ts",
                "line": 1,
                "offset": 8,
                "filesToSearch": ["b.ts"]
            }
        }),
        json!({
            "seq": 7,
            "type": "request",
            "command": "rename",
            "arguments": {"file": "a.ts", "line": 1, "offset": 8}
        }),
        json!({
            "seq": 8,
            "type": "request",
            "command": "rename",
            "arguments": {"file": "a.ts", "line": 1, "offset": 1}
        }),
        json!({
            "seq": 9,
            "type": "request",
            "command": "open",
            "arguments": {
                "file": "types.ts",
                "fileContent": "type Alias = string;\nconst item: Alias = '';"
            }
        }),
        json!({
            "seq": 10,
            "type": "request",
            "command": "typeDefinition",
            "arguments": {"file": "types.ts", "line": 2, "offset": 14}
        }),
        json!({
            "seq": 11,
            "type": "request",
            "command": "open",
            "arguments": {"file": "unicode.ts", "fileContent": "const café = 1; café;"}
        }),
        json!({
            "seq": 12,
            "type": "request",
            "command": "references-full",
            "arguments": {"file": "unicode.ts", "line": 1, "offset": 8}
        }),
    ];
    let mut input = Vec::new();
    for request in requests {
        let body = serde_json::to_vec(&request).unwrap();
        input.extend_from_slice(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes());
        input.extend_from_slice(&body);
    }

    let mut output = Vec::new();
    tsz_cli::tsserver::run_tsserver(Cursor::new(input), &mut output).unwrap();
    let wire = String::from_utf8_lossy(&output);
    assert!(wire.contains(concat!(
        r#""containerKind":"","containerName":"","fileName":"a.ts","kind":"const","#,
        r#""name":"const shared: 1""#,
    )));
    assert!(wire.contains(r#""displayParts":[{"text":"const","kind":"keyword"}"#));
    assert!(wire.contains(r#""isWriteAccess":true,"isDefinition":true"#));
    let responses = decode_messages(&output);
    assert_eq!(responses.len(), 12);

    let definition_and_bound = &responses[2]["body"];
    assert_eq!(definition_and_bound["definitions"][0]["file"], "a.ts");
    assert_eq!(definition_and_bound["definitions"][0]["kind"], "const");
    assert_eq!(definition_and_bound["definitions"][0]["name"], "shared");
    assert_eq!(definition_and_bound["definitions"][0]["containerKind"], "");
    assert_eq!(definition_and_bound["definitions"][0]["isLocal"], false);
    assert_eq!(definition_and_bound["definitions"][0]["isAmbient"], false);
    assert_eq!(definition_and_bound["definitions"][0]["unverified"], false);
    assert_eq!(
        definition_and_bound["definitions"][0]["failedAliasResolution"],
        false
    );
    assert_eq!(
        definition_and_bound["definitions"][0]["start"],
        json!({"line": 1, "offset": 7})
    );
    assert_eq!(
        definition_and_bound["textSpan"],
        json!({
            "start": {"line": 1, "offset": 1},
            "end": {"line": 1, "offset": 7}
        })
    );
    assert_eq!(responses[3]["body"][0]["file"], "a.ts");

    let references = &responses[4]["body"][0];
    assert_eq!(references["definition"]["fileName"], "a.ts");
    assert_eq!(
        references["definition"]["textSpan"],
        json!({"start": 6, "length": 6})
    );
    assert_eq!(references["references"].as_array().unwrap().len(), 3);
    assert_eq!(references["references"][0]["isDefinition"], true);

    assert_eq!(responses[5]["body"].as_array().unwrap().len(), 1);
    assert_eq!(responses[5]["body"][0]["file"], "b.ts");
    assert_eq!(
        responses[5]["body"][0]["highlightSpans"][0]["kind"],
        "reference"
    );

    let rename = &responses[6]["body"];
    assert_eq!(rename["info"]["canRename"], true);
    assert_eq!(rename["info"]["displayName"], "shared");
    assert_eq!(
        rename["info"]["triggerSpan"],
        json!({
            "start": {"line": 1, "offset": 7},
            "end": {"line": 1, "offset": 13},
            "length": 6
        })
    );
    assert_eq!(rename["locs"].as_array().unwrap().len(), 2);
    assert_eq!(responses[7]["body"]["info"]["canRename"], false);
    assert_eq!(
        responses[7]["body"]["info"]["localizedErrorMessage"],
        "You cannot rename this element."
    );
    assert_eq!(responses[9]["body"][0]["file"], "types.ts");
    assert_eq!(
        responses[9]["body"][0]["start"],
        json!({"line": 1, "offset": 6})
    );
    assert_eq!(
        responses[11]["body"][0]["definition"]["textSpan"],
        json!({"start": 6, "length": 4})
    );
}

#[test]
fn tsserver_reports_local_definition_metadata_and_identifier_rename_span() {
    let requests = [
        json!({
            "seq": 1,
            "type": "request",
            "command": "open",
            "arguments": {
                "file": "local.ts",
                "fileContent": "function wrap() { var local; local = 1; }"
            }
        }),
        json!({
            "seq": 2,
            "type": "request",
            "command": "definition",
            "arguments": {"file": "local.ts", "line": 1, "offset": 31}
        }),
        json!({
            "seq": 3,
            "type": "request",
            "command": "definitionAndBoundSpan",
            "arguments": {"file": "local.ts", "line": 1, "offset": 31}
        }),
        json!({
            "seq": 4,
            "type": "request",
            "command": "open",
            "arguments": {"file": "class.ts", "fileContent": "class C {}"}
        }),
        json!({
            "seq": 5,
            "type": "request",
            "command": "rename",
            "arguments": {"file": "class.ts", "line": 1, "offset": 7}
        }),
    ];
    let mut input = Vec::new();
    for request in requests {
        let body = serde_json::to_vec(&request).unwrap();
        input.extend_from_slice(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes());
        input.extend_from_slice(&body);
    }

    let mut output = Vec::new();
    tsz_cli::tsserver::run_tsserver(Cursor::new(input), &mut output).unwrap();
    let responses = decode_messages(&output);
    assert_eq!(responses.len(), 5);

    for definition in [
        &responses[1]["body"][0],
        &responses[2]["body"]["definitions"][0],
    ] {
        assert_eq!(definition["kind"], "local var");
        assert_eq!(definition["name"], "local");
        assert_eq!(definition["containerKind"], "");
        assert_eq!(definition["containerName"], "");
        assert_eq!(definition["isLocal"], true);
        assert_eq!(definition["isAmbient"], false);
        assert_eq!(definition["unverified"], false);
        assert_eq!(definition["failedAliasResolution"], false);
    }
    assert_eq!(
        responses[4]["body"]["info"]["triggerSpan"],
        json!({
            "start": {"line": 1, "offset": 7},
            "end": {"line": 1, "offset": 8},
            "length": 1,
        })
    );
}

#[test]
fn every_native_binary_has_an_honest_help_surface() {
    for (binary, expected) in [
        (env!("CARGO_BIN_EXE_tsz"), "Usage: tsz"),
        (env!("CARGO_BIN_EXE_try-tsz"), "Usage: tsz"),
        (env!("CARGO_BIN_EXE_tsz-server"), "Usage: tsz-server"),
        (env!("CARGO_BIN_EXE_tsz-lsp"), "Usage: tsz-lsp"),
    ] {
        let output = Command::new(binary).arg("--help").output().unwrap();
        assert!(output.status.success(), "{binary} --help failed");
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(
            stdout.contains(expected),
            "{binary} --help did not contain {expected:?}: {stdout:?}"
        );
        assert!(output.stderr.is_empty(), "{binary} --help wrote to stderr");
    }
}

#[test]
fn cli_check_js_implies_allow_js_before_project_discovery() {
    let project = tempfile::tempdir().unwrap();
    std::fs::write(
        project.path().join("tsconfig.json"),
        r#"{"compilerOptions":{"noEmit":true}}"#,
    )
    .unwrap();
    std::fs::write(project.path().join("case.js"), "MissingFromJavaScript;\n").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_tsz"))
        .current_dir(project.path())
        .args(["--checkJs", "--pretty", "false"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("case.js(1,1): error TS2304: Cannot find name 'MissingFromJavaScript'."),
        "{stdout:?}",
    );
}

#[test]
fn flat_cli_renders_array_relation_continuations_under_one_primary_diagnostic() {
    let project = tempfile::tempdir().unwrap();
    let source_path = project.path().join("array-relation.ts");
    std::fs::write(
        &source_path,
        "const values=[\"other\"];\nconst target:\"seed\"[]=values;\n",
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_tsz"))
        .args(["--strict", "--noEmit", "--pretty", "false"])
        .arg(&source_path)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains(
            "error TS2322: Type 'string[]' is not assignable to type '\"seed\"[]'.\n  Type 'string' is not assignable to type '\"seed\"'.\n"
        ),
        "{stdout:?}"
    );
    assert_eq!(stdout.matches("error TS2322:").count(), 1, "{stdout:?}");
    assert!(output.stderr.is_empty());
}

#[test]
fn cli_source_syntax_and_semantics_aggregate_in_both_root_orders() {
    let project = tempfile::tempdir().unwrap();
    let syntax = project.path().join("a-syntax.ts");
    let semantic = project.path().join("z-semantic.ts");
    std::fs::write(&syntax, "const broken = ;").unwrap();
    std::fs::write(&semantic, "const missing: number;").unwrap();

    for roots in [[&syntax, &semantic], [&semantic, &syntax]] {
        let output = Command::new(env!("CARGO_BIN_EXE_tsz"))
            .args(["--ignoreConfig", "--noEmit", "--pretty", "false"])
            .args(roots)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(3));
        assert!(output.stderr.is_empty());
        let stdout = String::from_utf8(output.stdout).unwrap();
        let syntax_diagnostic = format!(
            "{}(1,16): error TS1109: Expression expected.\n",
            syntax.display()
        );
        let semantic_diagnostic = format!(
            "{}(1,7): error TS1155: 'const' declarations must be initialized.\n",
            semantic.display()
        );
        assert_eq!(stdout, format!("{syntax_diagnostic}{semantic_diagnostic}"));
    }

    let unchecked = Command::new(env!("CARGO_BIN_EXE_tsz"))
        .args([
            "--ignoreConfig",
            "--noCheck",
            "--noEmit",
            "--pretty",
            "false",
        ])
        .args([&syntax, &semantic])
        .output()
        .unwrap();
    assert_eq!(unchecked.status.code(), Some(1));
    assert!(unchecked.stderr.is_empty());
    assert_eq!(
        String::from_utf8(unchecked.stdout).unwrap(),
        format!(
            "{}(1,16): error TS1109: Expression expected.\n",
            syntax.display()
        )
    );
}

#[test]
fn cli_config_diagnostics_remain_additive_with_selected_semantics() {
    let project = tempfile::tempdir().unwrap();
    std::fs::write(
        project.path().join("tsconfig.json"),
        r#"{"extends":"./renamed-missing-base","compilerOptions":{"noEmit":true},"files":["case.ts"]}"#,
    )
    .unwrap();
    std::fs::write(project.path().join("case.ts"), "const missing: number;").unwrap();
    let diagnostics_path = project.path().join("diagnostics.json");

    let output = Command::new(env!("CARGO_BIN_EXE_tsz"))
        .current_dir(project.path())
        .args([
            "--project",
            ".",
            "--pretty",
            "false",
            "--diagnostics-json",
            diagnostics_path.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stderr.is_empty());
    let diagnostics: Value =
        serde_json::from_slice(&std::fs::read(diagnostics_path).unwrap()).unwrap();
    assert_eq!(
        diagnostics
            .as_array()
            .unwrap()
            .iter()
            .map(|diagnostic| diagnostic["code"].as_u64().unwrap())
            .collect::<Vec<_>>(),
        vec![6053, 1155],
    );
}

#[test]
fn cli_syntax_program_and_semantic_diagnostics_aggregate() {
    let project = tempfile::tempdir().unwrap();
    std::fs::write(
        project.path().join("tsconfig.json"),
        r#"{"compilerOptions":{"noEmit":true},"files":["renamed-missing.ts","case.ts"]}"#,
    )
    .unwrap();
    let source = project.path().join("case.ts");
    let diagnostics_path = project.path().join("diagnostics.json");
    let run = || {
        let output = Command::new(env!("CARGO_BIN_EXE_tsz"))
            .current_dir(project.path())
            .args(["--project", ".", "--pretty", "false", "--diagnostics-json"])
            .arg(&diagnostics_path)
            .output()
            .unwrap();
        let diagnostics: Value =
            serde_json::from_slice(&std::fs::read(&diagnostics_path).unwrap()).unwrap();
        let codes = diagnostics
            .as_array()
            .unwrap()
            .iter()
            .map(|diagnostic| diagnostic["code"].as_u64().unwrap())
            .collect::<Vec<_>>();
        (output, codes)
    };

    std::fs::write(&source, "const missing: number;").unwrap();
    let (semantic, semantic_codes) = run();
    assert_eq!(semantic.status.code(), Some(1));
    assert!(semantic.stderr.is_empty());
    assert_eq!(semantic_codes, [6053, 1155]);

    std::fs::write(&source, "const broken = ;").unwrap();
    let (syntactic, syntactic_codes) = run();
    assert_eq!(syntactic.status.code(), Some(3));
    assert!(syntactic.stderr.is_empty());
    assert_eq!(syntactic_codes, [6053, 1109]);

    let javascript = project.path().join("case.js");
    std::fs::write(
        project.path().join("tsconfig.json"),
        r#"{"files":["renamed-missing.ts","case.ts"]}"#,
    )
    .unwrap();
    std::fs::write(&source, "const kept = 1;").unwrap();
    let (emitted, emitted_codes) = run();
    assert_eq!(emitted.status.code(), Some(2));
    assert_eq!(emitted_codes, [6053]);
    assert!(javascript.exists());

    std::fs::remove_file(&javascript).unwrap();
    std::fs::write(
        project.path().join("tsconfig.json"),
        r#"{"compilerOptions":{"noEmitOnError":true},"files":["renamed-missing.ts","case.ts"]}"#,
    )
    .unwrap();
    let (blocked, blocked_codes) = run();
    assert_eq!(blocked.status.code(), Some(1));
    assert_eq!(blocked_codes, [6053]);
    assert!(!javascript.exists());
}

#[test]
fn cli_missing_project_references_are_program_diagnostics() {
    let project = tempfile::tempdir().unwrap();
    std::fs::write(
        project.path().join("tsconfig.json"),
        r#"{"compilerOptions":{"noEmit":true},"files":["case.ts"],"references":[{"path":"./renamed-missing-reference"}]}"#,
    )
    .unwrap();
    std::fs::write(project.path().join("case.ts"), "const missing: number;").unwrap();
    let diagnostics_path = project.path().join("diagnostics.json");

    let output = Command::new(env!("CARGO_BIN_EXE_tsz"))
        .current_dir(project.path())
        .args(["--project", ".", "--pretty", "false", "--diagnostics-json"])
        .arg(&diagnostics_path)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stderr.is_empty());
    let diagnostics: Value =
        serde_json::from_slice(&std::fs::read(diagnostics_path).unwrap()).unwrap();
    assert_eq!(
        diagnostics
            .as_array()
            .unwrap()
            .iter()
            .map(|diagnostic| diagnostic["code"].as_u64().unwrap())
            .collect::<Vec<_>>(),
        [1155, 6053],
    );
}

#[test]
fn cli_config_target_diagnostics_remain_independent_of_source_products() {
    let project = tempfile::tempdir().unwrap();
    let config = project.path().join("tsconfig.json");
    std::fs::write(
        &config,
        r#"{"compilerOptions":{"target":"renamed-invalid","noEmit":true},"files":["case.ts"]}"#,
    )
    .unwrap();
    let source = project.path().join("case.ts");
    let diagnostics_path = project.path().join("diagnostics.json");
    let run = || {
        let output = Command::new(env!("CARGO_BIN_EXE_tsz"))
            .current_dir(project.path())
            .args(["--project", ".", "--pretty", "false", "--diagnostics-json"])
            .arg(&diagnostics_path)
            .output()
            .unwrap();
        let diagnostics: Value =
            serde_json::from_slice(&std::fs::read(&diagnostics_path).unwrap()).unwrap();
        let codes = diagnostics
            .as_array()
            .unwrap()
            .iter()
            .map(|diagnostic| diagnostic["code"].as_u64().unwrap())
            .collect::<Vec<_>>();
        (output, codes)
    };

    std::fs::write(&source, "const missing: number;").unwrap();
    let (semantic, semantic_codes) = run();
    assert_eq!(semantic.status.code(), Some(1));
    assert!(semantic.stderr.is_empty());
    assert_eq!(semantic_codes, [1155, 6046]);

    std::fs::write(&source, "const broken = ;").unwrap();
    let (syntactic, syntactic_codes) = run();
    assert_eq!(syntactic.status.code(), Some(3));
    assert!(syntactic.stderr.is_empty());
    assert_eq!(syntactic_codes, [1109, 6046]);

    std::fs::write(
        &config,
        r#"{"compilerOptions":{"target":"es5","noEmit":true},"files":["case.ts"]}"#,
    )
    .unwrap();
    std::fs::write(&source, "const missing: number;").unwrap();
    let (removed, removed_codes) = run();
    assert_eq!(removed.status.code(), Some(1));
    assert!(removed.stderr.is_empty());
    assert_eq!(removed_codes, [5108]);

    std::fs::write(&source, "const broken = ;").unwrap();
    let (removed_with_syntax, removed_with_syntax_codes) = run();
    assert_eq!(removed_with_syntax.status.code(), Some(1));
    assert!(removed_with_syntax.stderr.is_empty());
    assert_eq!(removed_with_syntax_codes, [5108]);
}

#[test]
fn config_target_parse_diagnostics_survive_effective_option_overrides() {
    let project = tempfile::tempdir().unwrap();
    let config = project.path().join("tsconfig.json");
    let source = project.path().join("case.ts");
    let diagnostics_path = project.path().join("diagnostics.json");
    let run = |arguments: &[&str]| {
        let output = Command::new(env!("CARGO_BIN_EXE_tsz"))
            .current_dir(project.path())
            .args(arguments)
            .args(["--pretty", "false", "--diagnostics-json"])
            .arg(&diagnostics_path)
            .output()
            .unwrap();
        let diagnostics: Value =
            serde_json::from_slice(&std::fs::read(&diagnostics_path).unwrap()).unwrap();
        let codes = diagnostics
            .as_array()
            .unwrap()
            .iter()
            .map(|diagnostic| diagnostic["code"].as_u64().unwrap())
            .collect::<Vec<_>>();
        (output, codes)
    };

    std::fs::write(
        &config,
        r#"{"compilerOptions":{"target":"renamed-invalid","noEmit":true},"files":["case.ts"]}"#,
    )
    .unwrap();
    std::fs::write(&source, "const missing: number;").unwrap();
    let (overridden, overridden_codes) = run(&["--project", ".", "--target", "es2022"]);
    assert_eq!(overridden.status.code(), Some(1));
    assert!(overridden.stderr.is_empty());
    assert_eq!(overridden_codes, [1155, 6046]);

    std::fs::write(
        project.path().join("base.json"),
        r#"{"compilerOptions":{"target":"renamed-invalid"}}"#,
    )
    .unwrap();
    std::fs::write(
        &config,
        r#"{"extends":"./base.json","compilerOptions":{"target":"es2022","noEmit":true},"files":["case.ts"]}"#,
    )
    .unwrap();
    let (inherited, inherited_codes) = run(&["--project", "."]);
    assert_eq!(inherited.status.code(), Some(1));
    assert!(inherited.stderr.is_empty());
    assert_eq!(inherited_codes, [6046, 1155]);

    std::fs::write(&source, "const broken = ;").unwrap();
    let (inherited_syntax, inherited_syntax_codes) = run(&["--project", "."]);
    assert_eq!(inherited_syntax.status.code(), Some(3));
    assert!(inherited_syntax.stderr.is_empty());
    assert_eq!(inherited_syntax_codes, [6046, 1109]);
}

#[test]
fn cli_removed_es5_target_is_fatal_and_suppresses_all_products() {
    let project = tempfile::tempdir().unwrap();
    let source = project.path().join("case.ts");
    let javascript = project.path().join("case.js");
    let diagnostics_path = project.path().join("diagnostics.json");
    let run = |target: &str| {
        let output = Command::new(env!("CARGO_BIN_EXE_tsz"))
            .current_dir(project.path())
            .args([
                "--ignoreConfig",
                "--target",
                target,
                "--pretty",
                "false",
                "--diagnostics-json",
            ])
            .arg(&diagnostics_path)
            .arg(&source)
            .output()
            .unwrap();
        let diagnostics: Value =
            serde_json::from_slice(&std::fs::read(&diagnostics_path).unwrap()).unwrap();
        let codes = diagnostics
            .as_array()
            .unwrap()
            .iter()
            .map(|diagnostic| diagnostic["code"].as_u64().unwrap())
            .collect::<Vec<_>>();
        (output, codes)
    };

    std::fs::write(&source, "const missing: number;").unwrap();
    let (semantic, semantic_codes) = run("es5");
    assert_eq!(semantic.status.code(), Some(1));
    assert!(semantic.stderr.is_empty());
    assert_eq!(semantic_codes, [5108]);
    assert!(!javascript.exists());

    std::fs::write(&source, "const broken = ;").unwrap();
    let (syntactic, syntactic_codes) = run("es5");
    assert_eq!(syntactic.status.code(), Some(1));
    assert!(syntactic.stderr.is_empty());
    assert_eq!(syntactic_codes, [5108]);
    assert!(!javascript.exists());

    let es3 = Command::new(env!("CARGO_BIN_EXE_tsz"))
        .current_dir(project.path())
        .args(["--ignoreConfig", "--target", "es3", "--pretty", "false"])
        .arg(&source)
        .output()
        .unwrap();
    assert_eq!(es3.status.code(), Some(1));
    assert!(es3.stderr.is_empty());
    assert_eq!(
        String::from_utf8(es3.stdout).unwrap(),
        concat!(
            "error TS6046: Argument for '--target' option must be: 'es6', 'es2015', ",
            "'es2016', 'es2017', 'es2018', 'es2019', 'es2020', 'es2021', 'es2022', ",
            "'es2023', 'es2024', 'es2025', 'esnext'.\n"
        )
    );
    assert!(!javascript.exists());
}

#[test]
fn cli_invalid_targets_precede_project_discovery_and_other_option_products() {
    const MESSAGE: &str = concat!(
        "error TS6046: Argument for '--target' option must be: 'es6', 'es2015', 'es2016', ",
        "'es2017', 'es2018', 'es2019', 'es2020', 'es2021', 'es2022', ",
        "'es2023', 'es2024', 'es2025', 'esnext'.\n"
    );
    let project = tempfile::tempdir().unwrap();
    let run = |arguments: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_tsz"))
            .current_dir(project.path())
            .args(arguments)
            .output()
            .unwrap()
    };

    let empty = run(&["--target", "renamed-invalid", "--pretty", "false"]);
    assert_eq!(empty.status.code(), Some(1));
    assert!(empty.stderr.is_empty());
    assert_eq!(String::from_utf8(empty.stdout).unwrap(), MESSAGE);

    std::fs::write(
        project.path().join("tsconfig.json"),
        r#"{"extends":"./renamed-missing-base","files":["case.ts"]}"#,
    )
    .unwrap();
    std::fs::write(project.path().join("case.ts"), "const missing: number;").unwrap();
    let config = run(&[
        "--project",
        ".",
        "--target",
        "renamed-invalid",
        "--strictPropertyInitialization",
        "true",
        "--strictNullChecks",
        "false",
        "--pretty",
        "false",
    ]);
    assert_eq!(config.status.code(), Some(1));
    assert!(config.stderr.is_empty());
    assert_eq!(String::from_utf8(config.stdout).unwrap(), MESSAGE);

    std::fs::write(
        project.path().join("tsconfig.json"),
        r#"{"files":["renamed-missing.ts"]}"#,
    )
    .unwrap();
    let root = run(&[
        "--project",
        ".",
        "--target",
        "renamed-invalid",
        "--pretty",
        "false",
    ]);
    assert_eq!(root.status.code(), Some(1));
    assert!(root.stderr.is_empty());
    assert_eq!(String::from_utf8(root.stdout).unwrap(), MESSAGE);

    let missing_project = run(&[
        "--project",
        "renamed-missing.json",
        "--target",
        "es5",
        "--pretty",
        "false",
    ]);
    assert_eq!(missing_project.status.code(), Some(1));
    assert!(missing_project.stderr.is_empty());
    assert_eq!(
        String::from_utf8(missing_project.stdout).unwrap(),
        "error TS5108: Option 'target=ES5' has been removed. Please remove it from your configuration.\n"
    );

    let invalid_missing_project = run(&[
        "--project",
        "renamed-missing.json",
        "--target",
        "renamed-invalid",
        "--pretty",
        "false",
    ]);
    assert_eq!(invalid_missing_project.status.code(), Some(1));
    assert!(invalid_missing_project.stderr.is_empty());
    assert_eq!(
        String::from_utf8(invalid_missing_project.stdout).unwrap(),
        MESSAGE
    );
}

#[test]
fn command_line_parse_diagnostics_precede_help_and_preserve_authored_order() {
    const TARGET: &str = concat!(
        "error TS6046: Argument for '--target' option must be: 'es6', 'es2015', 'es2016', ",
        "'es2017', 'es2018', 'es2019', 'es2020', 'es2021', 'es2022', ",
        "'es2023', 'es2024', 'es2025', 'esnext'.\n"
    );
    let run = |arguments: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_tsz"))
            .args(arguments)
            .output()
            .unwrap()
    };

    for mode in ["--help", "--version"] {
        let output = run(&[
            "--target",
            "renamed-invalid",
            "--target",
            "es2022",
            mode,
            "--pretty",
            "false",
        ]);
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stderr.is_empty());
        assert_eq!(String::from_utf8(output.stdout).unwrap(), TARGET);
    }

    let repeated = run(&[
        "--target",
        "first-invalid",
        "--target",
        "second-invalid",
        "--pretty",
        "false",
    ]);
    assert_eq!(repeated.status.code(), Some(1));
    assert!(repeated.stderr.is_empty());
    assert_eq!(
        String::from_utf8(repeated.stdout).unwrap(),
        TARGET.repeat(2)
    );

    let ordered = run(&[
        "--renamed-unknown",
        "--target",
        "renamed-invalid",
        "--help",
        "--pretty",
        "false",
    ]);
    assert_eq!(ordered.status.code(), Some(1));
    assert!(ordered.stderr.is_empty());
    assert_eq!(
        String::from_utf8(ordered.stdout).unwrap(),
        format!("error TS5023: Unknown compiler option '--renamed-unknown'.\n{TARGET}")
    );

    for argument in [
        "--help=true",
        "--version=false",
        "--preserveValueImports=true",
        "--importsNotUsedAsValues=remove",
    ] {
        let output = run(&[argument]);
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stderr.is_empty());
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            format!("error TS5023: Unknown compiler option '{argument}'.\n")
        );
    }

    for (arguments, code, authored) in [
        (&["--alwaysStrict=false"][..], 5108, "alwaysStrict=false"),
        (
            &["--downlevelIteration=false"][..],
            5102,
            "downlevelIteration",
        ),
        (
            &["--esModuleInterop=false"][..],
            5108,
            "esModuleInterop=false",
        ),
        (
            &["--moduleResolution", "node10"][..],
            5108,
            "moduleResolution=node10",
        ),
        (&["--baseUrl", "."][..], 5102, "baseUrl"),
    ] {
        let output = run(arguments);
        assert_eq!(output.status.code(), Some(1));
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            format!(
                "error TS{code}: Option '{authored}' has been removed. Please remove it from your configuration.\n"
            )
        );
    }
}

#[test]
fn cli_source_program_global_and_emit_path_diagnostics_aggregate() {
    let project = tempfile::tempdir().unwrap();
    let source = project.path().join("outside.ts");
    let source_root = project.path().join("src");
    std::fs::create_dir(&source_root).unwrap();
    let diagnostics_path = project.path().join("diagnostics.json");
    let run = |no_lib: bool| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_tsz"));
        command.args([
            "--ignoreConfig",
            "--noEmit",
            "--pretty",
            "false",
            "--strictPropertyInitialization=true",
            "--strictNullChecks=false",
            "--rootDir",
        ]);
        command
            .arg(&source_root)
            .args(["--diagnostics-json"])
            .arg(&diagnostics_path);
        if no_lib {
            command.arg("--noLib");
        }
        let output = command.arg(&source).output().unwrap();
        let diagnostics: Value =
            serde_json::from_slice(&std::fs::read(&diagnostics_path).unwrap()).unwrap();
        let codes = diagnostics
            .as_array()
            .unwrap()
            .iter()
            .map(|diagnostic| diagnostic["code"].as_u64().unwrap())
            .collect::<Vec<_>>();
        (output, codes)
    };

    std::fs::write(&source, "const broken = ;").unwrap();
    let (syntactic, syntactic_codes) = run(false);
    assert_eq!(syntactic.status.code(), Some(3));
    assert!(syntactic.stderr.is_empty());
    assert_eq!(syntactic_codes, [5052, 6059, 1109]);

    let (syntactic_global, syntactic_global_codes) = run(true);
    assert_eq!(syntactic_global.status.code(), Some(3));
    assert!(syntactic_global.stderr.is_empty());
    assert_eq!(syntactic_global_codes[..10], [2318; 10]);
    assert_eq!(syntactic_global_codes[10..], [5052, 6059, 1109]);

    std::fs::write(&source, "const missing: number;").unwrap();
    let (program, program_codes) = run(false);
    assert_eq!(program.status.code(), Some(1));
    assert!(program.stderr.is_empty());
    assert_eq!(program_codes, [5052, 6059, 1155]);

    let (global, global_codes) = run(true);
    assert_eq!(global.status.code(), Some(1));
    assert!(global.stderr.is_empty());
    assert_eq!(global_codes[..10], [2318; 10]);
    assert_eq!(global_codes[10..], [5052, 6059]);
}

#[test]
fn program_option_dependencies_aggregate_with_checker_diagnostics() {
    let project = tempfile::tempdir().unwrap();
    let source = project.path().join("case.ts");
    let diagnostics_path = project.path().join("diagnostics.json");
    std::fs::write(
        project.path().join("tsconfig.json"),
        r#"{"compilerOptions":{"strictPropertyInitialization":true,"strictNullChecks":false,"noEmit":true},"files":["case.ts"]}"#,
    )
    .unwrap();
    let run = || {
        let output = Command::new(env!("CARGO_BIN_EXE_tsz"))
            .current_dir(project.path())
            .args(["--project", ".", "--pretty", "false", "--diagnostics-json"])
            .arg(&diagnostics_path)
            .output()
            .unwrap();
        let diagnostics: Value =
            serde_json::from_slice(&std::fs::read(&diagnostics_path).unwrap()).unwrap();
        let codes = diagnostics
            .as_array()
            .unwrap()
            .iter()
            .map(|diagnostic| diagnostic["code"].as_u64().unwrap())
            .collect::<Vec<_>>();
        (output, codes)
    };

    std::fs::write(&source, "const missing: number;").unwrap();
    let (semantic, semantic_codes) = run();
    assert_eq!(semantic.status.code(), Some(1));
    assert!(semantic.stderr.is_empty());
    assert_eq!(semantic_codes, [1155, 5052]);

    std::fs::write(&source, "const broken = ;").unwrap();
    let (syntactic, syntactic_codes) = run();
    assert_eq!(syntactic.status.code(), Some(3));
    assert!(syntactic.stderr.is_empty());
    assert_eq!(syntactic_codes, [1109, 5052]);
}

#[test]
fn cli_diagnostic_phases_preserve_independent_completion_truth() {
    let project = tempfile::tempdir().unwrap();
    let syntax = project.path().join("syntax.ts");
    let deferred = project.path().join("deferred.ts");
    let diagnostics_path = project.path().join("diagnostics.json");
    let counters_path = project.path().join("counters.json");
    std::fs::write(&syntax, "const broken = ;").unwrap();
    std::fs::write(
        &deferred,
        "type Keys=keyof number; let key:Keys; const value:number=key;",
    )
    .unwrap();
    let fingerprint = || {
        let diagnostics: Value =
            serde_json::from_slice(&std::fs::read(&diagnostics_path).unwrap()).unwrap();
        let counters: Value =
            serde_json::from_slice(&std::fs::read(&counters_path).unwrap()).unwrap();
        (
            diagnostics
                .as_array()
                .unwrap()
                .iter()
                .map(|diagnostic| diagnostic["code"].as_u64().unwrap())
                .collect::<Vec<_>>(),
            counters["stats"]["semantic_completion"]
                .as_str()
                .unwrap()
                .to_string(),
        )
    };

    let syntax_output = Command::new(env!("CARGO_BIN_EXE_tsz"))
        .args([
            "--ignoreConfig",
            "--noEmit",
            "--pretty",
            "false",
            "--diagnostics-json",
        ])
        .arg(&diagnostics_path)
        .args(["--perf-counters-json"])
        .arg(&counters_path)
        .args([&syntax, &deferred])
        .output()
        .unwrap();
    assert_eq!(syntax_output.status.code(), Some(3));
    assert!(syntax_output.stderr.is_empty());
    assert_eq!(fingerprint(), (vec![1109], "deferred".to_string()));

    std::fs::write(
        project.path().join("tsconfig.json"),
        r#"{"compilerOptions":{"noEmit":true},"files":["renamed-missing.ts","deferred.ts"]}"#,
    )
    .unwrap();
    let program_output = Command::new(env!("CARGO_BIN_EXE_tsz"))
        .current_dir(project.path())
        .args(["--project", ".", "--pretty", "false", "--diagnostics-json"])
        .arg(&diagnostics_path)
        .args(["--perf-counters-json"])
        .arg(&counters_path)
        .output()
        .unwrap();
    assert_eq!(program_output.status.code(), Some(3));
    assert!(program_output.stderr.is_empty());
    assert_eq!(fingerprint(), (vec![6053], "deferred".to_string()));

    std::fs::write(
        project.path().join("tsconfig.json"),
        r#"{"extends":"./renamed-missing-base","compilerOptions":{"noEmit":true},"files":["deferred.ts"]}"#,
    )
    .unwrap();
    let config_output = Command::new(env!("CARGO_BIN_EXE_tsz"))
        .current_dir(project.path())
        .args(["--project", ".", "--pretty", "false", "--diagnostics-json"])
        .arg(&diagnostics_path)
        .args(["--perf-counters-json"])
        .arg(&counters_path)
        .output()
        .unwrap();
    assert_eq!(config_output.status.code(), Some(3));
    assert!(config_output.stderr.is_empty());
    assert_eq!(fingerprint(), (vec![6053], "deferred".to_string()));
}

#[test]
fn batch_process_emits_one_exact_sentinel_per_project() {
    let project = tempfile::tempdir().unwrap();
    std::fs::write(
        project.path().join("tsconfig.json"),
        r#"{"compilerOptions":{"strict":true,"noEmit":true},"files":["case.ts"]}"#,
    )
    .unwrap();
    std::fs::write(
        project.path().join("case.ts"),
        r#"const count: number = "wrong";"#,
    )
    .unwrap();

    let mut child = Command::new(env!("CARGO_BIN_EXE_tsz"))
        .args(["--batch", "--noEmit", "--pretty", "false"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    writeln!(child.stdin.take().unwrap(), "{}", project.path().display()).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("error TS2322:"));
    assert_eq!(
        stdout
            .matches("---TSZ-SEMANTIC-COMPLETION:complete---\n")
            .count(),
        1
    );
    assert_eq!(stdout.matches("---TSZ-BATCH-DONE---\n").count(), 1);
}

#[test]
fn semantic_nonclaims_use_exit_three_and_an_exact_batch_marker() {
    let project = tempfile::tempdir().unwrap();
    std::fs::write(
        project.path().join("tsconfig.json"),
        r#"{"compilerOptions":{"strict":true,"noEmit":true},"files":["case.ts"]}"#,
    )
    .unwrap();
    std::fs::write(
        project.path().join("case.ts"),
        "type Keys=keyof number; let key:Keys; const value:number=key;",
    )
    .unwrap();
    let stats_path = project.path().join("stats.json");

    let fresh = Command::new(env!("CARGO_BIN_EXE_tsz"))
        .args([
            "--project",
            project.path().to_str().unwrap(),
            "--noEmit",
            "--pretty",
            "false",
            "--perf-counters-json",
            stats_path.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(fresh.status.code(), Some(3));
    assert!(
        fresh.stdout.is_empty(),
        "no protocol marker belongs on fresh CLI stdout"
    );
    assert!(fresh.stderr.is_empty());
    let stats: Value = serde_json::from_slice(&std::fs::read(&stats_path).unwrap()).unwrap();
    assert_eq!(stats["schema_version"], 2);
    assert_eq!(stats["stats"]["semantic_completion"], "deferred");

    let mut child = Command::new(env!("CARGO_BIN_EXE_tsz"))
        .args(["--batch", "--noEmit", "--pretty", "false"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    writeln!(child.stdin.take().unwrap(), "{}", project.path().display()).unwrap();
    let batch = child.wait_with_output().unwrap();
    assert!(batch.status.success());
    assert!(batch.stderr.is_empty());
    assert_eq!(
        String::from_utf8(batch.stdout).unwrap(),
        "---TSZ-SEMANTIC-COMPLETION:deferred---\n---TSZ-BATCH-DONE---\n"
    );

    let unchecked_stats = project.path().join("unchecked-stats.json");
    let unchecked = Command::new(env!("CARGO_BIN_EXE_tsz"))
        .args([
            "--project",
            project.path().to_str().unwrap(),
            "--noCheck",
            "--noEmit",
            "--pretty",
            "false",
            "--perf-counters-json",
            unchecked_stats.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(unchecked.status.success());
    let stats: Value = serde_json::from_slice(&std::fs::read(&unchecked_stats).unwrap()).unwrap();
    assert_eq!(stats["stats"]["semantic_completion"], "complete");
}

#[test]
fn lsp_initializes_and_rejects_unsupported_methods_honestly() {
    let requests = [
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}}),
        json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {
                "textDocument": {
                    "uri": "file:///input.ts",
                    "languageId": "typescript",
                    "version": 1,
                    "text": "const value = 1;"
                }
            }
        }),
        json!({"jsonrpc": "2.0", "id": 2, "method": "textDocument/hover", "params": {}}),
        json!({"jsonrpc": "2.0", "id": 3, "method": "shutdown", "params": null}),
        json!({"jsonrpc": "2.0", "method": "exit", "params": null}),
    ];
    let mut child = Command::new(env!("CARGO_BIN_EXE_tsz-lsp"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    for request in requests {
        let body = serde_json::to_vec(&request).unwrap();
        write!(input, "Content-Length: {}\r\n\r\n", body.len()).unwrap();
        input.write_all(&body).unwrap();
    }
    drop(input);
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let responses = decode_messages(&output.stdout);
    assert_eq!(responses.len(), 3);
    assert_eq!(responses[0]["result"]["serverInfo"]["name"], "tsz-lsp");
    assert_eq!(
        responses[0]["result"]["capabilities"]["textDocumentSync"],
        0
    );
    assert_eq!(responses[1]["error"]["code"], -32601);
    assert_eq!(responses[2]["result"], Value::Null);
}

fn decode_messages(bytes: &[u8]) -> Vec<Value> {
    let mut cursor = Cursor::new(bytes);
    let mut messages = Vec::new();
    while cursor.position() < bytes.len() as u64 {
        let mut header = Vec::new();
        loop {
            let mut byte = [0_u8; 1];
            cursor.read_exact(&mut byte).unwrap();
            header.push(byte[0]);
            if header.ends_with(b"\r\n\r\n") {
                break;
            }
        }
        let header = String::from_utf8(header).unwrap();
        let length = header
            .lines()
            .find_map(|line| line.strip_prefix("Content-Length:"))
            .unwrap()
            .trim()
            .parse::<usize>()
            .unwrap();
        let mut body = vec![0; length];
        cursor.read_exact(&mut body).unwrap();
        messages.push(serde_json::from_slice(&body).unwrap());
    }
    messages
}
