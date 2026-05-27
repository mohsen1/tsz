#[test]
fn cross_file_non_react_namespace_component_type_still_emits_ts2786() {
    // Cross-file React alias detection must not over-eagerly skip TS2786 for
    // function components with invalid return types.
    //
    // Setup: the cross-file React lib (REACT_DECL) is present as file 0.  The
    // entry file declares a function with an incompatible return type.  TS2786
    // must still fire because `BadComp` is not a React alias type.
    let entry = r#"
declare namespace JSX {
    interface Element { ok: true; }
    interface ElementClass { render(): Element; }
    interface ElementAttributesProperty { props: {}; }
    interface IntrinsicElements {}
}
declare function BadComp(props: {}): number;
const elem = <BadComp />;
"#;
    let diags = check_multi_file(
        &[("react.d.ts", REACT_DECL), ("test.tsx", entry)],
        "test.tsx",
        cross_file_jsx_opts(),
    );
    assert!(
        diags.iter().any(|d| d.code == 2786),
        "Function component returning non-JSX type must still emit TS2786 when cross-file React lib is present; got: {diags:?}"
    );
}
