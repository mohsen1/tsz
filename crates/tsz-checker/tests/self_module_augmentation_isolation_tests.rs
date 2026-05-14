//! Self-module-augmentation isolation tests (regression for #6164).
//!
//! `declare module "./<self>" { interface Foo { ... } }` from a file that
//! also contains a file-scope `interface Foo` must NOT inject the
//! augmentation's members into the file-scope interface's type. tsc treats
//! the augmentation symbol table as independent from the augmenting file's
//! locals (the augmentation is merged into the *target* module's exports,
//! not into the augmenting file's own symbols).
//!
//! Structural rule (#6164):
//!
//! > When `declare module "X" { interface Foo { ... } }` appears in any
//! > file, the augmentation interface `Foo` is a separate symbol from any
//! > file-scope interface `Foo` in the augmenting file. Augmentation
//! > declarations of the same name within the same file (across one or
//! > more augmentation blocks of the same target module) merge with each
//! > other, but never with a non-augmentation declaration.

use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::diagnostic_codes;
use tsz_checker::test_utils::check_source;

fn diagnostics_for(source: &str) -> Vec<(u32, String)> {
    check_source(source, "test.ts", CheckerOptions::default())
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

#[test]
fn self_augmentation_after_local_interface_does_not_require_aug_members() {
    // Reproduction from #6164: the augmentation block follows the local
    // interface declarations in source order. tsc accepts the assignment;
    // tsz must too.
    let source = r#"
interface Merged {
    a: number;
}

interface Merged {
    b: string;
}

const m: Merged = { a: 1, b: "test" };

declare module "./test" {
    interface Merged {
        augmented: boolean;
    }
}

export {};
"#;

    let diags = diagnostics_for(source);
    let assignment_errors: Vec<_> = diags
        .iter()
        .filter(|(code, _)| {
            *code == diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE
                || *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                || *code == diagnostic_codes::OBJECT_LITERAL_MAY_ONLY_SPECIFY_KNOWN_PROPERTIES_AND_DOES_NOT_EXIST_IN_TYPE
        })
        .collect();
    assert!(
        assignment_errors.is_empty(),
        "self-augmentation must not require the augmented member on a local interface; got: {diags:#?}"
    );
}

#[test]
fn self_augmentation_before_local_interface_does_not_inject_members() {
    // Source order reversed: augmentation block precedes local interface
    // declarations. Same outcome — the local interface's type must reflect
    // only its own declarations.
    let source = r#"
declare module "./test" {
    interface Foo {
        b: string;
    }
}

interface Foo {
    a: number;
}

const y: Foo = { a: 1 };
console.log(y);

export {};
"#;

    let diags = diagnostics_for(source);
    let assignment_errors: Vec<_> = diags
        .iter()
        .filter(|(code, _)| {
            *code == diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE
                || *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
        })
        .collect();
    assert!(
        assignment_errors.is_empty(),
        "augmentation block before local interface must not inject members; got: {diags:#?}"
    );
}

#[test]
fn self_augmentation_added_members_are_not_accepted_on_local_interface() {
    // Conversely, augmentation members must not be silently accepted on a
    // local interface either: assigning an object literal that *includes*
    // the augmented property should fail because the local interface does
    // not have it.
    let source = r#"
interface Foo {
    a: number;
}

declare module "./test" {
    interface Foo {
        b: string;
    }
}

const y: Foo = { a: 1, b: "x" };
console.log(y);

export {};
"#;

    let diags = diagnostics_for(source);
    let unknown_prop: Vec<_> = diags
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::OBJECT_LITERAL_MAY_ONLY_SPECIFY_KNOWN_PROPERTIES_AND_DOES_NOT_EXIST_IN_TYPE)
        .collect();
    assert!(
        !unknown_prop.is_empty(),
        "extra augmentation-only property `b` must be flagged on the local interface; got: {diags:#?}"
    );
}

#[test]
fn self_augmentation_with_renamed_iteration_variable_is_still_separate() {
    // Same rule, distinct mapped-iteration name. The fix must not be
    // sensitive to the specific identifier chosen by the user.
    let source = r#"
interface Foo {
    a: number;
}

interface Foo {
    b: string;
}

const m: Foo = { a: 1, b: "test" };

declare module "./test" {
    interface Foo {
        c: boolean;
    }
}

export {};
"#;

    let diags = diagnostics_for(source);
    let assignment_errors: Vec<_> = diags
        .iter()
        .filter(|(code, _)| {
            *code == diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE
                || *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
        })
        .collect();
    assert!(
        assignment_errors.is_empty(),
        "augmentation isolation must hold regardless of the augmented property name; got: {diags:#?}"
    );
}

#[test]
fn self_augmentation_type_alias_does_not_displace_local_alias() {
    // The same architectural rule for type-alias augmentations: the local
    // `type X = string` must keep its declared type, with the augmentation
    // recorded separately for any future cross-file consumer.
    let source = r#"
type MyType = string;

declare module "./test" {
    type MyType = number;
}

const x: MyType = "hello";
console.log(x);

export {};
"#;

    let diags = diagnostics_for(source);
    let assignment_errors: Vec<_> = diags
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();
    assert!(
        assignment_errors.is_empty(),
        "self-augmentation of a type alias must not redefine the local alias; got: {diags:#?}"
    );
}
