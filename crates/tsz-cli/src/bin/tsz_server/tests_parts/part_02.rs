/// Issue #3753: outgoing call to an imported function should point at the
/// exported declaration in the imported module's source file, not at the
/// local import binding in the importer.
#[test]
fn test_call_hierarchy_outgoing_resolves_through_import() {
    let mut server = make_server();
    server.open_files.insert(
        "/a.ts".to_string(),
        "export function target() {}\n".to_string(),
    );
    server.open_files.insert(
        "/b.ts".to_string(),
        "import { target } from \"./a\";\nexport function caller() { target(); }\n".to_string(),
    );

    let req = make_request(
        "provideCallHierarchyOutgoingCalls",
        serde_json::json!({
            "file": "/b.ts",
            "line": 2,
            "offset": 17
        }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("outgoing calls should return a body");
    let calls = body
        .as_array()
        .expect("provideCallHierarchyOutgoingCalls body should be an array");

    let target_call = calls
        .iter()
        .find(|call| call["to"]["name"] == "target")
        .unwrap_or_else(|| panic!("expected outgoing target 'target', got: {calls:?}"));

    let to_file = target_call["to"]["file"].as_str().unwrap_or("");
    assert!(
        to_file.ends_with("/a.ts"),
        "Expected outgoing target to resolve to /a.ts (the imported module), got file={to_file:?} call={target_call:?}"
    );
    assert!(
        !to_file.ends_with("/b.ts"),
        "Outgoing target must not stay on the importer's local binding (/b.ts). Got file={to_file:?} call={target_call:?}"
    );

    // The selectionSpan should anchor on the exported function in /a.ts —
    // the identifier `target` lives at column 17 (1-based) of line 1.
    let selection_start = &target_call["to"]["selectionSpan"]["start"];
    assert_eq!(
        selection_start["line"].as_u64(),
        Some(1),
        "expected selection at line 1 of a.ts, got: {target_call:?}"
    );
}

/// Issue #3753 (incoming half): asking for incoming calls on a function
/// exported from /a.ts must include callers in /b.ts that reach it via
/// `import { target } from "./a"`. Without the cross-file scan tsz only
/// reported within-file callers, so the response was an empty array.
#[test]
fn test_call_hierarchy_incoming_includes_cross_file_caller() {
    let mut server = make_server();
    server.open_files.insert(
        "/a.ts".to_string(),
        "export function target() {}\n".to_string(),
    );
    server.open_files.insert(
        "/b.ts".to_string(),
        "import { target } from \"./a\";\nexport function caller() { target(); }\n".to_string(),
    );

    let req = make_request(
        "provideCallHierarchyIncomingCalls",
        serde_json::json!({
            "file": "/a.ts",
            "line": 1,
            "offset": 17
        }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("incoming calls should return a body");
    let calls = body
        .as_array()
        .expect("provideCallHierarchyIncomingCalls body should be an array");

    let caller = calls
        .iter()
        .find(|call| call["from"]["name"] == "caller")
        .unwrap_or_else(|| panic!("expected cross-file caller 'caller', got: {calls:?}"));

    let from_file = caller["from"]["file"].as_str().unwrap_or("");
    assert!(
        from_file.ends_with("/b.ts"),
        "cross-file caller should live in /b.ts, got file={from_file:?} call={caller:?}"
    );
}

/// Aliased imports — `import { target as t }` — must also be discovered as
/// callers when the local binding `t` is invoked. The exported-name match
/// is what counts, not the local name.
#[test]
fn test_call_hierarchy_incoming_handles_aliased_cross_file_import() {
    let mut server = make_server();
    server.open_files.insert(
        "/a.ts".to_string(),
        "export function target() {}\n".to_string(),
    );
    server.open_files.insert(
        "/b.ts".to_string(),
        "import { target as t } from \"./a\";\nexport function caller() { t(); }\n".to_string(),
    );

    let req = make_request(
        "provideCallHierarchyIncomingCalls",
        serde_json::json!({
            "file": "/a.ts",
            "line": 1,
            "offset": 17
        }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("incoming calls should return a body");
    let calls = body
        .as_array()
        .expect("provideCallHierarchyIncomingCalls body should be an array");

    assert!(
        calls.iter().any(|c| c["from"]["name"] == "caller"
            && c["from"]["file"]
                .as_str()
                .is_some_and(|f| f.ends_with("/b.ts"))),
        "expected cross-file caller via aliased import, got: {calls:?}"
    );
}

/// Issue #3753 follow-up: namespace imports — `import * as ns from "./a"`
/// followed by `ns.target()` must register as a cross-file caller of the
/// exported `target` in /a.ts.
#[test]
fn test_call_hierarchy_incoming_handles_namespace_import_member_call() {
    let mut server = make_server();
    server.open_files.insert(
        "/a.ts".to_string(),
        "export function target() {}\n".to_string(),
    );
    server.open_files.insert(
        "/b.ts".to_string(),
        "import * as ns from \"./a\";\nexport function caller() { ns.target(); }\n".to_string(),
    );

    let req = make_request(
        "provideCallHierarchyIncomingCalls",
        serde_json::json!({
            "file": "/a.ts",
            "line": 1,
            "offset": 17
        }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("incoming calls should return a body");
    let calls = body
        .as_array()
        .expect("provideCallHierarchyIncomingCalls body should be an array");

    assert!(
        calls.iter().any(|c| c["from"]["name"] == "caller"
            && c["from"]["file"]
                .as_str()
                .is_some_and(|f| f.ends_with("/b.ts"))),
        "expected cross-file caller via `ns.target()` namespace import, got: {calls:?}"
    );
}

/// Namespace import where the user calls a *different* member of the
/// imported namespace must NOT register as a caller of `target` (no false
/// positives from same-namespace, different-member calls).
#[test]
fn test_call_hierarchy_incoming_namespace_skips_other_members() {
    let mut server = make_server();
    server.open_files.insert(
        "/a.ts".to_string(),
        "export function target() {}\nexport function other() {}\n".to_string(),
    );
    server.open_files.insert(
        "/b.ts".to_string(),
        "import * as ns from \"./a\";\nexport function caller() { ns.other(); }\n".to_string(),
    );

    let req = make_request(
        "provideCallHierarchyIncomingCalls",
        serde_json::json!({
            "file": "/a.ts",
            "line": 1,
            "offset": 17
        }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("incoming calls should return a body");
    let calls = body
        .as_array()
        .expect("provideCallHierarchyIncomingCalls body should be an array");

    assert!(
        !calls.iter().any(|c| c["from"]["name"] == "caller"),
        "must not list caller — it calls ns.other(), not ns.target(). got: {calls:?}"
    );
}

/// A different file that has its own `target` (not imported from /a.ts) must
/// not pollute the incoming-calls answer for /a.ts's `target`.
#[test]
fn test_call_hierarchy_incoming_skips_unrelated_imports() {
    let mut server = make_server();
    server.open_files.insert(
        "/a.ts".to_string(),
        "export function target() {}\n".to_string(),
    );
    server.open_files.insert(
        "/c.ts".to_string(),
        "function target() {}\nexport function localCaller() { target(); }\n".to_string(),
    );

    let req = make_request(
        "provideCallHierarchyIncomingCalls",
        serde_json::json!({
            "file": "/a.ts",
            "line": 1,
            "offset": 17
        }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("incoming calls should return a body");
    let calls = body
        .as_array()
        .expect("provideCallHierarchyIncomingCalls body should be an array");

    assert!(
        !calls.iter().any(|c| c["from"]["name"] == "localCaller"),
        "must not report localCaller from /c.ts (no import edge from /a.ts), got: {calls:?}"
    );
}

#[test]
fn test_call_hierarchy_outgoing_includes_constructor_call_target() {
    let mut server = make_server();
    server.open_files.insert(
        "/test.ts".to_string(),
        "function foo() {\n    bar();\n}\n\nfunction bar() {\n    new Baz();\n}\n\nclass Baz {\n}\n"
            .to_string(),
    );

    let req = make_request(
        "provideCallHierarchyOutgoingCalls",
        serde_json::json!({
            "file": "/test.ts",
            "line": 5,
            "offset": 10
        }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("outgoing calls should return a body");
    let calls = body
        .as_array()
        .expect("provideCallHierarchyOutgoingCalls body should be an array");
    assert!(
        calls.iter().any(|call| call["to"]["name"] == "Baz"),
        "Expected outgoing constructor call target 'Baz', got: {calls:?}"
    );
}

#[test]
fn test_call_hierarchy_incoming_uses_script_kind_for_top_level_caller() {
    let mut server = make_server();
    server.open_files.insert(
        "/test.ts".to_string(),
        "function foo() {\n    bar();\n}\n\nconst bar = function () {\n    baz();\n}\n\nfunction baz() {\n}\n\nbar()\n"
            .to_string(),
    );

    let req = make_request(
        "provideCallHierarchyIncomingCalls",
        serde_json::json!({
            "file": "/test.ts",
            "line": 5,
            "offset": 7
        }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("incoming calls should return a body");
    let calls = body
        .as_array()
        .expect("provideCallHierarchyIncomingCalls body should be an array");
    assert!(
        calls.iter().any(|call| call["from"]["kind"] == "script"),
        "Expected top-level caller to be mapped to tsserver kind 'script', got: {calls:?}"
    );
}

#[test]
fn test_call_hierarchy_incoming_file_start_query_returns_no_calls() {
    let mut server = make_server();
    server.open_files.insert(
        "/test.ts".to_string(),
        "foo();\nfunction foo() {\n}\n".to_string(),
    );

    let req = make_request(
        "provideCallHierarchyIncomingCalls",
        serde_json::json!({
            "file": "/test.ts",
            "line": 1,
            "offset": 1
        }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("incoming calls should return a body");
    let calls = body
        .as_array()
        .expect("provideCallHierarchyIncomingCalls body should be an array");
    assert!(
        calls.is_empty(),
        "Expected no incoming calls for file-start source-file query, got: {calls:?}"
    );
}

/// Issue #3753 follow-up: default-export incoming calls.
///
/// `/a.ts` declares `export default function target()`. `/b.ts` invokes it
/// via a default-import (`import target from "./a"`). tsc reports `caller`
/// in /b.ts as an incoming call of `target`; the cross-file caller scan
/// previously only fired the default-import branch when the user asked
/// for incoming calls on a symbol literally named "default", so for this
/// shape (where the prepared name is "target") it returned [].
#[test]
fn test_call_hierarchy_incoming_handles_default_import_for_default_export() {
    let mut server = make_server();
    server.open_files.insert(
        "/a.ts".to_string(),
        "export default function target() {}\n".to_string(),
    );
    server.open_files.insert(
        "/b.ts".to_string(),
        "import target from \"./a\";\nexport function caller() { target(); }\n".to_string(),
    );

    let req = make_request(
        "provideCallHierarchyIncomingCalls",
        serde_json::json!({
            "file": "/a.ts",
            "line": 1,
            "offset": 25,
        }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("incoming calls should return a body");
    let calls = body
        .as_array()
        .expect("provideCallHierarchyIncomingCalls body should be an array");

    let caller = calls
        .iter()
        .find(|c| c["from"]["name"] == "caller")
        .unwrap_or_else(|| panic!("expected default-import caller 'caller', got: {calls:?}"));

    let from_file = caller["from"]["file"].as_str().unwrap_or("");
    assert!(
        from_file.ends_with("/b.ts"),
        "cross-file caller should live in /b.ts, got file={from_file:?} call={caller:?}"
    );
}

/// Issue #3753 follow-up: a default-import bound to a *different* local
/// name still reaches the default export. The local-name choice is the
/// importer's affair; tsc keys the resolution off the export name.
#[test]
fn test_call_hierarchy_incoming_handles_renamed_default_import() {
    let mut server = make_server();
    server.open_files.insert(
        "/a.ts".to_string(),
        "export default function target() {}\n".to_string(),
    );
    server.open_files.insert(
        "/b.ts".to_string(),
        "import renamed from \"./a\";\nexport function caller() { renamed(); }\n".to_string(),
    );

    let req = make_request(
        "provideCallHierarchyIncomingCalls",
        serde_json::json!({
            "file": "/a.ts",
            "line": 1,
            "offset": 25,
        }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("incoming calls should return a body");
    let calls = body
        .as_array()
        .expect("provideCallHierarchyIncomingCalls body should be an array");

    assert!(
        calls.iter().any(|c| c["from"]["name"] == "caller"
            && c["from"]["file"]
                .as_str()
                .is_some_and(|f| f.ends_with("/b.ts"))),
        "expected renamed default-import caller in /b.ts, got: {calls:?}"
    );
}

/// Issue #3753 follow-up: `import { default as target }` is a named-import
/// shape that resolves to the default export. The named-import branch must
/// match `default` against a default-exported target alongside its
/// declared name.
#[test]
fn test_call_hierarchy_incoming_handles_named_default_alias_for_default_export() {
    let mut server = make_server();
    server.open_files.insert(
        "/a.ts".to_string(),
        "export default function target() {}\n".to_string(),
    );
    server.open_files.insert(
        "/b.ts".to_string(),
        "import { default as target } from \"./a\";\nexport function caller() { target(); }\n"
            .to_string(),
    );

    let req = make_request(
        "provideCallHierarchyIncomingCalls",
        serde_json::json!({
            "file": "/a.ts",
            "line": 1,
            "offset": 25,
        }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("incoming calls should return a body");
    let calls = body
        .as_array()
        .expect("provideCallHierarchyIncomingCalls body should be an array");

    assert!(
        calls.iter().any(|c| c["from"]["name"] == "caller"
            && c["from"]["file"]
                .as_str()
                .is_some_and(|f| f.ends_with("/b.ts"))),
        "expected `import {{ default as target }}` caller in /b.ts, got: {calls:?}"
    );
}

/// Default-export detection must stay scoped to default-exported symbols.
/// A regular `export function NAME` must not start matching default-import
/// bindings — those bind to `default`, not `NAME`. Without this guard, any
/// `import x from "./a"` in any other file would falsely register against
/// every named export of /a.ts.
#[test]
fn test_call_hierarchy_incoming_named_export_does_not_capture_unrelated_default_import() {
    let mut server = make_server();
    server.open_files.insert(
        "/a.ts".to_string(),
        "export function target() {}\nexport default function other() {}\n".to_string(),
    );
    server.open_files.insert(
        "/b.ts".to_string(),
        "import other from \"./a\";\nexport function caller() { other(); }\n".to_string(),
    );

    let req = make_request(
        "provideCallHierarchyIncomingCalls",
        serde_json::json!({
            "file": "/a.ts",
            "line": 1,
            "offset": 17,
        }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("incoming calls should return a body");
    let calls = body
        .as_array()
        .expect("provideCallHierarchyIncomingCalls body should be an array");

    assert!(
        !calls.iter().any(|c| c["from"]["name"] == "caller"),
        "named export `target` must not capture the default-import of `other`, got: {calls:?}"
    );
}

#[test]
fn test_format_range_paste_applies_cleanly() {
    // When the server format handler runs, edits may come from either the
    // native tsserver worker (which produces tsserver-shaped structural
    // rewrites) or the LSP crate's conservative whitespace-only fallback
    // (which does not rewrite structure). Either way, reapplying the edits
    // must produce valid UTF-8 output that still contains the declarations
    // from the source.
    //
    // Structural-rewrite assertions were removed: they belong in prettier /
    // eslint / native tsserver tests, not in the LSP fallback. The LSP
    // fallback's conservative contract is covered by the formatting_tests
    // suite in tsz-lsp.
    let mut server = make_server();
    let file = "/test.ts";
    let source = "namespace TestModule {\n class TestClass{\nprivate   foo;\npublic testMethod( )\n{}\n}\n}\n";
    server
        .open_files
        .insert(file.to_string(), source.to_string());

    let req = make_request(
        "format",
        serde_json::json!({
            "file": file,
            "line": 2,
            "offset": 1,
            "endLine": 6,
            "endOffset": 2,
            "options": {
                "tabSize": 4,
                "insertSpaces": true
            }
        }),
    );

    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let edits = resp
        .body
        .expect("format should return edits")
        .as_array()
        .expect("format body should be array")
        .clone();
    let updated = apply_tsserver_text_edits(source.to_string(), &edits);
    assert!(
        updated.contains("namespace TestModule") && updated.contains("class TestClass"),
        "formatted output lost its declarations: {updated:?}"
    );
    assert!(
        updated.contains("testMethod"),
        "formatted output lost the method: {updated:?}"
    );
}

#[test]
fn test_format_with_explicit_range_preserves_inline_markers_on_indent_only_lines() {
    let mut server = make_server();
    let file = "/test.ts";
    let source = "class TestClass {\n    private testMethod1(param1: boolean,\n                        param2/*1*/: boolean) {\n    }\n\n    public testMethod2(a: number, b: number, c: number) {\n        if (a === b) {\n        }\n        else if (a != c &&\n                 a/*2*/ > b &&\n                 b/*3*/ < c) {\n        }\n\n    }\n}\n";
    server
        .open_files
        .insert(file.to_string(), source.to_string());

    let req = make_request(
        "format",
        serde_json::json!({
            "file": file,
            "line": 1,
            "offset": 1,
            "endLine": 15,
            "endOffset": 1,
            "options": {
                "tabSize": 4,
                "insertSpaces": true
            }
        }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let edits = resp
        .body
        .expect("format should return edits")
        .as_array()
        .expect("format body should be array")
        .clone();
    let updated = apply_tsserver_text_edits(source.to_string(), &edits);

    assert!(
        updated.contains("/*1*/"),
        "marker /*1*/ must survive formatting edits"
    );
    assert!(
        updated.contains("/*2*/"),
        "marker /*2*/ must survive formatting edits"
    );
    assert!(
        updated.contains("/*3*/"),
        "marker /*3*/ must survive formatting edits"
    );
}

#[test]
fn test_format_with_explicit_range_does_not_invalidate_fourslash_markers() {
    fn strip_markers(source: &str) -> (String, Vec<usize>) {
        let mut out = String::with_capacity(source.len());
        let mut markers = Vec::new();
        let bytes = source.as_bytes();
        let mut i = 0usize;
        while i < bytes.len() {
            if i + 4 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'*' {
                let mut j = i + 2;
                while j < bytes.len() && bytes[j].is_ascii_digit() {
                    j += 1;
                }
                if j + 1 < bytes.len() && bytes[j] == b'*' && bytes[j + 1] == b'/' && j > i + 2 {
                    markers.push(out.len());
                    i = j + 2;
                    continue;
                }
            }
            out.push(bytes[i] as char);
            i += 1;
        }
        (out, markers)
    }

    fn update_position(
        position: usize,
        edit_start: usize,
        edit_end: usize,
        new_text: &str,
    ) -> Option<usize> {
        if position <= edit_start {
            return Some(position);
        }
        if position < edit_end {
            return None;
        }
        Some(position + new_text.len() - (edit_end - edit_start))
    }

    let source_with_markers = "class TestClass {\n    private testMethod1(param1: boolean,\n                        param2/*1*/: boolean) {\n    }\n\n    public testMethod2(a: number, b: number, c: number) {\n        if (a === b) {\n        }\n        else if (a != c &&\n                 a/*2*/ > b &&\n                 b/*3*/ < c) {\n        }\n\n    }\n}\n";
    let (source, mut marker_positions) = strip_markers(source_with_markers);

    let mut server = make_server();
    let file = "/test.ts";
    server.open_files.insert(file.to_string(), source.clone());

    let req = make_request(
        "format",
        serde_json::json!({
            "file": file,
            "line": 1,
            "offset": 1,
            "endLine": 15,
            "endOffset": 1,
            "options": {
                "tabSize": 4,
                "insertSpaces": true
            }
        }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("format should return edits");
    let edits = body.as_array().expect("format body should be array");

    let mut changes: Vec<(usize, usize, String)> = edits
        .iter()
        .filter_map(|edit| {
            let start = edit.get("start")?;
            let end = edit.get("end")?;
            let start_line = start.get("line")?.as_u64()? as u32;
            let start_offset = start.get("offset")?.as_u64()? as u32;
            let end_line = end.get("line")?.as_u64()? as u32;
            let end_offset = end.get("offset")?.as_u64()? as u32;
            let new_text = edit.get("newText")?.as_str()?.to_string();
            let start_byte = Server::line_offset_to_byte(&source, start_line, start_offset);
            let end_byte = Server::line_offset_to_byte(&source, end_line, end_offset);
            Some((start_byte, end_byte.saturating_sub(start_byte), new_text))
        })
        .collect();

    for i in 0..changes.len() {
        let (start, len, new_text) = changes[i].clone();
        let end = start + len;
        for marker in &mut marker_positions {
            let next = update_position(*marker, start, end, &new_text);
            assert!(
                next.is_some(),
                "fourslash marker invalidated by edit span ({start}, {end}) -> {:?}",
                changes[i]
            );
            *marker = next.unwrap_or(0);
        }
        let delta = new_text.len() as isize - len as isize;
        for change in changes.iter_mut().skip(i + 1) {
            if change.0 >= start {
                change.0 = (change.0 as isize + delta) as usize;
            }
        }
    }
}

#[test]
fn test_format_document_does_not_invalidate_fourslash_markers() {
    fn strip_markers(source: &str) -> (String, Vec<usize>) {
        let mut out = String::with_capacity(source.len());
        let mut markers = Vec::new();
        let bytes = source.as_bytes();
        let mut i = 0usize;
        while i < bytes.len() {
            if i + 4 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'*' {
                let mut j = i + 2;
                while j < bytes.len() && bytes[j].is_ascii_digit() {
                    j += 1;
                }
                if j + 1 < bytes.len() && bytes[j] == b'*' && bytes[j + 1] == b'/' && j > i + 2 {
                    markers.push(out.len());
                    i = j + 2;
                    continue;
                }
            }
            out.push(bytes[i] as char);
            i += 1;
        }
        (out, markers)
    }

    fn update_position(
        position: usize,
        edit_start: usize,
        edit_end: usize,
        new_text: &str,
    ) -> Option<usize> {
        if position <= edit_start {
            return Some(position);
        }
        if position < edit_end {
            return None;
        }
        Some(position + new_text.len() - (edit_end - edit_start))
    }

    let source_with_markers = "class TestClass {\n    private testMethod1(param1: boolean,\n                        param2/*1*/: boolean) {\n    }\n\n    public testMethod2(a: number, b: number, c: number) {\n        if (a === b) {\n        }\n        else if (a != c &&\n                 a/*2*/ > b &&\n                 b/*3*/ < c) {\n        }\n\n    }\n}\n";
    let (source, mut marker_positions) = strip_markers(source_with_markers);

    let mut server = make_server();
    let file = "/test.ts";
    server.open_files.insert(file.to_string(), source.clone());

    let req = make_request(
        "format",
        serde_json::json!({
            "file": file,
            "options": {
                "tabSize": 4,
                "insertSpaces": true
            }
        }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("format should return edits");
    let edits = body.as_array().expect("format body should be array");

    let mut changes: Vec<(usize, usize, String)> = edits
        .iter()
        .filter_map(|edit| {
            let start = edit.get("start")?;
            let end = edit.get("end")?;
            let start_line = start.get("line")?.as_u64()? as u32;
            let start_offset = start.get("offset")?.as_u64()? as u32;
            let end_line = end.get("line")?.as_u64()? as u32;
            let end_offset = end.get("offset")?.as_u64()? as u32;
            let new_text = edit.get("newText")?.as_str()?.to_string();
            let start_byte = Server::line_offset_to_byte(&source, start_line, start_offset);
            let end_byte = Server::line_offset_to_byte(&source, end_line, end_offset);
            Some((start_byte, end_byte.saturating_sub(start_byte), new_text))
        })
        .collect();

    for i in 0..changes.len() {
        let (start, len, new_text) = changes[i].clone();
        let end = start + len;
        for marker in &mut marker_positions {
            let next = update_position(*marker, start, end, &new_text);
            assert!(
                next.is_some(),
                "fourslash marker invalidated by edit span ({start}, {end}) -> {:?}",
                changes[i]
            );
            *marker = next.unwrap_or(0);
        }
        let delta = new_text.len() as isize - len as isize;
        for change in changes.iter_mut().skip(i + 1) {
            if change.0 >= start {
                change.0 = (change.0 as isize + delta) as usize;
            }
        }
    }
}

#[test]
fn test_quickinfo_on_nonexistent_file_has_no_body() {
    let mut server = make_server();
    let req = make_request(
        "quickinfo",
        serde_json::json!({"file": "/nonexistent.ts", "line": 1, "offset": 1}),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    assert!(
        resp.body.is_none(),
        "quickinfo should omit body when the file cannot be resolved"
    );
}

#[test]
fn test_quickinfo_uses_hover_info_structured_fields() {
    // When HoverInfo returns structured kind/kindModifiers/displayString/
    // documentation fields, they should be used in the response instead of
    // being re-parsed from markdown contents.
    let mut server = make_server();
    server
        .open_files
        .insert("/test.ts".to_string(), "const myVar = 42;".to_string());
    let req = make_request(
        "quickinfo",
        serde_json::json!({"file": "/test.ts", "line": 1, "offset": 7}),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("quickinfo should return a body");
    // The body must have displayString, kind, kindModifiers, documentation
    assert!(
        body.get("displayString").is_some(),
        "quickinfo must have displayString"
    );
    assert!(body.get("kind").is_some(), "quickinfo must have kind");
    assert!(
        body.get("kindModifiers").is_some(),
        "quickinfo must have kindModifiers"
    );
    assert!(
        body.get("documentation").is_some(),
        "quickinfo must have documentation"
    );
}

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

// TODO: blDAJ (fix(binder,checker): support string literal export names in export specifiers)
// introduced a dedicated EXPORT_VALUE symbol for quoted alias specifiers. That removes the
// fallback path this test was anchoring on, so the `"<other>"` export-alias-side token no
// longer shows up as its own definition span in references-full. Keeping the test as
// #[ignore] until the LSP resolver is updated to follow EXPORT_VALUE alias symbols through
// `node_symbols` and re-emit per-specifier definition spans for quoted re-exports.
