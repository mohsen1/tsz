use std::sync::{Arc, OnceLock};
use tsz_binder::lib_loader::LibFile;
use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::{check_source_with_libs, has_diagnostic_code, load_lib_files};

fn lib_files() -> &'static Vec<Arc<LibFile>> {
    static CACHE: OnceLock<Vec<Arc<LibFile>>> = OnceLock::new();
    CACHE.get_or_init(|| {
        load_lib_files(&[
            "es5.d.ts",
            "es2015.iterable.d.ts",
            "es2015.generator.d.ts",
            "esnext.iterator.d.ts",
        ])
    })
}

fn compile(source: &str) -> Vec<Diagnostic> {
    check_source_with_libs(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
        lib_files(),
    )
}

fn skip_if_libs_unavailable() -> bool {
    lib_files().is_empty()
}

/// Rule: when a non-abstract class extends a lib-defined abstract class, tsz must
/// detect abstract members via the ABSTRACT symbol flag — not AST traversal, which
/// fails for declarations in different arenas (lib vs user file).
#[test]
fn lib_abstract_iterator_next_requires_ts2515_when_not_implemented() {
    if skip_if_libs_unavailable() {
        return;
    }
    let diags = compile("class C extends Iterator<number> {}");
    assert!(
        has_diagnostic_code(&diags, 2515),
        "Non-abstract class extending lib abstract Iterator without implementing \
         'next' must emit TS2515. Got: {diags:#?}"
    );
    assert!(
        !has_diagnostic_code(&diags, 2351),
        "No false TS2351 expected when extending abstract Iterator. Got: {diags:#?}"
    );
}

#[test]
fn lib_abstract_iterator_next_implemented_no_ts2515() {
    if skip_if_libs_unavailable() {
        return;
    }
    let diags = compile(
        r#"
class GoodIterator extends Iterator<number> {
    next(): IteratorResult<number, undefined> {
        return { value: 0, done: false };
    }
}
"#,
    );
    assert!(
        !has_diagnostic_code(&diags, 2515),
        "A class that properly implements Iterator.next must not emit TS2515. Got: {diags:#?}"
    );
}

#[test]
fn lib_abstract_iterator_instantiation_ts2511() {
    if skip_if_libs_unavailable() {
        return;
    }
    let diags = compile("new Iterator<number>();");
    assert!(
        has_diagnostic_code(&diags, 2511),
        "Instantiating the abstract Iterator class directly must emit TS2511. Got: {diags:#?}"
    );
}

/// Rule: TS2511 and TS2515 apply independently of the class name — any class
/// extending Iterator without implementing `next` triggers both.
#[test]
fn lib_abstract_iterator_both_ts2511_and_ts2515_regardless_of_class_name() {
    if skip_if_libs_unavailable() {
        return;
    }
    for class_name in ["C", "MyIter", "BadIterator", "X"] {
        let source =
            format!("new Iterator<number>();\nclass {class_name} extends Iterator<number> {{}}");
        let diags = compile(&source);
        assert!(
            has_diagnostic_code(&diags, 2511),
            "Direct abstract instantiation must produce TS2511 (class={class_name}). Got: {diags:#?}"
        );
        assert!(
            has_diagnostic_code(&diags, 2515),
            "Missing abstract 'next' implementation must produce TS2515 (class={class_name}). \
             Got: {diags:#?}"
        );
    }
}

/// Verify the symbol-flag check does not regress the existing AST-based path for
/// in-file abstract classes.
#[test]
fn user_defined_abstract_class_still_emits_ts2515() {
    let diags = compile(
        r#"
abstract class MyBase {
    abstract doWork(x: number): void;
}
class BadDerived extends MyBase {}
"#,
    );
    assert!(
        has_diagnostic_code(&diags, 2515),
        "Non-abstract class not implementing a user-defined abstract method \
         must still emit TS2515. Got: {diags:#?}"
    );
}
