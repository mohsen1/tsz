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
