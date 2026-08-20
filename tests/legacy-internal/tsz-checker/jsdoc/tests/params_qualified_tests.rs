use super::*;

fn qualified_param_names(jsdoc: &str) -> Vec<String> {
    CheckerState::jsdoc_unattached_qualified_param_tags(jsdoc)
        .into_iter()
        .map(|(_, name, _)| name)
        .collect()
}

#[test]
fn qualified_param_without_any_parent_tag_is_unattached() {
    assert_eq!(
        qualified_param_names(" * @param {number} xyz.p"),
        vec!["xyz.p".to_string()]
    );
    // Two levels deep with no parent at all still reports the whole path.
    assert_eq!(
        qualified_param_names(" * @param {number} xyz.bar.p"),
        vec!["xyz.bar.p".to_string()]
    );
}

#[test]
fn object_typed_parent_absorbs_direct_child_only() {
    // Direct child: absorbed, so only the parent tag is unattached.
    assert_eq!(
        qualified_param_names(" * @param {object} xyz.bar\n * @param {number} xyz.bar.p"),
        vec!["xyz.bar".to_string()]
    );
    // A grandchild skips a level, so it is not a direct child and stays
    // unattached even though its root is object-typed.
    assert_eq!(
        qualified_param_names(" * @param {object} xyz\n * @param {number} xyz.bar.p"),
        vec!["xyz.bar.p".to_string()]
    );
}

#[test]
fn absorption_is_name_independent() {
    // Same shape under renamed binders — the rule is structural, not textual.
    for (root, child) in [("xyz", "bar"), ("options", "nested"), ("_a1", "b2")] {
        let jsdoc = format!(" * @param {{object}} {root}\n * @param {{string}} {root}.{child}");
        assert!(
            qualified_param_names(&jsdoc).is_empty(),
            "{root}.{child} should be absorbed"
        );
        let skipped =
            format!(" * @param {{object}} {root}\n * @param {{string}} {root}.{child}.deep");
        assert_eq!(
            qualified_param_names(&skipped),
            vec![format!("{root}.{child}.deep")]
        );
    }
}

#[test]
fn only_object_typed_parents_absorb() {
    // `object`, `Object`, and arrays of either can carry children.
    for parent_type in ["object", "Object", "object[]", "Object[]"] {
        let jsdoc = format!(" * @param {{{parent_type}}} o\n * @param {{string}} o.x");
        assert!(
            qualified_param_names(&jsdoc).is_empty(),
            "{parent_type} should absorb o.x"
        );
    }
    // Any other parent type does not, so the child is reported.
    for parent_type in ["string", "number", "Foo", "Object<string, number>"] {
        let jsdoc = format!(" * @param {{{parent_type}}} o\n * @param {{string}} o.x");
        assert_eq!(
            qualified_param_names(&jsdoc),
            vec!["o.x".to_string()],
            "{parent_type} must not absorb o.x"
        );
    }
}

#[test]
fn bracketed_array_element_paths_nest_like_plain_paths() {
    // tsc's `parseJSDocEntityName` accepts `y[]` but discards the brackets.
    assert!(
        qualified_param_names(
            " * @param {Object[]} opts2\n * @param {string} opts2[].anotherX\n\
             * @param {string=} opts2[].anotherY"
        )
        .is_empty()
    );
    // Deep chain of array element paths, each level object-typed.
    assert!(
        qualified_param_names(
            " * @param {object[]} o\n * @param {object} o[].what\n\
             * @param {object[]} o[].what.bad\n * @param {string} o[].what.bad[].idea"
        )
        .is_empty()
    );
    // Optional and defaulted spellings still nest.
    assert!(
        qualified_param_names(
            " * @param {Object} o\n * @param {string} [o.z]\n * @param {string} [o.w=\"hi\"]"
        )
        .is_empty()
    );
}

#[test]
fn plain_identifier_tags_are_never_ts8032() {
    // Unmatched plain names are TS8024's business.
    assert!(qualified_param_names(" * @param {Object} a\n * @param {string} unrelated").is_empty());
    assert!(qualified_param_names(" * @param {string} b").is_empty());
}

#[test]
fn ts8032_reports_qualified_param_in_js_file() {
    let diags = crate::test_utils::check_js_source_diagnostics(
        "/**\n * @param {number} xyz.p\n */\nfunction g(xyz) { return xyz.p; }\n",
    );
    let ts8032: Vec<_> = diags.iter().filter(|d| d.code == 8032).collect();
    assert_eq!(ts8032.len(), 1, "got: {diags:?}");
    assert_eq!(
        ts8032[0].message_text,
        "Qualified name 'xyz.p' is not allowed without a leading '@param {object} xyz'."
    );
}

#[test]
fn ts8032_names_the_direct_parent_it_would_have_needed() {
    let diags = crate::test_utils::check_js_source_diagnostics(
        "/**\n * @param {object} xyz\n * @param {number} xyz.bar.p\n */\n\
         function g(xyz) { return xyz.bar.p; }\n",
    );
    let ts8032: Vec<_> = diags.iter().filter(|d| d.code == 8032).collect();
    assert_eq!(ts8032.len(), 1, "got: {diags:?}");
    assert_eq!(
        ts8032[0].message_text,
        "Qualified name 'xyz.bar.p' is not allowed without a leading '@param {object} xyz.bar'."
    );
}

#[test]
fn ts8032_is_not_reported_in_typescript_files() {
    let diags = crate::test_utils::check_source_diagnostics(
        "/**\n * @param {number} xyz.p\n */\nfunction g(xyz: any) { return xyz.p; }\n",
    );
    assert!(
        diags.iter().all(|d| d.code != 8032),
        "TS8032 is JS-only; got: {diags:?}"
    );
}

#[test]
fn ts8032_is_suppressed_when_the_body_reads_arguments() {
    let diags = crate::test_utils::check_js_source_diagnostics(
        "/**\n * @param {number} xyz.p\n */\nfunction g() { return arguments.length; }\n",
    );
    assert!(
        diags.iter().all(|d| d.code != 8032),
        "tsc skips the unmatched-parameter branch when `arguments` is read; got: {diags:?}"
    );
}

#[test]
fn ts8032_absorbed_nested_params_report_nothing() {
    let diags = crate::test_utils::check_js_source_diagnostics(
        "/**\n * @param {object} opts\n * @param {string} opts.x\n */\n\
         function g(opts) { return opts.x; }\n",
    );
    assert!(
        diags.iter().all(|d| d.code != 8032),
        "properly nested params must not report; got: {diags:?}"
    );
}
