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
            "command": "open",
            "arguments": {"file": "case.ts", "fileContent": "const x: number = 'bad';"}
        }),
        json!({
            "seq": 2,
            "type": "request",
            "command": "semanticDiagnosticsSync",
            "arguments": {"file": "case.ts"}
        }),
        json!({
            "seq": 3,
            "type": "request",
            "command": "quickinfo",
            "arguments": {"file": "case.ts", "line": 1, "offset": 7}
        }),
        json!({
            "seq": 4,
            "type": "request",
            "command": "notImplemented",
            "arguments": {}
        }),
        json!({
            "seq": 5,
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
    assert_eq!(responses.len(), 5);
    assert_eq!(responses[1]["success"], true);
    assert_eq!(responses[1]["body"][0]["code"], 2322);
    assert_eq!(responses[2]["success"], true);
    assert_eq!(responses[2]["body"]["displayString"], "const x: number");
    assert_eq!(responses[3]["success"], false);
    assert_eq!(responses[4]["command"], "tsz/reset");
    assert_eq!(responses[4]["request_seq"], 5);
    assert_eq!(responses[4]["success"], true);
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
