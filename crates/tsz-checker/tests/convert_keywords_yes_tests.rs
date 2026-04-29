use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source;

#[test]
fn convert_keywords_yes_type_params_and_namespace_classes_match_tsc_codes() {
    let source = r#"
function bigGeneric<
    constructor,
    implements,
    interface,
    let,
    module,
    package,
    private,
    protected,
    public,
    set,
    static,
    get,
    yield,
    declare
    >() { }

namespace bigModule {
    class constructor { }
    class implements { }
    class interface { }
    class let { }
    class module { }
    class package { }
    class private { }
    class protected { }
    class public { }
    class set { }
    class static { }
    class get { }
    class yield { }
    class declare { }
}
"#;

    let diagnostics = check_source(
        source,
        "test.ts",
        CheckerOptions {
            strict: false,
            always_strict: true,
            ..CheckerOptions::default()
        },
    );
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();

    assert!(
        codes.iter().filter(|&&code| code == 1212).count() >= 4,
        "expected strict-mode type parameter TS1212 diagnostics, got {diagnostics:?}"
    );
    assert!(
        codes.iter().filter(|&&code| code == 1213).count() >= 9,
        "expected namespace class-name TS1213 diagnostics, got {diagnostics:?}"
    );
    for unexpected in [1139, 2300, 2749] {
        assert!(
            !codes.contains(&unexpected),
            "did not expect TS{unexpected} cascade diagnostics, got {diagnostics:?}"
        );
    }
}
