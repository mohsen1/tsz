use std::io::{Cursor, Read};

use serde_json::{Value, json};

use super::run_tsserver;

#[test]
fn protocol_type_definition_uses_the_distinct_service_product() {
    let source = "class Item {}\nconst value = new Item();\nvalue;";
    let responses = run([
        json!({
            "seq": 1, "type": "request", "command": "open",
            "arguments": {"file": "case.ts", "fileContent": source},
        }),
        json!({
            "seq": 2, "type": "request", "command": "definition",
            "arguments": {"file": "case.ts", "line": 3, "offset": 1},
        }),
        json!({
            "seq": 3, "type": "request", "command": "typeDefinition",
            "arguments": {"file": "case.ts", "line": 3, "offset": 1},
        }),
        json!({
            "seq": 4, "type": "request", "command": "definitionAndBoundSpan",
            "arguments": {"file": "case.ts", "line": 3, "offset": 1},
        }),
    ]);

    assert!(responses.iter().all(|response| response["success"] == true));
    assert_eq!(responses[1]["body"][0]["name"], "value");
    assert_eq!(responses[1]["body"][0]["kind"], "const");
    assert_eq!(responses[2]["body"][0]["name"], "Item");
    assert_eq!(responses[2]["body"][0]["kind"], "class");
    for definition in [
        &responses[1]["body"][0],
        &responses[2]["body"][0],
        &responses[3]["body"]["definitions"][0],
    ] {
        assert_eq!(definition["containerKind"], "");
    }
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
