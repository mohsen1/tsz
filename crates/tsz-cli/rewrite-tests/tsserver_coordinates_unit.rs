use std::io::{Cursor, Read};

use serde_json::{Value, json};

use super::run_tsserver;

#[test]
fn open_without_content_reads_disk_and_never_registers_failed_reads() {
    let project = tempfile::tempdir().unwrap();
    let disk_path = project.path().join("disk 😀.ts");
    let disk_text = "const diskValue = \"😀\";\r\ndiskValue;";
    std::fs::write(&disk_path, disk_text).unwrap();
    let disk_path = disk_path.to_string_lossy().into_owned();

    let utf16_path = project.path().join("utf16.ts");
    let utf16_text = "const café = 1; café;";
    let mut utf16_bytes = vec![0xff, 0xfe];
    utf16_bytes.extend(utf16_text.encode_utf16().flat_map(u16::to_le_bytes));
    std::fs::write(&utf16_path, utf16_bytes).unwrap();
    let utf16_path = utf16_path.to_string_lossy().into_owned();

    let missing_path = project.path().join("missing.ts");
    let missing_path = missing_path.to_string_lossy().into_owned();
    let unreadable_path = project.path().join("directory.ts");
    std::fs::create_dir(&unreadable_path).unwrap();
    let unreadable_path = unreadable_path.to_string_lossy().into_owned();

    let responses = run([
        json!({
            "seq": 1, "type": "request", "command": "open",
            "arguments": {"file": disk_path},
        }),
        json!({
            "seq": 2, "type": "request", "command": "tsz/text",
            "arguments": {"file": disk_path},
        }),
        json!({
            "seq": 3, "type": "request", "command": "open",
            "arguments": {"file": disk_path, "fileContent": ""},
        }),
        json!({
            "seq": 4, "type": "request", "command": "tsz/text",
            "arguments": {"file": disk_path},
        }),
        json!({
            "seq": 5, "type": "request", "command": "open",
            "arguments": {"file": utf16_path},
        }),
        json!({
            "seq": 6, "type": "request", "command": "tsz/text",
            "arguments": {"file": utf16_path},
        }),
        json!({
            "seq": 7, "type": "request", "command": "open",
            "arguments": {"file": missing_path},
        }),
        json!({
            "seq": 8, "type": "request", "command": "tsz/text",
            "arguments": {"file": missing_path},
        }),
        json!({
            "seq": 9, "type": "request", "command": "open",
            "arguments": {"file": unreadable_path},
        }),
        json!({
            "seq": 10, "type": "request", "command": "tsz/text",
            "arguments": {"file": unreadable_path},
        }),
    ]);

    assert_eq!(responses.len(), 10);
    assert_eq!(responses[0]["success"], true);
    assert_eq!(responses[1]["body"], disk_text);
    assert_eq!(responses[2]["success"], true);
    assert_eq!(responses[3]["body"], "");
    assert_eq!(responses[4]["success"], true);
    assert_eq!(responses[5]["body"], utf16_text);
    for response in [&responses[6], &responses[8]] {
        assert_eq!(response["success"], false, "{response:#}");
        assert!(
            response["message"]
                .as_str()
                .is_some_and(|message| message.starts_with("Cannot open file '")),
            "{response:#}"
        );
    }
    for response in [&responses[7], &responses[9]] {
        assert_eq!(response["success"], false, "{response:#}");
        assert!(
            response["message"]
                .as_str()
                .is_some_and(|message| message.starts_with("File is not open: ")),
            "{response:#}"
        );
    }
}

#[test]
fn mixed_typescript_line_terminators_share_quickinfo_navigation_and_rename_coordinates() {
    let source = concat!(
        "const alpha = 1;\r",
        "const beta = alpha;\n",
        "const gamma = beta;\r\n",
        "const delta = gamma;\u{2028}",
        "const epsilon = delta;\u{2029}",
        "const face = \"😀\"; const target: number = 1; target;",
    );
    let responses = run([
        json!({
            "seq": 1, "type": "request", "command": "open",
            "arguments": {"file": "mixed.ts", "fileContent": source},
        }),
        json!({
            "seq": 2, "type": "request", "command": "quickinfo",
            "arguments": {"file": "mixed.ts", "line": 6, "offset": 26},
        }),
        json!({
            "seq": 3, "type": "request", "command": "definitionAndBoundSpan",
            "arguments": {"file": "mixed.ts", "line": 6, "offset": 26},
        }),
        json!({
            "seq": 4, "type": "request", "command": "rename",
            "arguments": {"file": "mixed.ts", "line": 6, "offset": 26},
        }),
    ]);

    assert_eq!(responses.len(), 4);
    assert!(
        responses.iter().all(|response| response["success"] == true),
        "{responses:#?}"
    );
    let target = json!({
        "start": {"line": 6, "offset": 26},
        "end": {"line": 6, "offset": 32},
    });
    assert_eq!(responses[1]["body"]["start"], target["start"]);
    assert_eq!(responses[1]["body"]["end"], target["end"]);
    assert_eq!(responses[2]["body"]["textSpan"], target);
    assert_eq!(
        responses[2]["body"]["definitions"][0]["start"],
        target["start"]
    );
    assert_eq!(responses[2]["body"]["definitions"][0]["end"], target["end"]);
    assert_eq!(
        responses[3]["body"]["info"]["triggerSpan"],
        json!({
            "start": {"line": 6, "offset": 26},
            "end": {"line": 6, "offset": 32},
            "length": 6,
        })
    );
    let locations = responses[3]["body"]["locs"][0]["locs"].as_array().unwrap();
    assert_eq!(locations.len(), 2);
    assert_eq!(locations[0]["start"], json!({"line": 6, "offset": 26}));
    assert_eq!(locations[0]["end"], json!({"line": 6, "offset": 32}));
    assert_eq!(locations[1]["start"], json!({"line": 6, "offset": 46}));
    assert_eq!(locations[1]["end"], json!({"line": 6, "offset": 52}));
}

#[test]
fn incremental_changes_rebuild_coordinates_and_invalid_positions_do_not_mutate_text() {
    let initial = "const face = \"😀\";\r\nconst target = 1;\u{2028}target;";
    let changed = "const face = \"😀\";\r\nconst renamed = 1;\u{2028}renamed;";
    let responses = run([
        json!({
            "seq": 1, "type": "request", "command": "open",
            "arguments": {"file": "change.ts", "fileContent": initial},
        }),
        json!({
            "seq": 2, "type": "request", "command": "change",
            "arguments": {
                "file": "change.ts", "line": 1, "offset": 16,
                "endLine": 1, "endOffset": 16, "insertString": "x",
            },
        }),
        json!({
            "seq": 3, "type": "request", "command": "tsz/text",
            "arguments": {"file": "change.ts"},
        }),
        json!({
            "seq": 4, "type": "request", "command": "change",
            "arguments": {
                "file": "change.ts", "line": 2, "offset": 7,
                "endLine": 2, "endOffset": 13, "insertString": "renamed",
            },
        }),
        json!({
            "seq": 5, "type": "request", "command": "change",
            "arguments": {
                "file": "change.ts", "line": 3, "offset": 1,
                "endLine": 3, "endOffset": 7, "insertString": "renamed",
            },
        }),
        json!({
            "seq": 6, "type": "request", "command": "tsz/text",
            "arguments": {"file": "change.ts"},
        }),
        json!({
            "seq": 7, "type": "request", "command": "quickinfo",
            "arguments": {"file": "change.ts", "line": 3, "offset": 1},
        }),
        json!({
            "seq": 8, "type": "request", "command": "definitionAndBoundSpan",
            "arguments": {"file": "change.ts", "line": 3, "offset": 1},
        }),
        json!({
            "seq": 9, "type": "request", "command": "quickinfo",
            "arguments": {"file": "change.ts", "line": 0, "offset": 1},
        }),
        json!({
            "seq": 10, "type": "request", "command": "quickinfo",
            "arguments": {"file": "change.ts", "line": 1, "offset": 16},
        }),
        json!({
            "seq": 11, "type": "request", "command": "quickinfo",
            "arguments": {"file": "change.ts", "line": 4, "offset": 1},
        }),
        json!({
            "seq": 12, "type": "request", "command": "quickinfo",
            "arguments": {"file": "change.ts", "line": 2, "offset": 999},
        }),
        json!({
            "seq": 13, "type": "request", "command": "quickinfo",
            "arguments": {"file": "change.ts", "line": u64::MAX, "offset": 1},
        }),
    ]);

    assert_eq!(responses.len(), 13);
    assert_eq!(responses[1]["success"], false);
    assert_eq!(responses[2]["body"], initial);
    assert_eq!(responses[3]["success"], true);
    assert_eq!(responses[4]["success"], true);
    assert_eq!(responses[5]["body"], changed);
    assert_eq!(responses[6]["success"], true);
    assert_eq!(
        responses[6]["body"]["start"],
        json!({"line": 3, "offset": 1})
    );
    assert_eq!(responses[6]["body"]["end"], json!({"line": 3, "offset": 8}));
    assert_eq!(
        responses[7]["body"]["definitions"][0]["start"],
        json!({"line": 2, "offset": 7})
    );
    assert_eq!(
        responses[7]["body"]["definitions"][0]["end"],
        json!({"line": 2, "offset": 14})
    );
    for response in &responses[8..] {
        assert_eq!(response["success"], false, "{response:#}");
        assert!(response.get("body").is_none(), "{response:#}");
    }
}

#[test]
fn crlf_interior_is_addressable_but_the_next_line_alias_is_invalid() {
    let initial = "a\r\nconst value = 1;";
    let responses = run([
        json!({
            "seq": 1, "type": "request", "command": "open",
            "arguments": {"file": "crlf.ts", "fileContent": initial},
        }),
        json!({
            "seq": 2, "type": "request", "command": "change",
            "arguments": {
                "file": "crlf.ts", "line": 1, "offset": 2,
                "endLine": 1, "endOffset": 2, "insertString": "",
            },
        }),
        json!({
            "seq": 3, "type": "request", "command": "change",
            "arguments": {
                "file": "crlf.ts", "line": 1, "offset": 4,
                "endLine": 1, "endOffset": 4, "insertString": "bad",
            },
        }),
        json!({
            "seq": 4, "type": "request", "command": "tsz/text",
            "arguments": {"file": "crlf.ts"},
        }),
        json!({
            "seq": 5, "type": "request", "command": "change",
            "arguments": {
                "file": "crlf.ts", "line": 1, "offset": 3,
                "endLine": 1, "endOffset": 3, "insertString": "x",
            },
        }),
        json!({
            "seq": 6, "type": "request", "command": "tsz/text",
            "arguments": {"file": "crlf.ts"},
        }),
    ]);

    assert_eq!(responses[1]["success"], true, "CR position must be valid");
    assert_eq!(responses[2]["success"], false);
    assert_eq!(responses[3]["body"], initial);
    assert_eq!(responses[4]["success"], true, "LF position must be valid");
    assert_eq!(responses[5]["body"], "a\rx\nconst value = 1;");
}

#[test]
fn navigation_nonclaims_fail_without_bodies_while_claimed_absence_stays_empty() {
    let unsupported = concat!(
        "const holder = { renamed<Value>(value: Value) { return value; } }; ",
        "holder.renamed;",
    );
    let unsupported_offset = unsupported.rfind("renamed").unwrap() as u64 + 2;
    let commands = [
        "quickinfo",
        "definitionAndBoundSpan",
        "definition",
        "typeDefinition",
        "references-full",
        "documentHighlights",
        "rename",
    ];
    let mut requests = vec![json!({
        "seq": 1, "type": "request", "command": "open",
        "arguments": {"file": "unsupported.ts", "fileContent": unsupported},
    })];
    for (index, command) in commands.iter().enumerate() {
        requests.push(json!({
            "seq": index + 2, "type": "request", "command": command,
            "arguments": {
                "file": "unsupported.ts", "line": 1, "offset": unsupported_offset,
                "filesToSearch": ["unsupported.ts"],
            },
        }));
    }
    requests.push(json!({
        "seq": 9, "type": "request", "command": "tsz/reset", "arguments": {},
    }));
    requests.push(json!({
        "seq": 10, "type": "request", "command": "open",
        "arguments": {"file": "empty.ts", "fileContent": "const value = 1;"},
    }));
    for (index, command) in commands.iter().enumerate() {
        requests.push(json!({
            "seq": index + 11, "type": "request", "command": command,
            "arguments": {
                "file": "empty.ts", "line": 1, "offset": 1,
                "filesToSearch": ["empty.ts"],
            },
        }));
    }

    let responses = run(requests);
    assert_eq!(responses.len(), 17);
    for (response, command) in responses[1..8].iter().zip(commands) {
        assert_eq!(response["success"], false, "{response:#}");
        assert!(response.get("body").is_none(), "{response:#}");
        assert_eq!(
            response["message"],
            format!("TSZ {command} incomplete: deferred")
        );
    }

    assert_eq!(responses[8]["success"], true);
    assert_eq!(responses[9]["success"], true);
    let claimed = &responses[10..17];
    assert_eq!(claimed[0]["success"], false);
    assert_eq!(
        claimed[0]["message"],
        "No content available at the requested position."
    );
    assert!(
        !claimed[0]["message"]
            .as_str()
            .unwrap()
            .contains("incomplete")
    );
    assert_eq!(claimed[1]["success"], true);
    assert_eq!(claimed[1]["body"], json!({"definitions": []}));
    for response in &claimed[2..6] {
        assert_eq!(response["success"], true, "{response:#}");
        assert_eq!(response["body"], json!([]), "{response:#}");
    }
    assert_eq!(claimed[6]["success"], true);
    assert_eq!(claimed[6]["body"]["info"]["canRename"], false);
    assert_eq!(claimed[6]["body"]["locs"], json!([]));
}

fn run(requests: impl IntoIterator<Item = Value>) -> Vec<Value> {
    let mut input = Vec::new();
    for request in requests {
        let body = serde_json::to_vec(&request).unwrap();
        input.extend_from_slice(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes());
        input.extend_from_slice(&body);
    }
    let mut output = Vec::new();
    run_tsserver(Cursor::new(input), &mut output).unwrap();
    decode_messages(&output)
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
