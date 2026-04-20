#[test]
fn test_ts2604_emitted_for_this_tag_in_class_method() {
    // <this/> inside a class method should emit TS2604 because the class
    // instance type has no construct or call signatures. The `this` keyword
    // starts with a lowercase letter but is NOT an intrinsic element —
    // it must not be skipped by the intrinsic-element shortcut.
    let source = format!(
        r#"
{JSX_PREAMBLE}
declare class Component<T, U> {{}}
class Text extends Component<{{}}, {{}}> {{
    render() {{
        return <this />;
    }}
}}
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        has_code(
            &diags,
            diagnostic_codes::JSX_ELEMENT_TYPE_DOES_NOT_HAVE_ANY_CONSTRUCT_OR_CALL_SIGNATURES
        ),
        "Should emit TS2604 for <this/> in class method since the class instance \
         type has no construct or call signatures, got: {diags:?}"
    );
}
