use super::*;

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

    // Symbol metadata (`name`) lives on the `-full` shape, not on plain
    // `definition` (see #4002). Use `definition-full` to inspect it.
    let req = make_request(
        "definition-full",
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
        .expect("definition-full should return body")
        .as_array()
        .cloned()
        .expect("definition-full response should be an array");
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

    // `isAmbient` / `isLocal` live on the `-full` shape, not on plain
    // `definition` (see #4002). Use `definition-full` to inspect them.
    let req = make_request(
        "definition-full",
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
        .expect("definition-full should return body")
        .as_array()
        .cloned()
        .expect("definition-full response should be an array");
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

// =============================================================================
// Issue #4002: definition vs definition-full response shape parity with tsc
// =============================================================================
//
// Plain `definition` returns `FileSpanWithContext`: `file`, `start`/`end`
// line/offset positions, optional `contextStart`/`contextEnd` line/offset
// positions. It must NOT include symbol metadata (`kind`, `name`,
// `containerName`, `isLocal`, `isAmbient`, `unverified`,
// `failedAliasResolution`) — those belong to the `-full` shape only.
//
// `definition-full` returns `DefinitionInfo`: `fileName`, numeric `textSpan`
// (`start`/`length`), optional numeric `contextSpan`, plus all the symbol
// metadata fields above. It must NOT include the plain `file`/`start`/`end`
// fields.

#[test]
fn test_definition_plain_shape_omits_full_only_fields() {
    let mut server = make_server();
    server.open_files.insert(
        "/a.ts".to_string(),
        "const alpha = 1;\nalpha;\n".to_string(),
    );

    let req = make_request(
        "definition",
        serde_json::json!({"file": "/a.ts", "line": 2, "offset": 1}),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("definition should return body");
    let entries = body
        .as_array()
        .expect("definition response should be an array");
    let entry = entries.first().expect("expected at least one definition");

    // FileSpan fields must be present.
    assert!(
        entry.get("file").is_some(),
        "plain definition must have 'file': {entry:?}"
    );
    assert!(
        entry.get("start").is_some(),
        "plain definition must have 'start': {entry:?}"
    );
    assert!(
        entry.get("end").is_some(),
        "plain definition must have 'end': {entry:?}"
    );

    // -full-only fields must be absent.
    for forbidden in [
        "fileName",
        "textSpan",
        "contextSpan",
        "kind",
        "name",
        "containerName",
        "containerKind",
        "isLocal",
        "isAmbient",
        "unverified",
        "failedAliasResolution",
    ] {
        assert!(
            entry.get(forbidden).is_none(),
            "plain definition must not include `{forbidden}`: {entry:?}"
        );
    }
}

#[test]
fn test_definition_full_shape_uses_filename_and_text_span() {
    let mut server = make_server();
    let source = "const alpha = 1;\nalpha;\n";
    server
        .open_files
        .insert("/a.ts".to_string(), source.to_string());

    let req = make_request(
        "definition-full",
        serde_json::json!({"file": "/a.ts", "line": 2, "offset": 1}),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("definition-full should return body");
    let entries = body
        .as_array()
        .expect("definition-full response should be an array");
    let entry = entries.first().expect("expected at least one definition");

    // -full uses `fileName` + numeric `textSpan` and must not include the
    // plain-shape fields.
    assert_eq!(
        entry.get("fileName").and_then(serde_json::Value::as_str),
        Some("/a.ts"),
        "definition-full should expose fileName: {entry:?}"
    );
    let text_span = entry.get("textSpan").expect("expected textSpan");
    assert_eq!(
        text_span.get("start").and_then(serde_json::Value::as_u64),
        Some(6),
        "alpha starts at byte 6: {text_span:?}"
    );
    assert_eq!(
        text_span.get("length").and_then(serde_json::Value::as_u64),
        Some(5),
        "alpha is 5 bytes long: {text_span:?}"
    );
    assert!(
        entry.get("file").is_none()
            && entry.get("start").is_none()
            && entry.get("end").is_none()
            && entry.get("contextStart").is_none()
            && entry.get("contextEnd").is_none(),
        "definition-full must not use plain definition fields: {entry:?}"
    );

    // Symbol metadata must be present on the -full shape.
    assert_eq!(
        entry.get("kind").and_then(serde_json::Value::as_str),
        Some("const")
    );
    assert_eq!(
        entry.get("name").and_then(serde_json::Value::as_str),
        Some("alpha")
    );
    assert!(entry.get("containerName").is_some());
    assert!(entry.get("isLocal").is_some());
    assert!(entry.get("isAmbient").is_some());
    assert_eq!(
        entry.get("unverified").and_then(serde_json::Value::as_bool),
        Some(false)
    );
    assert_eq!(
        entry
            .get("failedAliasResolution")
            .and_then(serde_json::Value::as_bool),
        Some(false)
    );

    // contextSpan is present and uses numeric start/length covering the
    // declaration `const alpha = 1;`.
    let context_span = entry.get("contextSpan").expect("expected contextSpan");
    assert_eq!(
        context_span
            .get("start")
            .and_then(serde_json::Value::as_u64),
        Some(0),
        "context starts at the `const` keyword: {context_span:?}"
    );
    assert_eq!(
        context_span
            .get("length")
            .and_then(serde_json::Value::as_u64),
        Some(16),
        "context covers `const alpha = 1;`: {context_span:?}"
    );
}

#[test]
fn test_type_definition_plain_shape_omits_full_only_fields() {
    // typeDefinition uses a dedicated handler and returns type declaration spans.
    // leak -full symbol metadata.
    let mut server = make_server();
    server.open_files.insert(
        "/a.ts".to_string(),
        "interface I { x: number; }\nconst v: I = { x: 1 };\nv;\n".to_string(),
    );

    let req = make_request(
        "typeDefinition",
        serde_json::json!({"file": "/a.ts", "line": 3, "offset": 1}),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("typeDefinition should return body");
    let entries = body
        .as_array()
        .expect("typeDefinition response should be an array");
    if let Some(entry) = entries.first() {
        assert_eq!(
            entry
                .get("start")
                .and_then(|start| start.get("line"))
                .and_then(|line| line.as_u64()),
            Some(1),
            "typeDefinition should resolve to the type declaration: {entry:?}"
        );
        for forbidden in [
            "fileName",
            "textSpan",
            "contextSpan",
            "kind",
            "name",
            "containerName",
            "isLocal",
            "isAmbient",
            "unverified",
            "failedAliasResolution",
        ] {
            assert!(
                entry.get(forbidden).is_none(),
                "plain typeDefinition must not include `{forbidden}`: {entry:?}"
            );
        }
    }
}

#[test]
fn test_type_definition_with_new_expression_infers_type_symbol() {
    let mut server = make_server();
    server.open_files.insert(
        "/a.ts".to_string(),
        "class Foo {}\nconst x = new Foo();\nx;\n".to_string(),
    );

    let req = make_request(
        "typeDefinition",
        serde_json::json!({"file": "/a.ts", "line": 3, "offset": 1}),
    );

    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("typeDefinition should return body");
    let entries = body
        .as_array()
        .expect("typeDefinition response should be an array");
    if let Some(entry) = entries.first() {
        assert_eq!(
            entry
                .get("start")
                .and_then(|start| start.get("line"))
                .and_then(serde_json::Value::as_u64),
            Some(1),
            "inferred typeDefinition should resolve to the class declaration: {entry:?}"
        );
    } else {
        panic!("typeDefinition should return inferred Foo declaration");
    }
}

#[test]
fn test_type_definition_full_shape_uses_filename_and_text_span() {
    let mut server = make_server();
    server.open_files.insert(
        "/a.ts".to_string(),
        "interface I { x: number; }\nconst v: I = { x: 1 };\nv;\n".to_string(),
    );

    let req = make_request(
        "typeDefinition-full",
        serde_json::json!({"file": "/a.ts", "line": 3, "offset": 1}),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("typeDefinition-full should return body");
    let entries = body
        .as_array()
        .expect("typeDefinition-full response should be an array");
    if let Some(entry) = entries.first() {
        assert!(
            entry.get("fileName").is_some(),
            "typeDefinition-full should expose fileName: {entry:?}"
        );
        let text_span = entry
            .get("textSpan")
            .expect("typeDefinition-full should expose textSpan");
        assert!(
            text_span
                .get("start")
                .and_then(serde_json::Value::as_u64)
                .is_some()
                && text_span
                    .get("length")
                    .and_then(serde_json::Value::as_u64)
                    .is_some(),
            "textSpan should be numeric: {text_span:?}"
        );
        assert!(
            entry.get("file").is_none()
                && entry.get("start").is_none()
                && entry.get("end").is_none(),
            "typeDefinition-full must not use plain definition fields: {entry:?}"
        );
    }
}

#[test]
fn test_definition_and_bound_span_plain_shape_omits_full_only_fields() {
    let mut server = make_server();
    server.open_files.insert(
        "/a.ts".to_string(),
        "const alpha = 1;\nalpha;\n".to_string(),
    );

    let req = make_request(
        "definitionAndBoundSpan",
        serde_json::json!({"file": "/a.ts", "line": 2, "offset": 1}),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp
        .body
        .expect("definitionAndBoundSpan should return body");
    let definitions = body
        .get("definitions")
        .and_then(serde_json::Value::as_array)
        .expect("expected definitions array");
    let entry = definitions
        .first()
        .expect("expected at least one definition");
    for forbidden in [
        "fileName",
        "textSpan",
        "contextSpan",
        "kind",
        "name",
        "containerName",
        "isLocal",
        "isAmbient",
        "unverified",
        "failedAliasResolution",
    ] {
        assert!(
            entry.get(forbidden).is_none(),
            "definitionAndBoundSpan plain definition must not include `{forbidden}`: {entry:?}"
        );
    }

    // textSpan in the wrapper is the bound span (line/offset shape).
    let text_span = body.get("textSpan").expect("expected textSpan");
    assert!(
        text_span.get("start").is_some() && text_span.get("end").is_some(),
        "plain definitionAndBoundSpan textSpan should use line/offset shape: {text_span:?}"
    );
    assert!(
        text_span.get("length").is_none(),
        "plain definitionAndBoundSpan textSpan must not be numeric: {text_span:?}"
    );
}

#[test]
fn test_definition_and_bound_span_full_uses_numeric_text_span_and_filename() {
    let mut server = make_server();
    server.open_files.insert(
        "/a.ts".to_string(),
        "const alpha = 1;\nalpha;\n".to_string(),
    );

    let req = make_request(
        "definitionAndBoundSpan-full",
        serde_json::json!({"file": "/a.ts", "line": 2, "offset": 1}),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp
        .body
        .expect("definitionAndBoundSpan-full should return body");
    let definitions = body
        .get("definitions")
        .and_then(serde_json::Value::as_array)
        .expect("expected definitions array");
    let entry = definitions
        .first()
        .expect("expected at least one definition");
    assert!(
        entry.get("fileName").is_some(),
        "definitionAndBoundSpan-full inner definition should use fileName: {entry:?}"
    );
    let inner_span = entry
        .get("textSpan")
        .expect("inner definition should have textSpan");
    assert!(
        inner_span
            .get("start")
            .and_then(serde_json::Value::as_u64)
            .is_some()
            && inner_span
                .get("length")
                .and_then(serde_json::Value::as_u64)
                .is_some(),
        "inner textSpan should be numeric: {inner_span:?}"
    );
    assert!(
        entry.get("file").is_none() && entry.get("start").is_none() && entry.get("end").is_none(),
        "definitionAndBoundSpan-full inner definition must not use plain fields: {entry:?}"
    );

    // Wrapper textSpan is also numeric for the -full shape.
    let outer = body.get("textSpan").expect("expected outer textSpan");
    assert!(
        outer
            .get("start")
            .and_then(serde_json::Value::as_u64)
            .is_some()
            && outer
                .get("length")
                .and_then(serde_json::Value::as_u64)
                .is_some(),
        "definitionAndBoundSpan-full outer textSpan should be numeric: {outer:?}"
    );
}

// Issue #3912: navtree-full TextSpans must be in UTF-16 code units, not
// Rust byte offsets. The protocol contract is "TextPosition is a UTF-16
// code-unit offset", and tsserver clients (e.g. VS Code) interpret
// `start` and `length` as UTF-16 indices.
#[test]
fn test_navtree_full_text_spans_use_utf16_units_for_non_ascii_source() {
    let mut server = make_server();
    // The string literal `"é"` is 2 bytes in UTF-8 but 1 UTF-16 code unit.
    // The `é` between `s = "` and `";` is what creates the byte-vs-utf16 gap.
    let source = "const s = \"é\";\nfunction f() {}\n";
    server
        .open_files
        .insert("/utf16.ts".to_string(), source.to_string());

    let req = make_request("navtree-full", serde_json::json!({"file": "/utf16.ts"}));
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("navtree-full should return a body");

    // Root span: length must be UTF-16 units, not byte length.
    let root_span = body
        .get("spans")
        .and_then(serde_json::Value::as_array)
        .and_then(|spans| spans.first())
        .expect("navtree-full root should include a span")
        .clone();
    assert_eq!(
        root_span.get("length").and_then(serde_json::Value::as_u64),
        Some(source.encode_utf16().count() as u64),
        "root length must be UTF-16 units (got byte length?): {root_span:?}"
    );
    assert_ne!(
        root_span.get("length").and_then(serde_json::Value::as_u64),
        Some(source.len() as u64),
        "root length must NOT match the UTF-8 byte length on non-ASCII source: {root_span:?}"
    );

    // Function item: nameSpan start should be the UTF-16 offset of `f`.
    let function_item = body
        .get("childItems")
        .and_then(serde_json::Value::as_array)
        .and_then(|items| {
            items
                .iter()
                .find(|item| item.get("text").and_then(serde_json::Value::as_str) == Some("f"))
        })
        .expect("navtree-full should include function f")
        .clone();
    let name_span = function_item
        .get("nameSpan")
        .expect("function item should include numeric nameSpan");
    let function_name_byte = source
        .find("function f")
        .map(|i| i + "function ".len())
        .expect("expected `function f` in source") as u32;
    let function_name_utf16 = source[..function_name_byte as usize].encode_utf16().count() as u64;
    assert_eq!(
        name_span.get("start").and_then(serde_json::Value::as_u64),
        Some(function_name_utf16),
        "function nameSpan start must be UTF-16 offset of the function-name `f`: {name_span:?}"
    );
    // Sanity: the byte offset and the UTF-16 offset must differ for this
    // source so the test guards against a UTF-8 regression.
    assert_ne!(function_name_utf16, function_name_byte as u64);
}

// Issue #3710: documentHighlights must honor `filesToSearch` and return
// highlight groups for each searched file, not just the request file.
#[test]
fn test_document_highlights_honors_files_to_search_across_files() {
    let mut server = make_server();
    server
        .open_files
        .insert("/a.ts".to_string(), "export const foo = 1;\n".to_string());
    server.open_files.insert(
        "/b.ts".to_string(),
        "import { foo } from \"./a\";\nconsole.log(foo);\n".to_string(),
    );

    // Click on the declaration in /a.ts and ask the server to search BOTH
    // files. tsc returns highlight groups for both /a.ts (declaration) and
    // /b.ts (import specifier + use).
    let req = make_request(
        "documentHighlights",
        serde_json::json!({
            "file": "/a.ts",
            "line": 1,
            "offset": 14,
            "filesToSearch": ["/a.ts", "/b.ts"]
        }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("documentHighlights body");
    let groups = body.as_array().expect("body must be an array");

    let files: std::collections::HashSet<&str> = groups
        .iter()
        .filter_map(|g| g.get("file").and_then(serde_json::Value::as_str))
        .collect();
    assert!(
        files.contains("/a.ts"),
        "expected highlight group for /a.ts, got groups: {groups:?}"
    );
    assert!(
        files.contains("/b.ts"),
        "expected highlight group for /b.ts, got groups: {groups:?}"
    );

    // /b.ts should have at least 2 highlight spans (import specifier + use)
    let b_group = groups
        .iter()
        .find(|g| g.get("file").and_then(serde_json::Value::as_str) == Some("/b.ts"))
        .expect("must have /b.ts group");
    let b_spans = b_group
        .get("highlightSpans")
        .and_then(serde_json::Value::as_array)
        .expect("/b.ts must have highlightSpans");
    assert!(
        b_spans.len() >= 2,
        "expected 2+ highlight spans in /b.ts (import + use), got: {b_spans:?}"
    );
}

/// documentHighlights on an inherited property in a deep linear class chain
/// must not crash. Previously the server ran on the default 8 MB thread stack
/// and SIGABRT'd for deeply-nested ASTs / large inheritance hierarchies.
#[test]
fn test_document_highlights_does_not_overflow_on_deep_linear_chain() {
    let mut server = make_server();
    // 30-level linear inheritance chain: Base <- L1 <- L2 <- … <- L29
    let mut source = String::from("class Base { prop: string; }\n");
    for i in 1..30 {
        source.push_str(&format!("class L{i} extends L{} {{}}\n", i - 1));
    }
    source.push_str("class L0 extends Base {}\n");
    source.push_str("var x: L29; x.prop;\n");
    let file_path = "/tests/cases/fourslash/deep_chain.ts";
    server
        .open_files
        .insert(file_path.to_string(), source.clone());
    let last_line = source.lines().count() as u32;
    let req = make_request(
        "documentHighlights",
        serde_json::json!({
            "file": file_path,
            "line": last_line,
            "offset": 5,
            "filesToSearch": [file_path],
        }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(
        resp.success,
        "documentHighlights must not crash on a deep linear class chain; response: {resp:?}"
    );
}

/// Regression for issue #8527: documentHighlight on a heritage-cycle source
/// must not crash the server (previously SIGABRT'd via stack overflow).
#[test]
fn test_document_highlights_does_not_panic_on_circular_heritage() {
    let mut server = make_server();
    let source = "class C extends D {\n    prop0: string;\n    prop1: string;\n}\n\nclass D extends C {\n    prop0: string;\n    prop1: string;\n}\n\nvar d: D;\nd.prop1;\n";
    let file_path = "/tests/cases/fourslash/file1.ts";
    server
        .open_files
        .insert(file_path.to_string(), source.to_string());

    // Line 12 column 3 = the `prop1` access on `d.prop1` (1-based).
    let req = make_request(
        "documentHighlights",
        serde_json::json!({
            "file": file_path,
            "line": 12,
            "offset": 3,
            "filesToSearch": [file_path],
        }),
    );

    let resp = server.handle_tsserver_request(req);
    assert!(
        resp.success,
        "documentHighlights must return without crashing on circular heritage; response: {resp:?}"
    );
}

/// Regression: `ScopeWalker::collect_references` previously used
/// `stacker::remaining_stack()` inside a `maybe_grow` closure as its depth
/// guard.  Inside a `maybe_grow` closure the remaining-stack probe always
/// returns ~2 MB (the new segment's headroom), so the guard never fired and
/// `stacker` kept chaining new segments until the OS killed the process with
/// SIGABRT.  The fix replaces the probe with an explicit depth counter shared
/// by all three recursive tree-walk functions (`walk_to_node`,
/// `walk_for_scope`, `collect_references`).
///
/// This test constructs a file whose AST nesting is deep enough that the old
/// code would exhaust the stacker budget many times over, while the fixed code
/// should terminate immediately (depth limit trips, returns empty highlights).
#[test]
fn test_document_highlights_depth_guard_prevents_stacker_runaway() {
    let mut server = make_server();
    // Build a deeply-nested function tree (200 levels of immediately-invoked
    // lambdas). The resulting AST has ~200 nesting levels, which is well
    // within the 4096 depth limit but deep enough to exercise the guard path
    // in environments with small per-segment budgets.
    let depth = 200usize;
    let mut source = String::new();
    source.push_str("var target = 0;\n");
    for _ in 0..depth {
        source.push_str("(function() {\n");
    }
    source.push_str("target;\n");
    for _ in 0..depth {
        source.push_str("})();\n");
    }
    let file_path = "/tests/cases/fourslash/deep_nesting.ts";
    server
        .open_files
        .insert(file_path.to_string(), source.clone());
    let req = make_request(
        "documentHighlights",
        serde_json::json!({
            "file": file_path,
            "line": depth as u32 + 1,
            "offset": 1,
            "filesToSearch": [file_path],
        }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(
        resp.success,
        "documentHighlights must not crash on deeply-nested AST; response: {resp:?}"
    );
}

/// Regression for documentHighlightAtInheritedProperties6: the fourslash test
/// calls getDocumentHighlights at each marked position (declarations + usage).
/// Previously only the `d.prop1` usage position was tested; declaration
/// positions (inside class bodies) could also trigger a crash.
#[test]
fn test_document_highlights_at_all_positions_in_circular_heritage() {
    let source = concat!(
        "class C extends D {\n",
        "    prop0: string;\n",
        "    prop1: string;\n",
        "}\n",
        "\n",
        "class D extends C {\n",
        "    prop0: string;\n",
        "    prop1: string;\n",
        "}\n",
        "\n",
        "var d: D;\n",
        "d.prop1;\n",
    );
    let file_path = "/tests/cases/fourslash/file1.ts";
    let mut server = make_server();
    server
        .open_files
        .insert(file_path.to_string(), source.to_string());

    // All marker positions as in the fourslash test (1-based line/offset).
    let positions: &[(&str, u32, u32)] = &[
        ("prop0 in class C decl", 2, 5),
        ("prop1 in class C decl", 3, 5),
        ("prop0 in class D decl", 7, 5),
        ("prop1 in class D decl", 8, 5),
        ("prop1 in d.prop1 usage", 12, 3),
    ];
    for &(desc, line, offset) in positions {
        let req = make_request(
            "documentHighlights",
            serde_json::json!({
                "file": file_path,
                "line": line,
                "offset": offset,
                "filesToSearch": [file_path],
            }),
        );
        let resp = server.handle_tsserver_request(req);
        assert!(
            resp.success,
            "documentHighlights must not crash at {desc} (line {line}, offset {offset}); response: {resp:?}"
        );
    }
}
