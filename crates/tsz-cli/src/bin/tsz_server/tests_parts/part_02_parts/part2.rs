#[test]
fn test_references_full_quoted_alias_definition_uses_file_name_and_text_span_shape() {
    let mut server = make_server();
    server.open_files.insert(
        "/foo.ts".to_string(),
        [
            "const foo = \"foo\";",
            "export { foo as \"__<alias>\" };",
            "import { \"__<alias>\" as bar } from \"./foo\";",
            "if (bar !== \"foo\") throw bar;",
        ]
        .join("\n"),
    );
    server.open_files.insert(
        "/bar.ts".to_string(),
        [
            "import { \"__<alias>\" as first } from \"./foo\";",
            "export { \"__<alias>\" as \"<other>\" } from \"./foo\";",
            "import { \"<other>\" as second } from \"./bar\";",
            "if (first !== \"foo\") throw first;",
            "if (second !== \"foo\") throw second;",
        ]
        .join("\n"),
    );

    let req = make_request(
        "references-full",
        serde_json::json!({
            "file": "/foo.ts",
            "line": 2,
            "offset": 19
        }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("references-full should return body");
    let entries = body
        .as_array()
        .expect("references-full response should be array");
    let first = entries
        .first()
        .expect("expected at least one referenced symbol");
    let definition = first
        .get("definition")
        .expect("referenced symbol should include definition");

    assert!(
        definition.get("fileName").is_some(),
        "definition should expose tsserver fileName in references-full: {definition:?}"
    );
    let text_span = definition
        .get("textSpan")
        .expect("definition should expose tsserver textSpan");
    assert!(
        text_span
            .get("start")
            .and_then(serde_json::Value::as_u64)
            .is_some()
            && text_span
                .get("length")
                .and_then(serde_json::Value::as_u64)
                .is_some(),
        "textSpan should include numeric start/length: {text_span:?}"
    );
    assert!(
        definition.get("file").is_none()
            && definition.get("start").is_none()
            && definition.get("end").is_none(),
        "references-full definition should not use definition-command fields: {definition:?}"
    );
}

#[test]
fn test_references_full_quoted_alias_includes_symbol_alias_references_when_available() {
    let mut server = make_server();
    server.open_files.insert(
        "/foo.ts".to_string(),
        [
            "type foo = \"foo\";",
            "export { type foo as \"__<alias>\" };",
            "import { type \"__<alias>\" as bar } from \"./foo\";",
            "const testBar: bar = \"foo\";",
        ]
        .join("\n"),
    );
    server.open_files.insert(
        "/bar.ts".to_string(),
        [
            "import { type \"__<alias>\" as first } from \"./foo\";",
            "export { type \"__<alias>\" as \"<other>\" } from \"./foo\";",
            "import { type \"<other>\" as second } from \"./bar\";",
            "const testFirst: first = \"foo\";",
            "const testSecond: second = \"foo\";",
        ]
        .join("\n"),
    );

    let req = make_request(
        "references-full",
        serde_json::json!({
            "file": "/foo.ts",
            "line": 2,
            "offset": 24
        }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("references-full should return body");
    let entries = body
        .as_array()
        .cloned()
        .expect("references-full response should be array");
    assert!(
        !entries.is_empty(),
        "expected at least one referenced symbol entry"
    );
    let mut seen_bar_alias = false;
    let mut seen_first_alias = false;
    let mut all_refs: Vec<serde_json::Value> = Vec::new();
    for symbol_entry in &entries {
        let refs = symbol_entry["references"]
            .as_array()
            .cloned()
            .expect("referenced symbol should include references");
        for entry in refs {
            let file = entry
                .get("fileName")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            let source = server
                .open_files
                .get(file)
                .expect("reference file should be open");
            let start = entry["textSpan"]["start"].as_u64().unwrap_or(0) as usize;
            let len = entry["textSpan"]["length"].as_u64().unwrap_or(0) as usize;
            let end = start.saturating_add(len);
            let text = source.get(start..end).unwrap_or_default();
            if text == "bar" {
                seen_bar_alias = true;
            }
            if text == "first" {
                seen_first_alias = true;
            }
            all_refs.push(entry);
        }
    }

    assert!(
        seen_bar_alias,
        "expected symbol references to include imported alias 'bar': {all_refs:?}"
    );
    assert!(
        seen_first_alias,
        "expected symbol references to include cross-file imported alias 'first': {all_refs:?}"
    );
}

#[test]
fn test_references_full_quoted_alias_does_not_duplicate_reference_spans_across_groups() {
    let mut server = make_server();
    server.open_files.insert(
        "/foo.ts".to_string(),
        [
            "type foo = \"foo\";",
            "export { type foo as \"__<alias>\" };",
            "import { type \"__<alias>\" as bar } from \"./foo\";",
            "const testBar: bar = \"foo\";",
        ]
        .join("\n"),
    );
    server.open_files.insert(
        "/bar.ts".to_string(),
        [
            "import { type \"__<alias>\" as first } from \"./foo\";",
            "export { type \"__<alias>\" as \"<other>\" } from \"./foo\";",
            "import { type \"<other>\" as second } from \"./bar\";",
            "const testFirst: first = \"foo\";",
            "const testSecond: second = \"foo\";",
        ]
        .join("\n"),
    );

    let req = make_request(
        "references-full",
        serde_json::json!({
            "file": "/foo.ts",
            "line": 2,
            "offset": 24
        }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("references-full should return body");
    let entries = body
        .as_array()
        .expect("references-full response should be array");

    let mut counts: std::collections::HashMap<(String, u64, u64), usize> =
        std::collections::HashMap::new();
    for symbol_entry in entries {
        let refs = symbol_entry["references"]
            .as_array()
            .expect("referenced symbol should include references");
        for entry in refs {
            let file = entry["fileName"]
                .as_str()
                .expect("reference.fileName should be a string")
                .to_string();
            let start = entry["textSpan"]["start"]
                .as_u64()
                .expect("reference.textSpan.start should be numeric");
            let length = entry["textSpan"]["length"]
                .as_u64()
                .expect("reference.textSpan.length should be numeric");
            *counts.entry((file, start, length)).or_insert(0) += 1;
        }
    }

    let duplicates: Vec<_> = counts.into_iter().filter(|(_, count)| *count > 1).collect();
    assert!(
        duplicates.is_empty(),
        "each reference span should belong to only one referenced-symbol group, duplicates: {duplicates:?}"
    );
}

#[test]
fn test_references_full_quoted_alias_returns_multiple_symbol_groups() {
    let mut server = make_server();
    server.open_files.insert(
        "/foo.ts".to_string(),
        [
            "type foo = \"foo\";",
            "export { type foo as \"__<alias>\" };",
            "import { type \"__<alias>\" as bar } from \"./foo\";",
            "const testBar: bar = \"foo\";",
        ]
        .join("\n"),
    );
    server.open_files.insert(
        "/bar.ts".to_string(),
        [
            "import { type \"__<alias>\" as first } from \"./foo\";",
            "export { type \"__<alias>\" as \"<other>\" } from \"./foo\";",
            "import { type \"<other>\" as second } from \"./bar\";",
            "const testFirst: first = \"foo\";",
            "const testSecond: second = \"foo\";",
        ]
        .join("\n"),
    );

    let req = make_request(
        "references-full",
        serde_json::json!({
            "file": "/foo.ts",
            "line": 2,
            "offset": 24
        }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("references-full should return body");
    let entries = body
        .as_array()
        .expect("references-full response should be array");

    assert!(
        entries.len() > 1,
        "quoted alias references-full should preserve multiple symbol groups, got: {entries:?}"
    );
    assert!(
        entries.iter().any(|entry| {
            entry["definition"]
                .get("kind")
                .and_then(serde_json::Value::as_str)
                == Some("alias")
        }),
        "expected at least one alias definition group: {entries:?}"
    );
    assert!(
        entries.iter().any(|entry| {
            entry["references"].as_array().is_some_and(|refs| {
                refs.iter().any(|r| {
                    r.get("fileName").and_then(serde_json::Value::as_str) == Some("/bar.ts")
                })
            })
        }),
        "expected at least one group with /bar.ts references: {entries:?}"
    );
}
