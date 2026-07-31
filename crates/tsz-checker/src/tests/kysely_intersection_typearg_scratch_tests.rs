//! Scratch repro for #16025's smallest witness: contextual inference of a
//! generic, argument-less call's type parameter from an intersection member
//! (`TB & string`) against the substituted contextual type.

use tsz_common::options::checker::CheckerOptions;

fn opts() -> CheckerOptions {
    CheckerOptions {
        strict: true,
        strict_null_checks: true,
        ..CheckerOptions::default()
    }
}

fn codes(source: &str) -> Vec<u32> {
    crate::test_utils::check_source(source, "test.ts", opts())
        .iter()
        .map(|diag| diag.code)
        .collect()
}

fn messages(source: &str) -> Vec<String> {
    crate::test_utils::check_source(source, "test.ts", opts())
        .iter()
        .map(|diag| {
            let mut lines = vec![format!("TS{}: {}", diag.code, diag.message_text)];
            for rel in &diag.related_information {
                lines.push(format!("  related: {}", rel.message_text));
            }
            lines.join("\n")
        })
        .collect()
}

#[test]
fn scratch_function_module_shape() {
    let src = r#"
interface FunctionModule<DB, TB extends keyof DB> {
    agg: TB & string;
    plain: TB;
}
declare function createFunctionModule<DB, TB extends keyof DB>(): FunctionModule<DB, TB>;
interface Container<DB> {
    fn: FunctionModule<DB, keyof DB>;
}
class Impl<DB> implements Container<DB> {
    get fn(): FunctionModule<DB, keyof DB> {
        return createFunctionModule();
    }
}
"#;
    let diags = messages(src);
    assert!(diags.is_empty(), "PROBE messages: {diags:#?}");
}

#[test]
fn scratch_bare_function_no_interface() {
    let src = r#"
interface FunctionModule<DB, TB extends keyof DB> {
    agg: TB & string;
    plain: TB;
}
declare function createFunctionModule<DB, TB extends keyof DB>(): FunctionModule<DB, TB>;
function make<DB>(): FunctionModule<DB, keyof DB> {
    return createFunctionModule();
}
"#;
    let diags = messages(src);
    assert!(diags.is_empty(), "PROBE bare-fn messages: {diags:#?}");
}

#[test]
fn scratch_single_member_no_interface() {
    let src = r#"
interface FunctionModule<DB, TB extends keyof DB> {
    agg: TB & string;
}
declare function createFunctionModule<DB, TB extends keyof DB>(): FunctionModule<DB, TB>;
function make<DB>(): FunctionModule<DB, keyof DB> {
    return createFunctionModule();
}
"#;
    let diags = messages(src);
    assert!(diags.is_empty(), "PROBE single-member messages: {diags:#?}");
}

#[test]
fn scratch_implements_no_intersection_member() {
    let src = r#"
interface FunctionModule<DB, TB extends keyof DB> {
    plain: TB;
}
declare function createFunctionModule<DB, TB extends keyof DB>(): FunctionModule<DB, TB>;
interface Container<DB> {
    fn: FunctionModule<DB, keyof DB>;
}
class Impl<DB> implements Container<DB> {
    get fn(): FunctionModule<DB, keyof DB> {
        return createFunctionModule();
    }
}
"#;
    let diags = messages(src);
    assert!(
        diags.is_empty(),
        "PROBE no-intersection messages: {diags:#?}"
    );
}

#[test]
fn scratch_implements_no_call_plain_field() {
    let src = r#"
interface FunctionModule<DB, TB extends keyof DB> {
    agg: TB & string;
}
interface Container<DB> {
    fn: FunctionModule<DB, keyof DB>;
}
declare class Impl<DB> implements Container<DB> {
    fn: FunctionModule<DB, keyof DB>;
}
"#;
    let diags = messages(src);
    assert!(
        diags.is_empty(),
        "PROBE no-call-plain-field messages: {diags:#?}"
    );
}

#[test]
fn scratch_implements_no_intersection_member_renamed_inner_db() {
    // Same as scratch_implements_no_intersection_member, but FunctionModule's
    // OWN first type parameter is renamed from "DB" to "Database" so it no
    // longer collides (by name) with Container's/Impl's own "DB". If this
    // passes while the "DB"-named sibling fails, the bug is a name-keyed
    // type-parameter collision across nesting levels, not a keyof/never
    // evaluation defect in general.
    let src = r#"
interface FunctionModule<Database, TB extends keyof Database> {
    plain: TB;
}
declare function createFunctionModule<Database, TB extends keyof Database>(): FunctionModule<Database, TB>;
interface Container<DB> {
    fn: FunctionModule<DB, keyof DB>;
}
class Impl<DB> implements Container<DB> {
    get fn(): FunctionModule<DB, keyof DB> {
        return createFunctionModule();
    }
}
"#;
    let diags = messages(src);
    assert!(
        diags.is_empty(),
        "PROBE renamed-inner-db messages: {diags:#?}"
    );
}

#[test]
fn scratch_implements_renamed_container_db() {
    // Same as scratch_implements_no_intersection_member, but Container's own
    // type parameter is renamed to "C" so only Impl's class param and
    // FunctionModule's own param are named "DB".
    let src = r#"
interface FunctionModule<DB, TB extends keyof DB> {
    plain: TB;
}
declare function createFunctionModule<DB, TB extends keyof DB>(): FunctionModule<DB, TB>;
interface Container<C> {
    fn: FunctionModule<C, keyof C>;
}
class Impl<DB> implements Container<DB> {
    get fn(): FunctionModule<DB, keyof DB> {
        return createFunctionModule();
    }
}
"#;
    let diags = messages(src);
    assert!(
        diags.is_empty(),
        "PROBE renamed-container-db messages: {diags:#?}"
    );
}

#[test]
fn scratch_direct_assign_no_call() {
    // No generic-call inference at all: just assign a bare Application whose
    // TB argument is annotated as `keyof DB` against the same interface.
    let src = r#"
interface FunctionModule<DB, TB extends keyof DB> {
    agg: TB & string;
}
declare const x: FunctionModule<{ a: 1 }, "a">;
const y: FunctionModule<{ a: 1 }, keyof { a: 1 }> = x;
"#;
    let diags = messages(src);
    assert!(diags.is_empty(), "PROBE direct-assign messages: {diags:#?}");
}
