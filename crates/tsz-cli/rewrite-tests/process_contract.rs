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
    assert!(invocation.options.no_emit);
    assert!(invocation.options.strict);
    assert!(!invocation.pretty);
    assert!(invocation.extended_diagnostics);
    assert!(invocation.unknown_options.is_empty());
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
    assert_eq!(stdout.matches("---TSZ-BATCH-DONE---\n").count(), 1);
}

#[test]
fn lsp_initializes_and_rejects_unsupported_methods_honestly() {
    let requests = [
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}}),
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
