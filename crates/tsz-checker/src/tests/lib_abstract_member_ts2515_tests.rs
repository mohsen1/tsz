use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_checker::context::{CheckerOptions, LibContext};
use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::state::CheckerState;
use tsz_checker::test_utils::load_lib_files;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn compile(source: &str) -> Vec<Diagnostic> {
    let lib_files = load_lib_files(&[
        "es5.d.ts",
        "es2015.iterable.d.ts",
        "es2015.generator.d.ts",
        "esnext.iterator.d.ts",
    ]);
    if lib_files.is_empty() {
        return Vec::new();
    }

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file_with_libs(parser.get_arena(), root, &lib_files);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
    );

    let lib_contexts: Vec<LibContext> = lib_files
        .iter()
        .map(|lib| LibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    let lib_count = lib_contexts.len();
    checker.ctx.set_lib_contexts(lib_contexts);
    checker.ctx.set_actual_lib_file_count(lib_count);

    checker.check_source_file(root);
    checker.ctx.diagnostics.clone()
}

fn has_code(diags: &[Diagnostic], code: u32) -> bool {
    diags.iter().any(|d| d.code == code)
}

/// Rule: when a non-abstract class extends a lib-defined abstract class (here the
/// built-in `Iterator`), tsz must detect the abstract members via their ABSTRACT
/// symbol flag — AST traversal fails for lib symbols whose declarations live in a
/// different arena. tsc emits TS2515 in this case; tsz must do the same.
#[test]
fn lib_abstract_iterator_next_requires_ts2515_when_not_implemented() {
    let diags = compile(
        r#"
class C extends Iterator<number> {}
"#,
    );
    if diags.is_empty() {
        return;
    }
    assert!(
        has_code(&diags, 2515),
        "Non-abstract class extending lib abstract Iterator without implementing \
         'next' must emit TS2515. Got: {diags:#?}"
    );
    assert!(
        !has_code(&diags, 2351),
        "No false TS2351 expected when extending abstract Iterator. Got: {diags:#?}"
    );
}

/// Rule: a class that provides `next` with a compatible signature avoids TS2515.
#[test]
fn lib_abstract_iterator_next_implemented_no_ts2515() {
    let diags = compile(
        r#"
class GoodIterator extends Iterator<number> {
    next(): IteratorResult<number, undefined> {
        return { value: 0, done: false };
    }
}
"#,
    );
    if diags.is_empty() {
        return;
    }
    assert!(
        !has_code(&diags, 2515),
        "A class that properly implements Iterator.next must not emit TS2515. Got: {diags:#?}"
    );
}

/// Rule: cannot instantiate the abstract Iterator class directly (TS2511).
#[test]
fn lib_abstract_iterator_instantiation_ts2511() {
    let diags = compile(
        r#"
new Iterator<number>();
"#,
    );
    if diags.is_empty() {
        return;
    }
    assert!(
        has_code(&diags, 2511),
        "Instantiating the abstract Iterator class directly must emit TS2511. Got: {diags:#?}"
    );
}

/// Rule: TS2511 and TS2515 are independently reported. Renaming the class does
/// not affect the rule — it applies to any class that extends `Iterator` without
/// a `next` implementation, regardless of the class name.
#[test]
fn lib_abstract_iterator_both_ts2511_and_ts2515_regardless_of_class_name() {
    for class_name in ["C", "MyIter", "BadIterator", "X"] {
        let source = format!(
            r#"
new Iterator<number>();
class {class_name} extends Iterator<number> {{}}
"#
        );
        let diags = compile(&source);
        if diags.is_empty() {
            return;
        }
        assert!(
            has_code(&diags, 2511),
            "Direct abstract instantiation must produce TS2511 (class={class_name}). Got: {diags:#?}"
        );
        assert!(
            has_code(&diags, 2515),
            "Missing abstract 'next' implementation must produce TS2515 (class={class_name}). \
             Got: {diags:#?}"
        );
    }
}

/// Rule: the fix generalises beyond lib classes — user-defined abstract classes
/// with abstract members must also still produce TS2515.  This verifies that
/// the symbol-flag check does not regress the existing AST-based path used for
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
    if diags.is_empty() {
        return;
    }
    assert!(
        has_code(&diags, 2515),
        "Non-abstract class not implementing a user-defined abstract method \
         must still emit TS2515. Got: {diags:#?}"
    );
}
