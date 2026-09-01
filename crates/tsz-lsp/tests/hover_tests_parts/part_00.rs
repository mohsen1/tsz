fn hover_markdown_section(info: &HoverInfo) -> String {
    info.contents
        .iter()
        .find(|c: &&String| !c.starts_with("```typescript"))
        .cloned()
        .unwrap_or_default()
}

#[test]
fn test_hover_jsdoc_summary_escapes_markdown_delimiters() {
    // A user-supplied summary contains characters that would otherwise be
    // interpreted as Markdown link syntax. The hover Markdown content must
    // backslash-escape them so the renderer treats them as literal text.
    let source = "/** See foo[0] for [an example](http://e) */\nfunction g() {}\ng();";
    let info = get_hover_at(source, 2, 0).expect("Should find hover info");
    let md = hover_markdown_section(&info);
    assert!(
        md.contains("foo\\[0\\]"),
        "Bracket literals must be escaped in Markdown hover; got: {md:?}"
    );
    assert!(
        md.contains("\\[an example\\]\\(http://e\\)"),
        "Link-shaped literals must be inert in hover; got: {md:?}"
    );
    // The plain documentation field is rendered as text, not Markdown, so it
    // must keep the original characters and not pick up backslashes.
    assert!(
        info.documentation.contains("foo[0]"),
        "Plain documentation must keep raw text; got: {:?}",
        info.documentation
    );
    assert!(
        !info.documentation.contains("\\["),
        "Plain documentation must not contain Markdown escapes; got: {:?}",
        info.documentation
    );
}

#[test]
fn test_hover_jsdoc_param_description_escapes_markdown_delimiters() {
    let source = "/**\n * @param a See [docs](http://e) for details\n */\nfunction g(a: number) { return a; }\ng(1);";
    let info = get_hover_at(source, 4, 0).expect("Should find hover info");
    let md = hover_markdown_section(&info);
    assert!(
        md.contains("`a` See \\[docs\\]\\(http://e\\) for details"),
        "Parameter description must be Markdown-escaped; got: {md:?}"
    );
}

#[test]
fn test_hover_jsdoc_param_name_with_backtick_uses_safe_fence() {
    // A parameter description containing a literal backtick must not be
    // emitted as a single-fenced inline span — that would terminate the
    // span at the first inner backtick. The renderer chooses a longer
    // fence per CommonMark §6.1.
    let source =
        "/**\n * @param a a `with` backtick\n */\nfunction g(a: number) { return a; }\ng(1);";
    let info = get_hover_at(source, 4, 0).expect("Should find hover info");
    let md = hover_markdown_section(&info);
    // Description's literal backticks are escaped by the prose path so the
    // Markdown renderer shows them verbatim instead of starting an inline
    // code span.
    assert!(
        md.contains("`a` a \\`with\\` backtick"),
        "Inline backticks in @param description must be escaped; got: {md:?}"
    );
}

#[test]
fn test_hover_jsdoc_returns_escapes_markdown_delimiters() {
    let source = "/**\n * @returns the result [0..n]\n */\nfunction g() { return 0; }\ng();";
    let info = get_hover_at(source, 4, 0).expect("Should find hover info");
    let md = hover_markdown_section(&info);
    assert!(
        md.contains("Returns: the result \\[0..n\\]"),
        "@returns text must be Markdown-escaped; got: {md:?}"
    );
}

#[test]
fn test_hover_jsdoc_example_uses_longer_fence_when_content_has_triple_backtick() {
    // A code block whose body contains its own triple-backtick fence must be
    // emitted with a longer outer fence so the renderer cannot terminate the
    // block early. Avoid embedding three backticks directly in the Rust
    // literal — write the JSDoc literal as concatenated string pieces.
    let triple = "```";
    let source = format!(
        "/**\n * @example {triple}js\n *   foo();\n * {triple}\n */\nfunction g() {{}}\ng();"
    );
    let info = get_hover_at(&source, 6, 0).expect("Should find hover info");
    let md = hover_markdown_section(&info);
    let four = "````";
    assert!(
        md.contains(four),
        "Example fence must outlast the inner ``` fence; got: {md:?}"
    );
}

#[test]
fn test_hover_jsdoc_summary_rule_is_name_independent() {
    // Renaming the alphanumeric portion of a summary must not change whether
    // delimiter characters are escaped. The rule is structural (§25).
    let make = |label: &str| format!("/** See {label}[0] */\nfunction g() {{}}\ng();");
    for label in ["foo", "bar", "myThing", "X"] {
        let source = make(label);
        let info = get_hover_at(&source, 2, 0).expect("Should find hover info");
        let md = hover_markdown_section(&info);
        let expected = format!("See {label}\\[0\\]");
        assert!(
            md.contains(&expected),
            "Bracket escape must apply regardless of identifier {label:?}; got: {md:?}"
        );
    }
}

// ── inline @link expansion ────────────────────────────────────────────────────

#[test]
fn test_hover_jsdoc_link_in_summary_becomes_inline_code() {
    // When a JSDoc summary contains {@link X}, the hover Markdown must render
    // it as `X` (inline code), not as the raw JSDoc {@link X} construct.
    let source = "/** Use {@link Helper} to process. */\nfunction f() {}\nf();";
    let info = get_hover_at(source, 2, 0).expect("Should find hover info");
    let md = hover_markdown_section(&info);
    assert!(
        md.contains("`Helper`"),
        "Link-to-symbol must render as inline code in Markdown hover; got: {md:?}"
    );
    assert!(
        !md.contains("{@link"),
        "Raw {{@link}} syntax must not appear in Markdown hover; got: {md:?}"
    );
}

#[test]
fn test_hover_jsdoc_link_in_summary_resolves_to_declaration_uri() {
    for name in ["Helper", "WidgetX"] {
        let source = format!(
            "class {name} {{}}\n/** Use {{@link {name}}} to process. */\nfunction f() {{}}\nf();"
        );
        let info = get_hover_at(&source, 3, 0).expect("Should find hover info");
        let md = hover_markdown_section(&info);
        let expected = format!("[{name}](file://test.ts#L1,1)");
        assert!(
            md.contains(&expected),
            "Resolved JSDoc link must point at declaration for {name:?}; got: {md:?}"
        );
    }
}

#[test]
fn test_hover_jsdoc_link_in_param_resolves_to_declaration_uri() {
    let source = "class Helper {}\n/**\n * @param value See {@link Helper}.\n */\nfunction f(value: number) {}\nf(1);";
    let info = get_hover_at(source, 5, 0).expect("Should find hover info");
    let md = hover_markdown_section(&info);
    assert!(
        md.contains("`value` See [Helper](file://test.ts#L1,1)."),
        "Param JSDoc link must resolve through the hover formatter; got: {md:?}"
    );
}

#[test]
fn test_hover_jsdoc_linkcode_resolves_with_code_label() {
    let source = "function helper() {}\n/** Call {@linkcode helper}. */\nfunction f() {}\nf();";
    let info = get_hover_at(source, 3, 0).expect("Should find hover info");
    let md = hover_markdown_section(&info);
    assert!(
        md.contains("[`helper`](file://test.ts#L1,1)"),
        "Resolved @linkcode must keep code voice inside the Markdown link; got: {md:?}"
    );
}

#[test]
fn test_hover_jsdoc_link_plain_text_strips_syntax() {
    // The plain-text documentation field (used in quickinfo protocol) must
    // expand {@link X} to just X, with no JSDoc syntax.
    let source = "/** Use {@link Helper} to process. */\nfunction f() {}\nf();";
    let info = get_hover_at(source, 2, 0).expect("Should find hover info");
    assert!(
        info.documentation.contains("Helper"),
        "Symbol name must appear in plain documentation; got: {:?}",
        info.documentation
    );
    assert!(
        !info.documentation.contains("{@link"),
        "Raw {{@link}} must not appear in plain documentation; got: {:?}",
        info.documentation
    );
}

#[test]
fn test_hover_jsdoc_link_with_display_text_uses_display() {
    // {@link X the label} should show "the label", not "X".
    let source = "/** See {@link Helper the handler} for usage. */\nfunction f() {}\nf();";
    let info = get_hover_at(source, 2, 0).expect("Should find hover info");
    let md = hover_markdown_section(&info);
    assert!(
        md.contains("the handler") || md.contains("`the handler`"),
        "Display text must be shown for @link with display; got: {md:?}"
    );
    assert!(
        !md.contains("{@link"),
        "Raw {{@link}} must not appear in Markdown; got: {md:?}"
    );
}

#[test]
fn test_hover_jsdoc_linkcode_renders_as_inline_code() {
    let source = "/** Call {@linkcode process} here. */\nfunction f() {}\nf();";
    let info = get_hover_at(source, 2, 0).expect("Should find hover info");
    let md = hover_markdown_section(&info);
    assert!(
        md.contains("`process`"),
        "{{@linkcode}} must render as inline code; got: {md:?}"
    );
}

#[test]
fn test_hover_jsdoc_linkplain_renders_as_plain_text() {
    let source = "/** See {@linkplain MyType} for details. */\nfunction f() {}\nf();";
    let info = get_hover_at(source, 2, 0).expect("Should find hover info");
    let md = hover_markdown_section(&info);
    // linkplain must render as plain text, not code-formatted.
    assert!(
        md.contains("MyType") && !md.contains("`MyType`"),
        "{{@linkplain}} must render as plain text, not code; got: {md:?}"
    );
}

#[test]
fn test_hover_jsdoc_link_url_renders_as_hyperlink() {
    let source = "/** See {@link https://example.com} for more. */\nfunction f() {}\nf();";
    let info = get_hover_at(source, 2, 0).expect("Should find hover info");
    let md = hover_markdown_section(&info);
    assert!(
        md.contains("[https://example.com](https://example.com)"),
        "URL @link must render as Markdown hyperlink; got: {md:?}"
    );
}

#[test]
fn test_hover_jsdoc_link_is_name_independent() {
    // The expansion rule applies to any symbol name, not just specific spellings.
    for name in ["K", "MyClass", "X", "someFunc", "NS.Method"] {
        let source = format!("/** Use {{@link {name}}} here. */\nfunction f() {{}}\nf();");
        let info = get_hover_at(&source, 2, 0).expect("Should find hover info");
        let md = hover_markdown_section(&info);
        let expected_code = format!("`{name}`");
        assert!(
            md.contains(&expected_code),
            "Link expansion must work for any name {name:?}; got: {md:?}"
        );
        assert!(
            !md.contains("{@link"),
            "Raw {{@link}} must not appear regardless of name; got: {md:?}"
        );
    }
}

#[test]
fn test_hover_enum_member_value_variable_keeps_member_type() {
    // const e = E.A  =>  tsserver: `const e: E.A` (variable type, not value)
    let source = "enum E { A, B, C }\nconst e = E.A;\ne;";
    let info = get_hover_at(source, 2, 0).expect("Should find hover info for e");
    assert_eq!(info.display_string, "const e: E.A");
}

#[test]
fn test_hover_enum_member_access_shows_constant_value() {
    // hover on the `.A` member access  =>  tsserver: `(enum member) E.A = 0`
    let source = "enum E { A, B, C }\nconst e = E.A;\ne;";
    let info = get_hover_at(source, 1, 12).expect("Should find hover info for member A");
    assert_eq!(info.display_string, "(enum member) E.A = 0");
}

#[test]
fn test_hover_enum_member_access_shows_later_member_value() {
    // Auto-incremented members carry their own constant value.
    let source = "enum E { A, B, C }\nconst e = E.C;\ne;";
    let info = get_hover_at(source, 1, 12).expect("Should find hover info for member C");
    assert_eq!(info.display_string, "(enum member) E.C = 2");
}

#[test]
fn test_hover_string_enum_member_access_shows_string_value() {
    // String enum members show the quoted string value.
    let source = "enum F { X = \"x\", Y = \"y\" }\nconst f = F.X;\nf;";
    let info = get_hover_at(source, 1, 12).expect("Should find hover info for member X");
    assert_eq!(info.display_string, "(enum member) F.X = \"x\"");
}

#[test]
fn test_hover_explicit_numeric_enum_member_value() {
    let source = "enum E { A = 5, B = 10 }\nconst e = E.B;\ne;";
    let info = get_hover_at(source, 1, 12).expect("Should find hover info for member B");
    assert_eq!(info.display_string, "(enum member) E.B = 10");
}

#[test]
fn test_hover_const_enum_member_access_shows_constant_value() {
    let source = "const enum E { A, B, C }\nconst e = E.B;\ne;";
    let info = get_hover_at(source, 1, 12).expect("Should find hover info for const enum member");
    assert_eq!(info.display_string, "(enum member) E.B = 1");
}

#[test]
fn test_hover_async_promise_result() {
    // const q = ag()  =>  tsserver: `const q: Promise<number>`
    let lib = Arc::new(LibFile::from_source(
        "lib.es2022.d.ts".to_string(),
        "interface Promise<T> { then(): void; }\n\
         interface PromiseConstructor { resolve(): Promise<void>; }\n\
         declare var Promise: PromiseConstructor;"
            .to_string(),
    ));
    let source = "async function ag(): Promise<number> { return 1; }\nconst q = ag();\nq;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file_with_libs(parser.get_arena(), root, &[Arc::clone(&lib)]);
    let interner = TypeInterner::new();
    let line_map = LineMap::build(source);
    let lib_contexts = vec![LibContext {
        arena: Arc::clone(&lib.arena),
        binder: Arc::clone(&lib.binder),
    }];
    let provider = HoverProvider::with_options_and_lib_contexts(
        parser.get_arena(),
        &binder,
        &line_map,
        &interner,
        source,
        "test.ts".to_string(),
        FullProviderOptions {
            strict: true,
            checker_options: None,
            lib_contexts: &lib_contexts,
        },
    );
    let mut cache = None;
    let info = provider
        .get_hover(root, Position::new(2, 0), &mut cache)
        .expect("Expected hover for q");
    assert_eq!(info.display_string, "const q: Promise<number>");
}

#[test]
fn test_hover_jsdoc_multiple_links_all_expanded() {
    let source = "/** Use {@link Foo} or {@link Bar} for this. */\nfunction f() {}\nf();";
    let info = get_hover_at(source, 2, 0).expect("Should find hover info");
    let md = hover_markdown_section(&info);
    assert!(
        md.contains("`Foo`"),
        "First link must be expanded; got: {md:?}"
    );
    assert!(
        md.contains("`Bar`"),
        "Second link must be expanded; got: {md:?}"
    );
    assert!(
        !md.contains("{@link"),
        "No raw @link must remain; got: {md:?}"
    );
}
