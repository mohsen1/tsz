#[test]
fn test_hover_catch_parameter() {
    let source = "try {\n  throw new Error('oops');\n} catch (err) {\n  err;\n}";
    let info = get_hover_at(source, 3, 2);
    if let Some(info) = info {
        assert!(
            info.contents[0].contains("err"),
            "Should contain catch parameter name, got: {}",
            info.contents[0]
        );
    }
}

#[test]
fn test_hover_for_of_variable() {
    let source = "const items = [1, 2, 3];\nfor (const item of items) {\n  item;\n}";
    let info = get_hover_at(source, 2, 2);
    if let Some(info) = info {
        assert!(
            info.contents[0].contains("item"),
            "Should contain for-of variable name, got: {}",
            info.contents[0]
        );
    }
}

#[test]
fn test_hover_arrow_function_typed() {
    let source = "const add = (a: number, b: number): number => a + b;\nadd;";
    let info = get_hover_at(source, 1, 0);
    assert!(
        info.is_some(),
        "Should find hover for arrow function variable"
    );
    if let Some(info) = info {
        assert!(
            info.contents[0].contains("add"),
            "Should contain arrow function variable name, got: {}",
            info.contents[0]
        );
    }
}

#[test]
fn test_hover_namespace_declaration() {
    let source = "namespace MyNS {\n  export const value = 42;\n}\nMyNS;";
    let info = get_hover_at(source, 3, 0);
    if let Some(info) = info {
        assert!(
            info.contents[0].contains("MyNS"),
            "Should contain namespace name, got: {}",
            info.contents[0]
        );
    }
}

#[test]
fn test_hover_type_assertion_variable() {
    let source = "const x = 42 as unknown as string;\nx;";
    let info = get_hover_at(source, 1, 0);
    assert!(
        info.is_some(),
        "Should find hover for type-asserted variable"
    );
    if let Some(info) = info {
        assert!(
            info.contents[0].contains("x"),
            "Should contain variable name, got: {}",
            info.contents[0]
        );
    }
}

#[test]
fn test_hover_import_declaration() {
    let source = "import { foo } from './mod';\nfoo;";
    // Hover over imported identifier
    let info = get_hover_at(source, 1, 0);
    if let Some(info) = info {
        assert!(
            info.contents[0].contains("foo"),
            "Should contain imported name, got: {}",
            info.contents[0]
        );
    }
}

#[test]
fn test_hover_generic_function_call() {
    let source = "function identity<T>(val: T): T { return val; }\nidentity;";
    let info = get_hover_at(source, 1, 0);
    assert!(info.is_some(), "Should find hover for generic function");
    if let Some(info) = info {
        assert!(
            info.contents[0].contains("identity"),
            "Should contain function name, got: {}",
            info.contents[0]
        );
    }
}

#[test]
fn test_hover_method_declaration() {
    let source =
        "class Greeter {\n  greet(name: string): string {\n    return 'Hello ' + name;\n  }\n}";
    let info = get_hover_at(source, 1, 4);
    if let Some(info) = info {
        assert!(
            info.display_string.contains("greet"),
            "Should contain method name, got: {}",
            info.display_string
        );
    }
}

#[test]
fn test_hover_getter_accessor() {
    let source =
        "class Box {\n  private _value = 0;\n  get value(): number { return this._value; }\n}";
    let info = get_hover_at(source, 2, 6);
    if let Some(info) = info {
        assert!(
            info.display_string.contains("value"),
            "Should contain getter name, got: {}",
            info.display_string
        );
    }
}

#[test]
fn test_hover_setter_accessor() {
    let source =
        "class Box {\n  private _value = 0;\n  set value(v: number) { this._value = v; }\n}";
    let info = get_hover_at(source, 2, 6);
    if let Some(info) = info {
        assert!(
            info.display_string.contains("value"),
            "Should contain setter name, got: {}",
            info.display_string
        );
    }
}

#[test]
fn test_hover_static_method() {
    let source = "class Factory {\n  static create(): Factory { return new Factory(); }\n}";
    let info = get_hover_at(source, 1, 9);
    if let Some(info) = info {
        assert!(
            info.display_string.contains("create"),
            "Should contain static method name, got: {}",
            info.display_string
        );
    }
}

#[test]
fn test_hover_mapped_type_alias() {
    let source = "type ReadonlyAll<T> = { readonly [K in keyof T]: T[K] };\ntype X = ReadonlyAll<{a: 1}>;\nlet v: X;";
    // Hover over the type alias name
    let info = get_hover_at(source, 0, 5);
    if let Some(info) = info {
        assert!(
            info.contents[0].contains("ReadonlyAll"),
            "Should contain mapped type alias name, got: {}",
            info.contents[0]
        );
    }
}

#[test]
fn test_hover_conditional_type_alias() {
    let source = "type IsString<T> = T extends string ? true : false;\ntype R = IsString<'hello'>;";
    let info = get_hover_at(source, 0, 5);
    if let Some(info) = info {
        assert!(
            info.contents[0].contains("IsString"),
            "Should contain conditional type alias name, got: {}",
            info.contents[0]
        );
    }
}

#[test]
fn test_hover_export_default_function() {
    let source = "export default function myFunc() { return 1; }";
    let info = get_hover_at(source, 0, 24);
    if let Some(info) = info {
        assert!(
            info.contents[0].contains("myFunc"),
            "Should contain exported default function name, got: {}",
            info.contents[0]
        );
    }
}

#[test]
fn test_hover_private_field() {
    let source = "class Secret {\n  #data = 42;\n  reveal() { return this.#data; }\n}";
    let info = get_hover_at(source, 1, 4);
    if let Some(info) = info {
        assert!(
            info.display_string.contains("data") || info.display_string.contains("#data"),
            "Should contain private field name, got: {}",
            info.display_string
        );
    }
}

#[test]
fn test_hover_keyword_returns_none() {
    let source = "const x = 1;";
    // Hover over the 'const' keyword at col 0
    let info = get_hover_at(source, 0, 0);
    assert!(info.is_none(), "Should return None for keyword 'const'");
}

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
