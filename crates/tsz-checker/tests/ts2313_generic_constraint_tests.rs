//! Regression tests for TS2313 circular generic constraint detection.

use tsz_checker::test_utils::check_source_diagnostics;

#[test]
fn mixin_constructor_alias_constraint_no_false_ts2313() {
    let source = r#"
type Constructor = new (...args: any[]) => {};

declare const Object: Constructor;

const Mixin1 = <C extends Constructor>(Base: C) => class extends Base { private _fooPrivate: {}; }

type FooConstructor = typeof Mixin1 extends (a: Constructor) => infer Cls ? Cls : never;
const Mixin2 = <C extends FooConstructor>(Base: C) => class extends Base {};

class C extends Mixin2(Mixin1(Object)) {}
"#;
    let diags = check_source_diagnostics(source);

    assert!(
        diags.iter().all(|d| d.code != 2313),
        "Expected no TS2313 for mixin constructor alias constraint, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}
