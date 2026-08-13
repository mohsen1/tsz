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
fn overload_signatures_parse_every_tag_on_one_line() {
    // Both the tag sharing the `@overload` line and a mid-block multi-tag
    // line must contribute; tsc parses one signature per `@overload` with
    // params (x: number, y: string) => string and (x: string) => number.
    let jsdoc = "\
@overload
@param {number} x @param {string} y
@returns {string}
@overload @param {string} x @returns {number}
";

    let signatures = DeclarationEmitter::parse_jsdoc_overload_signatures(jsdoc);
    assert_eq!(signatures.len(), 2);

    let first: Vec<(&str, &str)> = signatures[0]
        .params
        .iter()
        .map(|p| (p.name.as_str(), p.type_text.as_str()))
        .collect();
    assert_eq!(first, vec![("x", "number"), ("y", "string")]);
    assert_eq!(signatures[0].return_type, "string");

    let second: Vec<(&str, &str)> = signatures[1]
        .params
        .iter()
        .map(|p| (p.name.as_str(), p.type_text.as_str()))
        .collect();
    assert_eq!(second, vec![("x", "string")]);
    assert_eq!(signatures[1].return_type, "number");
}

#[test]
fn overload_signatures_keep_braced_tag_text_in_one_segment() {
    // An `@`-tag inside a braced group is comment text, not a boundary:
    // the bogus `@param` inside `{@link ...}` must not become a parameter.
    let jsdoc = "\
@overload
@param {number} x - see {@link other @param {bogus} nope}
@returns {string}
";

    let signatures = DeclarationEmitter::parse_jsdoc_overload_signatures(jsdoc);
    assert_eq!(signatures.len(), 1);
    let params: Vec<(&str, &str)> = signatures[0]
        .params
        .iter()
        .map(|p| (p.name.as_str(), p.type_text.as_str()))
        .collect();
    assert_eq!(params, vec![("x", "number")]);
    assert_eq!(signatures[0].return_type, "string");
}

#[test]
fn property_alias_parses_every_tag_on_one_line() {
    // The whole typedef fits on one physical line; both properties parse and
    // the bracketed name stays `?`-optional without an `| undefined` branch.
    let jsdoc = "@typedef {Object} Pair @property {number} first @property {string} [second]";

    let decl = DeclarationEmitter::parse_jsdoc_type_alias_decl(jsdoc)
        .expect("expected JSDoc property alias");
    assert_eq!(decl.name, "Pair");

    let rendered = DeclarationEmitter::render_jsdoc_type_alias_decl(&decl, false)
        .expect("expected rendered type alias");
    assert_eq!(
        rendered,
        "type Pair = {\n    first: number;\n    second?: string;\n};\n"
    );
}

#[test]
fn property_alias_same_line_tags_nest_dotted_paths_and_prop_alias() {
    let jsdoc = "\
@typedef Nested
@property {Object} outer @prop {number} outer.value @property {string} outer.label
";

    let decl = DeclarationEmitter::parse_jsdoc_type_alias_decl(jsdoc)
        .expect("expected JSDoc property alias");
    assert_eq!(decl.name, "Nested");

    let rendered = DeclarationEmitter::render_jsdoc_type_alias_decl(&decl, false)
        .expect("expected rendered type alias");
    assert_eq!(
        rendered,
        "type Nested = {\n    outer: {\n        value: number;\n        label: string;\n    };\n};\n"
    );
}

#[test]
fn property_alias_braced_tag_text_stays_description() {
    // `@property` inside a `{@link ...}` group is protected text: only the
    // real property parses, and the link stays in its description.
    let jsdoc = "\
@typedef {Object} Doc
@property {number} a see {@link B @property {string} fake}
";

    let decl = DeclarationEmitter::parse_jsdoc_type_alias_decl(jsdoc)
        .expect("expected JSDoc property alias");
    assert_eq!(decl.name, "Doc");

    let rendered = DeclarationEmitter::render_jsdoc_type_alias_decl(&decl, false)
        .expect("expected rendered type alias");
    assert_eq!(
        rendered,
        "type Doc = {\n    /**\n     * see {@link B @property {string} fake}\n     */\n    a: number;\n};\n"
    );
}

#[test]
fn property_alias_typeless_property_gets_any() {
    // tsc gives a type-less `@property p` the `any` type instead of
    // dropping the alias, in the same-line and mixed shapes alike.
    let decl = DeclarationEmitter::parse_jsdoc_type_alias_decl("@typedef {Object} T1 @property p")
        .expect("expected JSDoc property alias");
    assert_eq!(decl.name, "T1");
    let rendered = DeclarationEmitter::render_jsdoc_type_alias_decl(&decl, false)
        .expect("expected rendered type alias");
    assert_eq!(rendered, "type T1 = {\n    p: any;\n};\n");

    let decl = DeclarationEmitter::parse_jsdoc_type_alias_decl(
        "@typedef {Object} T3\n@property {number} a @property p",
    )
    .expect("expected JSDoc property alias");
    let rendered = DeclarationEmitter::render_jsdoc_type_alias_decl(&decl, false)
        .expect("expected rendered type alias");
    assert_eq!(rendered, "type T3 = {\n    a: number;\n    p: any;\n};\n");
}

#[test]
fn property_alias_postfix_type_form_parses() {
    // tsc's `@property name {type}` postfix form, with the `=` marker
    // carrying both `?` and the `| undefined` branch (oracle: postfix1.js).
    let decl = DeclarationEmitter::parse_jsdoc_type_alias_decl(
        "@typedef {Object} P\n@property anotherX {string}\n@property anotherY {string=}",
    )
    .expect("expected JSDoc property alias");
    assert_eq!(decl.name, "P");
    let rendered = DeclarationEmitter::render_jsdoc_type_alias_decl(&decl, false)
        .expect("expected rendered type alias");
    assert_eq!(
        rendered,
        "type P = {\n    anotherX: string;\n    anotherY?: string | undefined;\n};\n"
    );
}

#[test]
fn property_tag_detection_requires_real_tag_boundary() {
    // `@propertyx` / `@properties` are different tags, not property tags.
    assert!(!DeclarationEmitter::jsdoc_has_property_tags(
        "@typedef {Object} T\n@propertyx {number} y"
    ));
    assert!(!DeclarationEmitter::jsdoc_has_property_tags(
        "@typedef {Object} T\n@properties {number} y"
    ));
}

#[test]
fn has_property_tags_sees_same_line_and_respects_braced_groups() {
    assert!(DeclarationEmitter::jsdoc_has_property_tags(
        "@typedef {Object} T @property {number} a"
    ));
    // Braced `{@link ... @property ...}` text is not a property tag.
    assert!(!DeclarationEmitter::jsdoc_has_property_tags(
        "@typedef {Object} T see {@link B @property {string} fake}"
    ));
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

#[test]
fn property_list_terminated_by_unknown_tag_falls_back_to_annotation() {
    // tsc: an unrecognized tag before the `@property` list ends the list, and
    // the typedef resolves from its braced annotation instead (#17285).
    let decl = DeclarationEmitter::parse_jsdoc_type_alias_decl(
        "@typedef {Object} Legacy\n@foo bar\n@property {number} real",
    )
    .expect("expected fallback typedef alias");
    assert_eq!(decl.name, "Legacy");

    let rendered = DeclarationEmitter::render_jsdoc_type_alias_decl(&decl, false)
        .expect("expected rendered type alias");
    assert_eq!(rendered, "type Legacy = Object;\n");
}

#[test]
fn property_list_terminated_by_near_miss_property_tags() {
    // `@propertyx` / `@properties` are not property tags, so they terminate
    // the list exactly like any other unrecognized tag.
    for stray in ["@propertyx bogus", "@properties nope"] {
        let jsdoc = format!("@typedef {{Object}} Cfg\n{stray}\n@property {{number}} real");
        let decl = DeclarationEmitter::parse_jsdoc_type_alias_decl(&jsdoc)
            .expect("expected fallback typedef alias");
        let rendered = DeclarationEmitter::render_jsdoc_type_alias_decl(&decl, false)
            .expect("expected rendered type alias");
        assert_eq!(rendered, "type Cfg = Object;\n", "stray tag: {stray}");
    }
}

#[test]
fn property_list_terminated_by_known_non_property_tags() {
    // Known-but-foreign tags (`@see`, `@param`, `@returns`, `@author`)
    // terminate the list the same way unknown tags do.
    for stray in [
        "@see something",
        "@param {number} x",
        "@returns {number} nope",
        "@author someone",
    ] {
        let jsdoc = format!("@typedef {{Object}} Rec\n{stray}\n@property {{number}} real");
        let decl = DeclarationEmitter::parse_jsdoc_type_alias_decl(&jsdoc)
            .expect("expected fallback typedef alias");
        let rendered = DeclarationEmitter::render_jsdoc_type_alias_decl(&decl, false)
            .expect("expected rendered type alias");
        assert_eq!(rendered, "type Rec = Object;\n", "stray tag: {stray}");
    }
}

#[test]
fn property_list_survives_trailing_unknown_tag() {
    // An unknown tag after the properties does not break the chain.
    let decl = DeclarationEmitter::parse_jsdoc_type_alias_decl(
        "@typedef {Object} Opts\n@property {number} real\n@foo bar",
    )
    .expect("expected JSDoc property alias");
    let rendered = DeclarationEmitter::render_jsdoc_type_alias_decl(&decl, false)
        .expect("expected rendered type alias");
    assert_eq!(rendered, "type Opts = {\n    real: number;\n};\n");
}

#[test]
fn property_list_keeps_only_prefix_before_terminating_tag() {
    // A tag between two properties keeps the prefix and discards the rest.
    let decl = DeclarationEmitter::parse_jsdoc_type_alias_decl(
        "@typedef {Object} Pair\n@property {number} first\n@foo bar\n@property {string} second",
    )
    .expect("expected JSDoc property alias");
    let rendered = DeclarationEmitter::render_jsdoc_type_alias_decl(&decl, false)
        .expect("expected rendered type alias");
    assert_eq!(rendered, "type Pair = {\n    first: number;\n};\n");
}

#[test]
fn property_tags_before_typedef_are_ignored_without_terminating() {
    // A `@property` before the `@typedef` tag is dropped by tsc, and does not
    // end the list that follows the typedef.
    let decl = DeclarationEmitter::parse_jsdoc_type_alias_decl(
        "@property {number} early\n@typedef {Object} Late\n@property {number} real",
    )
    .expect("expected JSDoc property alias");
    let rendered = DeclarationEmitter::render_jsdoc_type_alias_decl(&decl, false)
        .expect("expected rendered type alias");
    assert_eq!(rendered, "type Late = {\n    real: number;\n};\n");
}

#[test]
fn name_only_typedef_with_terminated_property_list_drops_alias() {
    // With no braced annotation to fall back to, tsc emits no alias at all.
    assert!(
        DeclarationEmitter::parse_jsdoc_type_alias_decl(
            "@typedef Bare\n@foo bar\n@property {number} real",
        )
        .is_none()
    );
}

#[test]
fn non_object_typedef_with_property_tags_falls_back_to_annotation() {
    // tsc ignores `@property` tags on a non-object typedef and keeps the
    // annotation; the alias must not be dropped.
    let decl = DeclarationEmitter::parse_jsdoc_type_alias_decl(
        "@typedef {string} Label\n@property {number} x",
    )
    .expect("expected fallback typedef alias");
    let rendered = DeclarationEmitter::render_jsdoc_type_alias_decl(&decl, false)
        .expect("expected rendered type alias");
    assert_eq!(rendered, "type Label = string;\n");
}

#[test]
fn same_line_unknown_tag_after_typedef_terminates_property_list() {
    // The terminator can share the `@typedef` line: segments, not lines,
    // drive the scan.
    let decl = DeclarationEmitter::parse_jsdoc_type_alias_decl(
        "@typedef {Object} Inline @foo bar\n@property {number} real",
    )
    .expect("expected fallback typedef alias");
    let rendered = DeclarationEmitter::render_jsdoc_type_alias_decl(&decl, false)
        .expect("expected rendered type alias");
    assert_eq!(rendered, "type Inline = Object;\n");
}

#[test]
fn description_lines_do_not_terminate_property_list() {
    // Plain description lines between the typedef and its properties are not
    // tags and leave the list intact.
    let decl = DeclarationEmitter::parse_jsdoc_type_alias_decl(
        "@typedef {Object} Doc\nsome description line\n@property {number} real",
    )
    .expect("expected JSDoc property alias");
    let rendered = DeclarationEmitter::render_jsdoc_type_alias_decl(&decl, false)
        .expect("expected rendered type alias");
    assert_eq!(rendered, "type Doc = {\n    real: number;\n};\n");
}

#[test]
fn type_tag_is_transparent_to_property_list() {
    // `@type` is a recognized typedef companion tag in tsc: it does not
    // terminate the property list, whether it precedes the properties or
    // sits between them.
    let decl = DeclarationEmitter::parse_jsdoc_type_alias_decl(
        "@typedef Shape\n@type {object}\n@property {string} id",
    )
    .expect("expected JSDoc property alias");
    let rendered = DeclarationEmitter::render_jsdoc_type_alias_decl(&decl, false)
        .expect("expected rendered type alias");
    assert_eq!(rendered, "type Shape = {\n    id: string;\n};\n");

    let decl = DeclarationEmitter::parse_jsdoc_type_alias_decl(
        "@typedef Wide\n@property {string} a\n@type {object}\n@property {string} b",
    )
    .expect("expected JSDoc property alias");
    let rendered = DeclarationEmitter::render_jsdoc_type_alias_decl(&decl, false)
        .expect("expected rendered type alias");
    assert_eq!(
        rendered,
        "type Wide = {\n    a: string;\n    b: string;\n};\n"
    );
}

#[test]
fn inline_object_typedef_annotation_wins_over_property_tags() {
    // An inline `@typedef {{...}} M` annotation wins outright: `@property`
    // tags are ignored rather than merged, and the alias must not be
    // dropped just because the property path declines the non-Object base.
    let decl = DeclarationEmitter::parse_jsdoc_type_alias_decl(
        "@typedef {{a: number}} Mix\n@property {string} b",
    )
    .expect("expected inline typedef alias");
    let rendered = DeclarationEmitter::render_jsdoc_type_alias_decl(&decl, false)
        .expect("expected rendered type alias");
    assert_eq!(rendered, "type Mix = {\n    a: number;\n};\n");
}

fn rendered_alias_decls(jsdoc: &str) -> Vec<String> {
    DeclarationEmitter::parse_jsdoc_type_alias_decls(jsdoc)
        .iter()
        .map(|decl| {
            DeclarationEmitter::render_jsdoc_type_alias_decl(decl, false)
                .expect("expected rendered type alias")
        })
        .collect()
}

#[test]
fn multi_typedef_block_emits_every_alias() {
    // Each `@typedef` in one comment declares its own alias with its own
    // property run (oracle: typescript@7.0.2).
    let rendered = rendered_alias_decls(
        "@typedef {Object} A\n@property {number} a\n@typedef {Object} B\n@property {string} b",
    );
    assert_eq!(
        rendered,
        vec![
            "type A = {\n    a: number;\n};\n",
            "type B = {\n    b: string;\n};\n",
        ]
    );
}

#[test]
fn multi_typedef_block_keeps_annotation_and_property_aliases_in_source_order() {
    let rendered =
        rendered_alias_decls("@typedef {Object} A\n@property {number} a\n@typedef {string} S");
    assert_eq!(
        rendered,
        vec!["type A = {\n    a: number;\n};\n", "type S = string;\n"]
    );

    let rendered =
        rendered_alias_decls("@typedef {string} S\n@typedef {Object} B\n@property {number} b");
    assert_eq!(
        rendered,
        vec!["type S = string;\n", "type B = {\n    b: number;\n};\n"]
    );
}

#[test]
fn multi_alias_block_mixes_callback_and_typedef_in_both_orders() {
    let rendered = rendered_alias_decls(
        "@callback Cb\n@param {number} x\n@returns {string}\n@typedef {Object} T\n@property {number} a",
    );
    assert_eq!(
        rendered,
        vec![
            "type Cb = (x: number) => string;\n",
            "type T = {\n    a: number;\n};\n",
        ]
    );

    let rendered = rendered_alias_decls(
        "@typedef {Object} T\n@property {number} a\n@callback Cb\n@param {number} x\n@returns {string}",
    );
    assert_eq!(
        rendered,
        vec![
            "type T = {\n    a: number;\n};\n",
            "type Cb = (x: number) => string;\n",
        ]
    );
}

#[test]
fn multi_callback_block_emits_every_callback() {
    let rendered = rendered_alias_decls(
        "@callback Cb1\n@param {number} x\n@returns {string}\n@callback Cb2\n@param {string} y\n@returns {number}",
    );
    assert_eq!(
        rendered,
        vec![
            "type Cb1 = (x: number) => string;\n",
            "type Cb2 = (y: string) => number;\n",
        ]
    );
}

#[test]
fn block_templates_bind_to_every_alias() {
    // A `@template` between two annotated typedefs parameterizes both
    // (oracle: `type Arr<T, U> = T[]` / `type Maybe<T, U> = U | null`).
    let rendered = rendered_alias_decls(
        "@template T\n@typedef {T[]} Arr\n@template U\n@typedef {U|null} Maybe",
    );
    assert_eq!(
        rendered,
        vec!["type Arr<T, U> = T[];\n", "type Maybe<T, U> = U | null;\n"]
    );
}

#[test]
fn template_after_plain_typedef_binds_block_wide() {
    // A braced non-object typedef absorbs nothing, so a trailing
    // `@template` still binds (oracle: `type Arr<T> = T[]`), whether the
    // annotation references it or not.
    let decl = DeclarationEmitter::parse_jsdoc_type_alias_decl("@typedef {T[]} Arr\n@template T")
        .expect("expected typedef alias");
    assert_eq!(decl.type_params, vec!["T"]);
    assert_eq!(decl.type_text, "T[]");

    let decl = DeclarationEmitter::parse_jsdoc_type_alias_decl("@typedef {number} N\n@template T")
        .expect("expected typedef alias");
    assert_eq!(decl.type_params, vec!["T"]);
    assert_eq!(decl.type_text, "number");
}

#[test]
fn template_swallowed_by_absorbing_typedef_degrades_later_alias() {
    // A `@template` after an object typedef's property run binds to no
    // alias in the block; tsc prints the unbound name verbatim (invalid
    // `.d.ts`), so the referencing alias degrades to `any` instead.
    let rendered = rendered_alias_decls(
        "@typedef {Object} A\n@property {number} a\n@template T\n@typedef {T[]} Arr",
    );
    assert_eq!(
        rendered,
        vec!["type A = {\n    a: number;\n};\n", "type Arr = any;\n"]
    );
}

#[test]
fn second_property_list_keeps_prefix_before_terminating_tag() {
    // The #17290 prefix rule applies per alias: a foreign tag inside the
    // second typedef's run drops only that list's tail.
    let rendered = rendered_alias_decls(
        "@typedef {Object} A\n@property {number} a\n@typedef {Object} B\n@property {string} b\n@foo bar\n@property {boolean} c",
    );
    assert_eq!(
        rendered,
        vec![
            "type A = {\n    a: number;\n};\n",
            "type B = {\n    b: string;\n};\n",
        ]
    );
}

#[test]
fn foreign_tag_between_typedefs_does_not_hide_the_second_alias() {
    let rendered = rendered_alias_decls(
        "@typedef {Object} A\n@property {number} a\n@foo bar\n@typedef {Object} B\n@property {string} b",
    );
    assert_eq!(
        rendered,
        vec![
            "type A = {\n    a: number;\n};\n",
            "type B = {\n    b: string;\n};\n",
        ]
    );
}

#[test]
fn name_only_second_typedef_collects_its_own_properties() {
    let rendered = rendered_alias_decls(
        "@typedef {Object} A\n@property {number} a\n@typedef Bare\n@property {string} b",
    );
    assert_eq!(
        rendered,
        vec![
            "type A = {\n    a: number;\n};\n",
            "type Bare = {\n    b: string;\n};\n",
        ]
    );
}

#[test]
fn same_line_second_typedef_starts_its_own_alias() {
    // Segments, not lines, delimit alias blocks.
    let rendered = rendered_alias_decls(
        "@typedef {Object} A @property {number} a @typedef {Object} B @property {string} b",
    );
    assert_eq!(
        rendered,
        vec![
            "type A = {\n    a: number;\n};\n",
            "type B = {\n    b: string;\n};\n",
        ]
    );
}

#[test]
fn duplicate_alias_names_parse_both_decls() {
    // tsc emits both duplicate aliases verbatim (invalid `.d.ts`); parsing
    // surfaces both and the emitter's per-name dedup keeps the first.
    let decls = DeclarationEmitter::parse_jsdoc_type_alias_decls(
        "@typedef {Object} A\n@property {number} a\n@typedef {Object} A\n@property {string} b",
    );
    assert_eq!(decls.len(), 2);
    assert_eq!(decls[0].name, "A");
    assert_eq!(decls[1].name, "A");
}

#[test]
fn single_alias_block_parses_identically_through_both_entry_points() {
    let jsdoc = "@typedef {Object} Pair\n@property {number} first\n@property {string} [second]";
    let single = DeclarationEmitter::parse_jsdoc_type_alias_decl(jsdoc)
        .expect("expected JSDoc property alias");
    let decls = DeclarationEmitter::parse_jsdoc_type_alias_decls(jsdoc);
    assert_eq!(decls.len(), 1);
    assert_eq!(decls[0].name, single.name);
    assert_eq!(decls[0].type_text, single.type_text);
    assert_eq!(decls[0].type_params, single.type_params);
}
