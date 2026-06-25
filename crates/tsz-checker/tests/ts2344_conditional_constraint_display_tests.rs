//! Regression tests for #14786: a failing type-parameter constraint that is a
//! conditional type must render the *evaluated* (reduced) branch in the TS2344
//! message once the conditional's check/extends types are concrete, matching
//! tsc.
//!
//! Structural rule: tsc instantiates a conditional type-parameter constraint
//! eagerly, so a constraint like `number extends string ? number[] : number`
//! (after type-argument substitution it has no remaining type parameters)
//! reduces to its single branch (`number`) before being formatted. tsz used to
//! keep the conditional deferred and print the raw, unevaluated conditional.
//! The relation CHECK was already correct — the right argument was rejected at
//! the right site — so this is a display-only divergence. The fix lives in the
//! TS2344 constraint-violation diagnostic path, which now formats the evaluated
//! constraint when a conditional constraint reduces to a non-conditional type.
//!
//! A genuinely deferred conditional (free type parameters remain) and every
//! non-conditional constraint (aliases, unions, object shapes) keep their
//! original display, so alias names survive.

fn ts2344_messages(source: &str) -> Vec<String> {
    tsz_checker::test_utils::check_source_code_messages(source)
        .into_iter()
        .filter(|(code, _)| *code == 2344)
        .map(|(_, msg)| msg)
        .collect()
}

#[test]
fn conditional_constraint_false_branch_reduces_to_primitive() {
    // `number extends string` is false → constraint reduces to `number`.
    let messages = ts2344_messages(
        r#"
type F<U extends (number extends string ? number[] : number)> = U;
type Bad = F<string>;
"#,
    );
    assert_eq!(messages.len(), 1, "expected one TS2344, got: {messages:?}");
    assert_eq!(
        messages[0],
        "Type 'string' does not satisfy the constraint 'number'."
    );
}

#[test]
fn conditional_constraint_true_branch_reduces_to_array() {
    // `string extends string` is true → constraint reduces to `string[]`.
    let messages = ts2344_messages(
        r#"
type F<U extends (string extends string ? string[] : number)> = U;
type Bad = F<number>;
"#,
    );
    assert_eq!(messages.len(), 1, "expected one TS2344, got: {messages:?}");
    assert_eq!(
        messages[0],
        "Type 'number' does not satisfy the constraint 'string[]'."
    );
}

#[test]
fn conditional_constraint_reduces_after_type_argument_substitution() {
    // After substituting `T = string`, the constraint
    // `T extends string ? Box<T> : T` becomes the concrete conditional
    // `string extends string ? Box<string> : string` → `Box<string>`.
    let messages = ts2344_messages(
        r#"
interface Box<T> { value: T; }
type G<T, U extends (T extends string ? Box<T> : T)> = U;
type Bad = G<string, string>;
"#,
    );
    assert_eq!(messages.len(), 1, "expected one TS2344, got: {messages:?}");
    assert_eq!(
        messages[0],
        "Type 'string' does not satisfy the constraint 'Box<string>'."
    );
}

#[test]
fn nested_conditional_constraint_reduces_to_single_branch() {
    // Nested conditional, both levels concrete → reduces to `"yes"`.
    let messages = ts2344_messages(
        r#"
type H<U extends (1 extends number ? (2 extends number ? "yes" : "no") : "x")> = U;
type Bad = H<number>;
"#,
    );
    assert_eq!(messages.len(), 1, "expected one TS2344, got: {messages:?}");
    assert_eq!(
        messages[0],
        "Type 'number' does not satisfy the constraint '\"yes\"'."
    );
}

#[test]
fn non_conditional_alias_constraint_keeps_alias_name() {
    // Negative control: a non-conditional alias constraint must keep its alias
    // name (`Keys`), proving the reduction is scoped to conditional constraints
    // and does not expand or rewrite other constraint kinds.
    let messages = ts2344_messages(
        r#"
type Keys = "a" | "b";
type K<X extends Keys> = X;
type Bad = K<boolean>;
"#,
    );
    assert_eq!(messages.len(), 1, "expected one TS2344, got: {messages:?}");
    assert!(
        messages[0].contains("'Keys'"),
        "plain alias constraint should display `Keys`, got: {:?}",
        messages[0]
    );
}
