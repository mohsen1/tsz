use super::DeclarationEmitter;

#[test]
fn template_params_ignore_dash_descriptions() {
    let jsdoc = "\
@template T
@template [U=T] - ok: default can reference earlier type parameter
";

    assert_eq!(
        DeclarationEmitter::parse_jsdoc_template_params(jsdoc),
        vec!["T", "U = T"]
    );
}

#[test]
fn constrained_template_param_ignores_dash_description() {
    let jsdoc = "@template {string | number} [T=string] - ok: defaults are permitted";

    assert_eq!(
        DeclarationEmitter::parse_jsdoc_template_params(jsdoc),
        vec!["T extends string | number = string"]
    );
}

#[test]
fn constrained_template_param_missing_default_uses_any_default() {
    let jsdoc = "\
@template {string | number} [T] - error: default requires an `=type`
@template {string | number} [U=] - error: default requires a `type`
";

    assert_eq!(
        DeclarationEmitter::parse_jsdoc_template_params(jsdoc),
        vec![
            "T extends string | number = any",
            "U extends string | number = any",
        ]
    );
}

#[test]
fn comma_template_params_keep_names_before_dash_description() {
    let jsdoc = "@template T, U, [V=T] - description words are not params";

    assert_eq!(
        DeclarationEmitter::parse_jsdoc_template_params(jsdoc),
        vec!["T", "U", "V = T"]
    );
}

#[test]
fn dash_inside_default_is_not_description_separator() {
    let jsdoc = "@template [T=-1] - description";

    assert_eq!(
        DeclarationEmitter::parse_jsdoc_template_params(jsdoc),
        vec!["T = -1"]
    );
}

#[test]
fn bracket_default_preserves_spaced_type_text() {
    let jsdoc = "@template [T=string | number] - default is a union";

    assert_eq!(
        DeclarationEmitter::parse_jsdoc_template_params(jsdoc),
        vec!["T = string | number"]
    );
}

#[test]
fn constrained_bracket_default_preserves_spaced_type_text() {
    let jsdoc = "@template {string | number} [T=string | number] - default is a union";

    assert_eq!(
        DeclarationEmitter::parse_jsdoc_template_params(jsdoc),
        vec!["T extends string | number = string | number"]
    );
}

#[test]
fn bracket_default_segment_can_be_followed_by_another_param() {
    let jsdoc = "@template [T=string | number], U - default is a union";

    assert_eq!(
        DeclarationEmitter::parse_jsdoc_template_params(jsdoc),
        vec!["T = string | number", "U"]
    );
}

#[test]
fn legacy_jsdoc_generic_dot_is_normalized_by_generic_parser() {
    assert_eq!(
        DeclarationEmitter::normalize_jsdoc_type_expr("Array.<Object.<string, number>>"),
        "Array<{\n    [x: string]: number;\n}>"
    );
    assert_eq!(
        DeclarationEmitter::normalize_jsdoc_type_expr("(Array.<> | null)"),
        "(any[] | null)"
    );
}

#[test]
fn jsdoc_type_attaches_through_trailing_line_comment() {
    assert!(DeclarationEmitter::jsdoc_attaches_through_var_prefix(
        " // explanation\nconst "
    ));
    assert!(DeclarationEmitter::jsdoc_attaches_through_var_prefix(
        " /* explanation */\nlet "
    ));
    assert!(!DeclarationEmitter::jsdoc_attaches_through_var_prefix(
        " sideEffect();\nconst "
    ));
}

#[test]
fn typedef_alias_renders_constrained_template_default_with_description() {
    let jsdoc = "\
@template {string | number} [T=string] - ok: defaults are permitted
@typedef {[T]} A
";

    let decl = DeclarationEmitter::parse_jsdoc_type_alias_decl(jsdoc)
        .expect("expected JSDoc typedef alias");
    assert_eq!(decl.name, "A");
    assert_eq!(decl.type_params, vec!["T extends string | number = string"]);

    let rendered = DeclarationEmitter::render_jsdoc_type_alias_decl(&decl, false)
        .expect("expected rendered type alias");
    assert!(
        rendered.contains("type A<T extends string | number = string> = [T];"),
        "rendered alias should keep the constrained default:\n{rendered}"
    );
}

#[test]
fn typedef_alias_ignores_template_after_typedef_tag() {
    let jsdoc = "\
@typedef Box
@template T
@property {T} value
";

    let decl = DeclarationEmitter::parse_jsdoc_type_alias_decl(jsdoc)
        .expect("expected JSDoc typedef alias");
    assert_eq!(decl.name, "Box");
    assert!(decl.type_params.is_empty());

    let rendered = DeclarationEmitter::render_jsdoc_type_alias_decl(&decl, false)
        .expect("expected rendered type alias");
    assert_eq!(rendered, "type Box = any;\n");
}

#[test]
fn callback_alias_ignores_template_after_callback_tag() {
    let jsdoc = "\
@callback Fn
@template T
@param {T} value
@returns {T}
";

    let decl = DeclarationEmitter::parse_jsdoc_type_alias_decl(jsdoc)
        .expect("expected JSDoc callback alias");
    assert_eq!(decl.name, "Fn");
    assert!(decl.type_params.is_empty());

    let rendered = DeclarationEmitter::render_jsdoc_type_alias_decl(&decl, false)
        .expect("expected rendered type alias");
    assert_eq!(rendered, "type Fn = () => any;\n");
}

#[test]
fn property_alias_nests_dotted_property_paths() {
    let jsdoc = "\
@typedef Nested
@property {Object} outer
@template T
@property {number} outer.value
@property {string} outer.label
";

    let decl = DeclarationEmitter::parse_jsdoc_type_alias_decl(jsdoc)
        .expect("expected JSDoc property alias");
    assert_eq!(decl.name, "Nested");
    assert!(decl.type_params.is_empty());

    let rendered = DeclarationEmitter::render_jsdoc_type_alias_decl(&decl, false)
        .expect("expected rendered type alias");
    assert_eq!(
        rendered,
        "type Nested = {\n    outer: {\n        value: number;\n        label: string;\n    };\n};\n"
    );
}

#[test]
fn overload_template_after_overload_collapses_signature_surface() {
    let jsdoc = "\
@overload
@template T
@template U
@param {U} value
@returns {U}
";

    let signatures = DeclarationEmitter::parse_jsdoc_overload_signatures(jsdoc);
    assert_eq!(signatures.len(), 1);
    assert_eq!(signatures[0].type_params, vec!["U"]);
    assert!(signatures[0].params.is_empty());
    assert_eq!(signatures[0].return_type, "any");
}

#[test]
fn satisfies_param_facts_require_real_tag_boundary() {
    let jsdoc = "\
@satisfiesx {(bad: number) => void}
@satisfies_inner {(alsoBad: boolean) => void}
@satisfies$foo {(alsoBad: boolean) => void}
@satisfies {(value: string, count: number) => void}
";

    assert!(DeclarationEmitter::jsdoc_has_satisfies_tag(jsdoc));
    let params = DeclarationEmitter::parse_jsdoc_satisfies_param_decls(jsdoc);
    assert_eq!(params.len(), 2);
    assert_eq!(params[0].name, "value");
    assert_eq!(params[0].type_text, "string");
    assert_eq!(params[1].name, "count");
    assert_eq!(params[1].type_text, "number");
    assert!(!params[1].rest);
}

#[test]
fn satisfies_param_facts_ignore_longer_tag_names() {
    for jsdoc in [
        "@satisfiesx {(value: string) => void}",
        "@satisfies_inner {(value: string) => void}",
        "@satisfies$foo {(value: string) => void}",
    ] {
        assert!(!DeclarationEmitter::jsdoc_has_satisfies_tag(jsdoc));
        assert!(DeclarationEmitter::parse_jsdoc_satisfies_param_decls(jsdoc).is_empty());
    }
}

#[test]
fn jsdoc_type_expression_requires_real_tag_boundary() {
    for jsdoc in [
        "@typex {string}",
        "@type_inner {number}",
        "@type$foo {boolean}",
    ] {
        assert_eq!(
            DeclarationEmitter::extract_jsdoc_type_expression(jsdoc),
            None
        );
        assert_eq!(DeclarationEmitter::parse_jsdoc_type_text(jsdoc), None);
    }

    assert_eq!(
        DeclarationEmitter::extract_jsdoc_type_expression("@type {string}"),
        Some("string")
    );
    assert_eq!(
        DeclarationEmitter::parse_jsdoc_type_text("@type {string}"),
        Some("string".to_string())
    );
}

#[test]
fn ambient_module_relative_specifier_resolves_against_current_module() {
    // Pinned before routing the collapse loop through
    // path_identity::apply_slash_segments_lossy.
    assert_eq!(
        DeclarationEmitter::resolve_ambient_module_relative_specifier("pkg/mod", "./sib"),
        "pkg/sib"
    );
    assert_eq!(
        DeclarationEmitter::resolve_ambient_module_relative_specifier("pkg/sub/mod", "../other/x"),
        "pkg/other/x"
    );
    // Embedded `.` and empty segments are skipped.
    assert_eq!(
        DeclarationEmitter::resolve_ambient_module_relative_specifier("pkg/mod", ".//./sib"),
        "pkg/sib"
    );
}

#[test]
fn ambient_module_relative_specifier_drops_unmatched_parent_segments() {
    // Historical (and preserved) underflow policy: a `..` that escapes the
    // virtual module root is silently dropped, not kept or bailed on.
    assert_eq!(
        DeclarationEmitter::resolve_ambient_module_relative_specifier("mod", "../../x"),
        "x"
    );
}

#[test]
fn tag_segments_split_at_whitespace_preceded_at_signs() {
    assert_eq!(
        DeclarationEmitter::split_jsdoc_tag_segments("@param {number} [a] @param {number} b"),
        vec!["@param {number} [a]", "@param {number} b"]
    );
    // Leading description text becomes its own (non-tag) segment.
    assert_eq!(
        DeclarationEmitter::split_jsdoc_tag_segments("Computes a thing @param {number} seed"),
        vec!["Computes a thing", "@param {number} seed"]
    );
}

#[test]
fn tag_segments_protect_braced_groups_backticks_and_glued_at() {
    // `{@link ...}` stays inside the current segment.
    assert_eq!(
        DeclarationEmitter::split_jsdoc_tag_segments(
            "@param {number} first - see {@link other} @param {string} second"
        ),
        vec![
            "@param {number} first - see {@link other}",
            "@param {string} second"
        ]
    );
    // A backtick code span protects tag-like text.
    assert_eq!(
        DeclarationEmitter::split_jsdoc_tag_segments(
            "@param {number} real - use `@param {string} fake` in docs"
        ),
        vec!["@param {number} real - use `@param {string} fake` in docs"]
    );
    // A glued `@` (email) is not a tag boundary.
    assert_eq!(
        DeclarationEmitter::split_jsdoc_tag_segments(
            "@param {string} addr mail user@host.example @param {string} subject"
        ),
        vec![
            "@param {string} addr mail user@host.example",
            "@param {string} subject"
        ]
    );
    // `@` followed by whitespace or punctuation is comment text, not a tag.
    assert_eq!(
        DeclarationEmitter::split_jsdoc_tag_segments("@param {string} sigil - an @ alone @!bang"),
        vec!["@param {string} sigil - an @ alone @!bang"]
    );
}

#[test]
fn param_decls_parse_every_tag_on_one_line() {
    let decls =
        DeclarationEmitter::parse_jsdoc_param_decls("@param {number=} lo @param {string} [hi]");
    let summary: Vec<(String, String, bool)> = decls
        .into_iter()
        .map(|d| (d.name, d.type_text, d.optional))
        .collect();
    assert_eq!(
        summary,
        vec![
            ("lo".to_string(), "number".to_string(), true),
            ("hi".to_string(), "string".to_string(), true),
        ]
    );
}
