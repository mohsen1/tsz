use super::*;

#[test]
fn test_definition_response_entries_have_valid_spans() {
    // Each definition entry in the response must have start/end spans with
    // valid line/offset fields.
    let mut server = make_server();
    server
        .open_files
        .insert("/test.ts".to_string(), "const x = 1;\nx;".to_string());
    // Open file with actual newline
    server.open_files.insert(
        "/test.ts".to_string(),
        "const x = 1;
x;"
        .to_string(),
    );
    let req = make_request(
        "definition",
        serde_json::json!({"file": "/test.ts", "line": 2, "offset": 1}),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("definition should return a body");
    // The body is an array; each entry must have start/end and file
    if let Some(arr) = body.as_array() {
        for (i, entry) in arr.iter().enumerate() {
            assert_valid_span(entry, &format!("definition entry {i}"));
            assert!(
                entry.get("file").is_some(),
                "definition entry {i} must have 'file'"
            );
        }
    }
}

#[test]
fn test_definition_empty_response_is_valid_array() {
    // When no definition is found, the response must be an empty array,
    // not null or an object missing start/end.
    let mut server = make_server();
    server
        .open_files
        .insert("/test.ts".to_string(), "   ".to_string());
    let req = make_request(
        "definition",
        serde_json::json!({"file": "/test.ts", "line": 1, "offset": 1}),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("definition should return a body");
    assert!(body.is_array(), "definition fallback must be an array");
}

#[test]
fn test_definition_and_bound_span_has_no_body_without_definition() {
    let mut server = make_server();
    server
        .open_files
        .insert("/test.ts".to_string(), "   ".to_string());
    let req = make_request(
        "definitionAndBoundSpan",
        serde_json::json!({"file": "/test.ts", "line": 1, "offset": 1}),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    assert!(
        resp.body.is_none(),
        "definitionAndBoundSpan should omit body when no definition exists"
    );
}

#[test]
fn test_navtree_fallback_has_spans() {
    // The navtree/navbar fallback must include a spans array so the harness
    // does not crash when iterating item.spans.
    let mut server = make_server();
    let req = make_request("navtree", serde_json::json!({"file": "/nonexistent.ts"}));
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("navtree should return a body");
    let spans = body.get("spans");
    assert!(spans.is_some(), "navtree fallback must have spans array");
    let spans_arr = spans.unwrap().as_array().expect("spans must be an array");
    assert!(
        !spans_arr.is_empty(),
        "navtree fallback must have at least one span"
    );
    assert_valid_span(&spans_arr[0], "navtree fallback span");
}

#[test]
fn test_navtree_full_returns_numeric_text_spans() {
    let mut server = make_server();
    let source = "export function f(x: number) {\n  return x;\n}\n";
    server
        .open_files
        .insert("/a.ts".to_string(), source.to_string());

    let req = make_request("navtree-full", serde_json::json!({"file": "/a.ts"}));
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("navtree-full should return a body");
    let root_span = body
        .get("spans")
        .and_then(serde_json::Value::as_array)
        .and_then(|spans| spans.first())
        .expect("navtree-full root should include a span");
    assert_eq!(
        root_span.get("start").and_then(serde_json::Value::as_u64),
        Some(0),
        "navtree-full root span should use numeric TextSpan shape: {root_span:?}"
    );
    assert_eq!(
        root_span.get("length").and_then(serde_json::Value::as_u64),
        Some(source.len() as u64),
        "navtree-full root span should cover the source text: {root_span:?}"
    );

    let function_item = body
        .get("childItems")
        .and_then(serde_json::Value::as_array)
        .and_then(|items| {
            items
                .iter()
                .find(|item| item.get("text").and_then(serde_json::Value::as_str) == Some("f"))
        })
        .expect("navtree-full should include function f");
    let function_span = function_item
        .get("spans")
        .and_then(serde_json::Value::as_array)
        .and_then(|spans| spans.first())
        .expect("function item should include a span");
    assert!(
        function_span
            .get("start")
            .and_then(serde_json::Value::as_u64)
            .is_some(),
        "function span should use numeric TextSpan shape: {function_span:?}"
    );
    let name_span = function_item
        .get("nameSpan")
        .expect("function item should include numeric nameSpan");
    assert_eq!(
        name_span.get("start").and_then(serde_json::Value::as_u64),
        Some(16),
        "function nameSpan should start at the function name: {name_span:?}"
    );
    assert_eq!(
        name_span.get("length").and_then(serde_json::Value::as_u64),
        Some(1),
        "function nameSpan should cover the function name: {name_span:?}"
    );
}

#[test]
fn test_references_response_entries_have_valid_spans() {
    // Each reference entry must have valid start/end spans.
    let mut server = make_server();
    server.open_files.insert(
        "/test.ts".to_string(),
        "const x = 1;
x;
x;"
        .to_string(),
    );
    let req = make_request(
        "references",
        serde_json::json!({"file": "/test.ts", "line": 1, "offset": 7}),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("references should return a body");
    let refs = body.get("refs").expect("references must have refs array");
    if let Some(arr) = refs.as_array() {
        for (i, entry) in arr.iter().enumerate() {
            assert_valid_span(entry, &format!("reference entry {i}"));
        }
    }
    assert!(
        body.get("symbolName").is_some(),
        "references must have symbolName"
    );
}

#[test]
fn test_references_response_includes_symbol_metadata() {
    let mut server = make_server();
    server.open_files.insert(
        "/index.ts".to_string(),
        "const alpha = 1;\nalpha;\n".to_string(),
    );
    let req = make_request(
        "references",
        serde_json::json!({"file": "/index.ts", "line": 2, "offset": 1}),
    );

    let resp = server.handle_tsserver_request(req);

    assert!(resp.success);
    let body = resp.body.expect("references should return a body");
    assert_eq!(body.get("symbolName"), Some(&serde_json::json!("alpha")));
    assert_eq!(body.get("symbolStartOffset"), Some(&serde_json::json!(1)));
    assert_eq!(
        body.get("symbolDisplayString"),
        Some(&serde_json::json!("const alpha: 1"))
    );
}

#[test]
fn test_alias_string_literal_navigation_uses_project_wide_resolution() {
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
    let (arena, _binder, _root, source_text) = server
        .parse_and_bind_file("/bar.ts")
        .expect("expected parse_and_bind_file for /bar.ts");
    let line_map = tsz::lsp::position::LineMap::build(&source_text);
    let probe_pos = Server::tsserver_to_lsp_position(1, 14);
    let probe_off = line_map
        .position_to_offset(probe_pos, &source_text)
        .expect("offset at marker");
    let alias_query = server.debug_alias_query_target(&arena, &source_text, probe_off);
    let direct_resolve =
        server.debug_resolve_export_alias_definition("/bar.ts", "./foo", "__<alias>");
    let probe_node =
        tsz::lsp::utils::find_node_at_or_before_offset(&arena, probe_off, &source_text);
    let probe_kind = arena.kind_at(probe_node).unwrap_or_default();
    let mut chain = Vec::new();
    let mut walk = probe_node;
    while walk.is_some() {
        if let Some(node) = arena.get(walk) {
            chain.push(node.kind);
        }
        let Some(ext) = arena.get_extended(walk) else {
            break;
        };
        walk = ext.parent;
    }
    let canonical =
        server.canonical_definition_for_alias_position("/bar.ts", &arena, &source_text, probe_off);
    assert!(
        canonical.is_some(),
        "expected canonical definition for alias specifier (off={probe_off}, kind={probe_kind}, chain={chain:?}, alias_query={alias_query:?}, direct_resolve={direct_resolve:?})"
    );

    let definition_req = make_request(
        "definition",
        serde_json::json!({
            "file": "/bar.ts",
            "line": 1,
            "offset": 14
        }),
    );
    let definition_resp = server.handle_tsserver_request(definition_req);
    assert!(definition_resp.success);
    let definition_body = definition_resp
        .body
        .expect("definition should return body")
        .as_array()
        .cloned()
        .expect("definition response should be an array");
    assert!(
        definition_body.iter().any(|entry| {
            entry.get("file").and_then(serde_json::Value::as_str) == Some("/foo.ts")
        }),
        "expected alias definition to include /foo.ts, got: {definition_body:?}"
    );

    let references_req = make_request(
        "references",
        serde_json::json!({
            "file": "/bar.ts",
            "line": 1,
            "offset": 14
        }),
    );
    let references_resp = server.handle_tsserver_request(references_req);
    assert!(references_resp.success);
    let references_body = references_resp.body.expect("references should return body");
    let refs = references_body["refs"]
        .as_array()
        .cloned()
        .expect("references should have refs");
    assert!(
        refs.iter()
            .filter_map(|entry| entry.get("file").and_then(serde_json::Value::as_str))
            .any(|file| file == "/foo.ts"),
        "expected refs to include /foo.ts, got: {refs:?}"
    );
}

#[test]
fn test_definition_and_bound_span_quoted_local_export_alias_has_token_span() {
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

    let req = make_request(
        "definitionAndBoundSpan",
        serde_json::json!({
            "file": "/foo.ts",
            "line": 2,
            "offset": 19
        }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp
        .body
        .expect("definitionAndBoundSpan should return body");
    let text_span = body.get("textSpan").expect("textSpan must be present");
    assert_valid_span(text_span, "quoted export alias textSpan");
    let start = text_span["start"]["offset"]
        .as_u64()
        .expect("textSpan.start.offset should be numeric");
    let end = text_span["end"]["offset"]
        .as_u64()
        .expect("textSpan.end.offset should be numeric");
    assert!(
        end > start,
        "quoted export alias textSpan must be non-empty (start={start}, end={end})"
    );
}

#[test]
fn test_quoted_alias_chain_references_and_rename_stay_on_quoted_specifiers() {
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

    let refs_req = make_request(
        "references",
        serde_json::json!({
            "file": "/bar.ts",
            "line": 2,
            "offset": 12
        }),
    );
    let refs_resp = server.handle_tsserver_request(refs_req);
    assert!(refs_resp.success);
    let refs = refs_resp
        .body
        .expect("references should return body")
        .get("refs")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .expect("references should include refs array");
    assert!(
        refs.len() >= 3,
        "expected multiple quoted alias refs across files, got: {refs:?}"
    );
    assert!(
        refs.iter()
            .filter_map(|entry| entry.get("file").and_then(serde_json::Value::as_str))
            .any(|file| file == "/foo.ts"),
        "expected quoted alias refs to include /foo.ts"
    );
    assert!(
        refs.iter().all(|entry| {
            entry
                .get("lineText")
                .and_then(serde_json::Value::as_str)
                .is_none_or(|line| {
                    !line.contains("if (bar")
                        && !line.contains("if (first")
                        && !line.contains("if (second")
                })
        }),
        "expected quoted alias refs to stay on quoted import/export specifiers: {refs:?}"
    );

    let rename_req = make_request(
        "rename",
        serde_json::json!({
            "file": "/bar.ts",
            "line": 2,
            "offset": 12
        }),
    );
    let rename_resp = server.handle_tsserver_request(rename_req);
    assert!(rename_resp.success);
    let locs = rename_resp
        .body
        .expect("rename should return body")
        .get("locs")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .expect("rename should include locs array");
    assert!(
        locs.iter().any(|entry| {
            entry.get("file").and_then(serde_json::Value::as_str) == Some("/foo.ts")
        }),
        "expected rename locations to include /foo.ts: {locs:?}"
    );
    assert!(
        locs.iter().any(|entry| {
            entry.get("file").and_then(serde_json::Value::as_str) == Some("/bar.ts")
        }),
        "expected rename locations to include /bar.ts: {locs:?}"
    );
}

#[test]
fn test_rename_from_export_quoted_alias_filters_non_specifier_locations() {
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

    let rename_req = make_request(
        "rename",
        serde_json::json!({
            "file": "/foo.ts",
            "line": 2,
            "offset": 21
        }),
    );
    let rename_resp = server.handle_tsserver_request(rename_req);
    assert!(rename_resp.success);
    let body = rename_resp.body.expect("rename should return body");
    let loc_groups = body
        .get("locs")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .expect("rename should include locs array");
    assert!(
        loc_groups.iter().any(|entry| {
            entry.get("file").and_then(serde_json::Value::as_str) == Some("/foo.ts")
        }),
        "expected rename locations to include /foo.ts: {loc_groups:?}"
    );
    assert!(
        loc_groups.iter().any(|entry| {
            entry.get("file").and_then(serde_json::Value::as_str) == Some("/bar.ts")
        }),
        "expected rename locations to include /bar.ts: {loc_groups:?}"
    );

    for group in &loc_groups {
        let file = group
            .get("file")
            .and_then(serde_json::Value::as_str)
            .expect("loc group should contain file");
        let source = server
            .open_files
            .get(file)
            .expect("test source should be present in open files");
        let lines: Vec<&str> = source.lines().collect();
        let locs = group
            .get("locs")
            .and_then(serde_json::Value::as_array)
            .expect("loc group should contain locs");
        for loc in locs {
            let line_one_based = loc
                .get("start")
                .and_then(|start| start.get("line"))
                .and_then(serde_json::Value::as_u64)
                .expect("loc start.line should be numeric");
            let line_idx = line_one_based.saturating_sub(1) as usize;
            let line_text = lines.get(line_idx).copied().unwrap_or("");
            assert!(
                line_text.contains("import {") || line_text.contains("export {"),
                "rename on quoted alias should stay on import/export specifiers, got line: {line_text}"
            );
            assert!(
                !line_text.contains("const foo")
                    && !line_text.contains("if (bar")
                    && !line_text.contains("if (first")
                    && !line_text.contains("if (second"),
                "rename on quoted alias should not include identifier usage lines, got line: {line_text}"
            );
        }
    }
}

#[test]
fn test_rename_quoted_alias_marker_offset_uses_literal_only_locations() {
    let mut server = make_server();
    server.open_files.insert(
        "/foo.ts".to_string(),
        [
            "const foo = \"foo\";",
            "export { foo as \"/*RENAME*/__<alias>\" };",
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

    // Offset lands inside the comment marker in the quoted export alias string literal.
    let req = make_request(
        "rename",
        serde_json::json!({
            "file": "/foo.ts",
            "line": 2,
            "offset": 19
        }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("rename should return body");
    let groups = body["locs"]
        .as_array()
        .expect("rename should include grouped locations");
    assert!(
        groups
            .iter()
            .any(|g| g.get("file").and_then(serde_json::Value::as_str) == Some("/foo.ts")),
        "expected /foo.ts rename locations: {groups:?}"
    );
    assert!(!groups.is_empty(), "expected rename locations: {groups:?}");

    for group in groups {
        let file = group["file"]
            .as_str()
            .expect("group.file should be a string");
        let source = server
            .open_files
            .get(file)
            .expect("source file should exist");
        let lines: Vec<&str> = source.lines().collect();
        for loc in group["locs"]
            .as_array()
            .expect("group.locs should be an array")
        {
            let line = loc["start"]["line"]
                .as_u64()
                .expect("start.line should be numeric")
                .saturating_sub(1) as usize;
            let line_text = lines.get(line).copied().unwrap_or_default();
            assert!(
                line_text.contains("import {") || line_text.contains("export {"),
                "rename should stay on import/export specifiers, got line: {line_text}"
            );
            assert!(
                !line_text.contains("\"<other>\""),
                "rename for __<alias> should not include <other> aliases, got line: {line_text}"
            );
            assert!(
                loc.get("contextStart").is_some() && loc.get("contextEnd").is_some(),
                "rename locations should carry context spans for fourslash baseline wrapping: {loc:?}"
            );
        }
    }
}

#[test]
fn test_document_highlights_import_specifier_dedupes_and_has_context() {
    let mut server = make_server();
    server.open_files.insert(
        "/a.ts".to_string(),
        "export const shared = 1;\nshared;\n".to_string(),
    );
    server.open_files.insert(
        "/b.ts".to_string(),
        "import { shared } from \"./a\";\nshared;\n".to_string(),
    );

    let req = make_request(
        "documentHighlights",
        serde_json::json!({
            "file": "/b.ts",
            "line": 1,
            "offset": 10,
            "filesToSearch": ["/b.ts"]
        }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("documentHighlights should return a body");
    let groups = body
        .as_array()
        .expect("documentHighlights body should be an array");
    let spans = groups
        .first()
        .and_then(|group| group.get("highlightSpans"))
        .and_then(serde_json::Value::as_array)
        .expect("documentHighlights should include highlightSpans");

    let import_spans: Vec<_> = spans
        .iter()
        .filter(|span| {
            span["start"]["line"].as_u64() == Some(1)
                && span["start"]["offset"].as_u64() == Some(10)
                && span["end"]["offset"].as_u64() == Some(16)
        })
        .collect();
    assert_eq!(
        import_spans.len(),
        1,
        "import specifier highlight should not be duplicated: {spans:?}"
    );
    let import_span = import_spans[0];
    assert_eq!(
        import_span.get("kind").and_then(serde_json::Value::as_str),
        Some("writtenReference"),
        "import specifier should be a writtenReference: {import_span:?}"
    );
    assert!(
        import_span.get("contextStart").is_some() && import_span.get("contextEnd").is_some(),
        "import specifier highlight should include import-line context: {import_span:?}"
    );
    assert!(
        spans.iter().any(|span| {
            span["start"]["line"].as_u64() == Some(2)
                && span["start"]["offset"].as_u64() == Some(1)
                && span["end"]["offset"].as_u64() == Some(7)
        }),
        "expected the local usage highlight too: {spans:?}"
    );
}

#[test]
fn test_references_full_quoted_alias_uses_inner_literal_span_and_cross_file_refs() {
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
        "expected at least one referenced symbol"
    );
    let mut refs: Vec<serde_json::Value> = Vec::new();
    for symbol_entry in &entries {
        let symbol_refs = symbol_entry["references"]
            .as_array()
            .cloned()
            .expect("referenced symbol should include references");
        refs.extend(symbol_refs);
    }
    assert!(
        refs.iter().any(|entry| {
            entry.get("fileName").and_then(serde_json::Value::as_str) == Some("/bar.ts")
        }),
        "expected cross-file references to include /bar.ts: {refs:?}"
    );
    let foo_source = server
        .open_files
        .get("/foo.ts")
        .cloned()
        .expect("missing /foo.ts");
    let has_inner_alias_span = refs.iter().any(|entry| {
        if entry.get("fileName").and_then(serde_json::Value::as_str) != Some("/foo.ts") {
            return false;
        }
        let start = entry["textSpan"]["start"].as_u64().unwrap_or(0) as usize;
        let len = entry["textSpan"]["length"].as_u64().unwrap_or(0) as usize;
        let end = start.saturating_add(len);
        foo_source.get(start..end) == Some("__<alias>")
    });
    assert!(
        has_inner_alias_span,
        "expected at least one /foo.ts reference span to map to inner alias text"
    );
}

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

#[test]
fn test_type_only_quoted_alias_references_work_from_type_keyword_offset() {
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

    // Place the query on `type`, not on the quoted string literal token.
    let refs_req = make_request(
        "references",
        serde_json::json!({
            "file": "/bar.ts",
            "line": 1,
            "offset": 10
        }),
    );
    let refs_resp = server.handle_tsserver_request(refs_req);
    assert!(refs_resp.success);
    let refs = refs_resp
        .body
        .expect("references should return body")
        .get("refs")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .expect("references should include refs array");
    assert!(
        refs.iter()
            .filter_map(|entry| entry.get("file").and_then(serde_json::Value::as_str))
            .any(|file| file == "/foo.ts"),
        "expected type-only quoted alias refs to include /foo.ts: {refs:?}"
    );
    assert!(
        refs.iter()
            .filter_map(|entry| entry.get("file").and_then(serde_json::Value::as_str))
            .any(|file| file == "/bar.ts"),
        "expected type-only quoted alias refs to include /bar.ts: {refs:?}"
    );
}

#[test]
fn test_type_only_quoted_alias_references_follow_local_alias_uses() {
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
        "references",
        serde_json::json!({
            "file": "/foo.ts",
            "line": 2,
            "offset": 24
        }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("references should return body");
    assert_eq!(
        body.get("symbolName").and_then(serde_json::Value::as_str),
        Some("\"__<alias>\""),
        "quoted type alias references should preserve symbolName: {body:?}"
    );
    let refs = body
        .get("refs")
        .and_then(serde_json::Value::as_array)
        .expect("references should include refs array");
    assert_eq!(
        refs.len(),
        12,
        "expected the full quoted type-only alias chain, got: {refs:?}"
    );

    let ref_text = |entry: &serde_json::Value| -> Option<String> {
        let file = entry.get("file")?.as_str()?;
        let source = server.open_files.get(file)?;
        let start = entry.get("start")?;
        let end = entry.get("end")?;
        let start_line = start.get("line")?.as_u64()? as usize;
        let start_offset = start.get("offset")?.as_u64()? as usize;
        let end_line = end.get("line")?.as_u64()? as usize;
        let end_offset = end.get("offset")?.as_u64()? as usize;
        if start_line != end_line || start_offset == 0 || end_offset == 0 {
            return None;
        }
        let line = source.lines().nth(start_line.checked_sub(1)?)?;
        line.get(start_offset - 1..end_offset - 1)
            .map(str::to_string)
    };

    let has_ref = |file: &str, line: u64, text: &str| {
        refs.iter().any(|entry| {
            entry.get("file").and_then(serde_json::Value::as_str) == Some(file)
                && entry
                    .get("start")
                    .and_then(|start| start.get("line"))
                    .and_then(serde_json::Value::as_u64)
                    == Some(line)
                && ref_text(entry).as_deref() == Some(text)
        })
    };

    for (file, line, text) in [
        ("/foo.ts", 2, "__<alias>"),
        ("/foo.ts", 3, "__<alias>"),
        ("/foo.ts", 3, "bar"),
        ("/foo.ts", 4, "bar"),
        ("/bar.ts", 1, "__<alias>"),
        ("/bar.ts", 1, "first"),
        ("/bar.ts", 2, "__<alias>"),
        ("/bar.ts", 2, "<other>"),
        ("/bar.ts", 3, "<other>"),
        ("/bar.ts", 3, "second"),
        ("/bar.ts", 4, "first"),
        ("/bar.ts", 5, "second"),
    ] {
        assert!(
            has_ref(file, line, text),
            "missing reference {file}:{line} {text:?}; refs: {refs:?}"
        );
    }

    let queried_export = refs.iter().find(|entry| {
        entry.get("file").and_then(serde_json::Value::as_str) == Some("/foo.ts")
            && entry
                .get("start")
                .and_then(|start| start.get("line"))
                .and_then(serde_json::Value::as_u64)
                == Some(2)
            && ref_text(entry).as_deref() == Some("__<alias>")
    });
    let queried_export = queried_export.expect("query export alias reference should be present");
    assert_eq!(
        queried_export
            .get("isDefinition")
            .and_then(serde_json::Value::as_bool),
        Some(true),
        "queried export alias should be marked as a definition: {queried_export:?}"
    );
    assert_eq!(
        queried_export
            .get("isWriteAccess")
            .and_then(serde_json::Value::as_bool),
        Some(true),
        "queried export alias should be marked as a write reference: {queried_export:?}"
    );
}

#[test]
fn test_definition_type_only_quoted_import_alias_resolves_to_exported_symbol() {
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

    let req = make_request(
        "definition",
        serde_json::json!({
            "file": "/foo.ts",
            "line": 3,
            "offset": 18
        }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let defs = resp
        .body
        .expect("definition should return body")
        .as_array()
        .cloned()
        .expect("definition response should be an array");
    assert!(
        defs.iter()
            .any(|entry| entry.get("name").and_then(serde_json::Value::as_str) == Some("foo")),
        "expected type-only quoted alias definition to resolve to exported symbol `foo`, got: {defs:?}"
    );
}

#[test]
fn test_definition_type_only_quoted_alias_marks_non_declare_target_as_local_non_ambient() {
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

    let req = make_request(
        "definition",
        serde_json::json!({
            "file": "/foo.ts",
            "line": 3,
            "offset": 18
        }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let defs = resp
        .body
        .expect("definition should return body")
        .as_array()
        .cloned()
        .expect("definition response should be an array");
    let foo_def = defs
        .iter()
        .find(|entry| entry.get("name").and_then(serde_json::Value::as_str) == Some("foo"))
        .expect("expected foo definition entry");
    assert_eq!(
        foo_def
            .get("isAmbient")
            .and_then(serde_json::Value::as_bool),
        Some(false),
        "non-declare quoted alias definition should not be ambient: {foo_def:?}"
    );
    assert_eq!(
        foo_def.get("isLocal").and_then(serde_json::Value::as_bool),
        Some(true),
        "non-declare quoted alias definition should be local: {foo_def:?}"
    );
}
