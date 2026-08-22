use std::ffi::OsString;
use std::io::Cursor;
use std::process::Command;

use serde_json::{Value, json};

#[test]
fn command_line_preserves_explicit_lib_and_false_boolean_patches() {
    let arguments = [
        "--lib",
        "ES5, DOM",
        "--noLib",
        "false",
        "--allowJs",
        "false",
        "case.ts",
    ]
    .into_iter()
    .map(OsString::from)
    .collect::<Vec<_>>();
    let invocation = tsz_cli::driver::parse_arguments(&arguments).unwrap();
    assert_eq!(
        invocation.options.lib,
        Some(vec!["ES5".to_string(), "DOM".to_string()])
    );
    assert_eq!(invocation.options.no_lib, Some(false));
    assert_eq!(invocation.options.allow_js, Some(false));
    assert!(invocation.unknown_options.is_empty());
}

#[test]
fn project_config_lib_array_replaces_defaults() {
    let project = tempfile::tempdir().unwrap();
    std::fs::write(
        project.path().join("tsconfig.json"),
        r#"{"compilerOptions":{"noEmit":true,"lib":["es2015.promise"]},"files":["case.ts"]}"#,
    )
    .unwrap();
    std::fs::write(project.path().join("case.ts"), "Promise;").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_tsz"))
        .args([
            "--project",
            project.path().to_str().unwrap(),
            "--pretty",
            "false",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        missing_essential_globals_stdout()
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn project_config_no_lib_suppresses_explicit_and_default_libraries() {
    let project = tempfile::tempdir().unwrap();
    std::fs::write(
        project.path().join("tsconfig.json"),
        r#"{"compilerOptions":{"noEmit":true,"noLib":true,"lib":["es5"]},"files":["case.ts"]}"#,
    )
    .unwrap();
    std::fs::write(project.path().join("case.ts"), "parseInt;").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_tsz"))
        .args([
            "--project",
            project.path().to_str().unwrap(),
            "--pretty",
            "false",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        missing_essential_globals_stdout()
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn tsserver_preserves_target_and_explicit_lib_options() {
    let requests = [
        json!({
            "seq": 1,
            "type": "request",
            "command": "compilerOptionsForInferredProjects",
            "arguments": {"options": {"target": "es2021"}}
        }),
        json!({
            "seq": 2,
            "type": "request",
            "command": "open",
            "arguments": {"file": "target.ts", "fileContent": "WeakRef;"}
        }),
        json!({
            "seq": 3,
            "type": "request",
            "command": "semanticDiagnosticsSync",
            "arguments": {"file": "target.ts"}
        }),
        json!({
            "seq": 4,
            "type": "request",
            "command": "compilerOptionsForInferredProjects",
            "arguments": {"options": {"lib": ["es2015.promise"]}}
        }),
        json!({
            "seq": 5,
            "type": "request",
            "command": "open",
            "arguments": {"file": "explicit.ts", "fileContent": "Promise;"}
        }),
        json!({
            "seq": 6,
            "type": "request",
            "command": "semanticDiagnosticsSync",
            "arguments": {"file": "explicit.ts"}
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
    assert_eq!(responses[2]["body"], json!([]));
    assert_eq!(responses[5]["body"], json!([]));
}

#[test]
fn tsserver_preserves_explicit_no_implicit_any_false_under_strict() {
    let requests = [
        json!({
            "seq": 1,
            "type": "request",
            "command": "compilerOptionsForInferredProjects",
            "arguments": {"options": {"strict": true, "noImplicitAny": false}}
        }),
        json!({
            "seq": 2,
            "type": "request",
            "command": "open",
            "arguments": {
                "file": "opted-out.ts",
                "fileContent": "function optedOutIdentity(value) { return value; }"
            }
        }),
        json!({
            "seq": 3,
            "type": "request",
            "command": "semanticDiagnosticsSync",
            "arguments": {"file": "opted-out.ts"}
        }),
        json!({
            "seq": 4,
            "type": "request",
            "command": "compilerOptionsForInferredProjects",
            "arguments": {"options": {"strict": true}}
        }),
        json!({
            "seq": 5,
            "type": "request",
            "command": "open",
            "arguments": {
                "file": "strict-default.ts",
                "fileContent": "function strictIdentity(value) { return value; }"
            }
        }),
        json!({
            "seq": 6,
            "type": "request",
            "command": "semanticDiagnosticsSync",
            "arguments": {"file": "strict-default.ts"}
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
    assert_eq!(responses[2]["body"], json!([]));
    assert_eq!(responses[5]["body"][0]["code"], 7006);
}

fn decode_messages(bytes: &[u8]) -> Vec<Value> {
    let mut messages = Vec::new();
    let mut cursor = 0;
    while cursor < bytes.len() {
        let separator = bytes[cursor..]
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .unwrap();
        let header_end = cursor + separator;
        let header = std::str::from_utf8(&bytes[cursor..header_end]).unwrap();
        let length = header
            .strip_prefix("Content-Length: ")
            .unwrap()
            .parse::<usize>()
            .unwrap();
        let body_start = header_end + 4;
        let body_end = body_start + length;
        messages.push(serde_json::from_slice(&bytes[body_start..body_end]).unwrap());
        cursor = body_end;
    }
    messages
}

fn missing_essential_globals_stdout() -> String {
    [
        "Array",
        "Boolean",
        "CallableFunction",
        "Function",
        "IArguments",
        "NewableFunction",
        "Number",
        "Object",
        "RegExp",
        "String",
    ]
    .into_iter()
    .map(|name| format!("error TS2318: Cannot find global type '{name}'.\n"))
    .collect()
}
