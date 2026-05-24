//! The left operand of `in` must be assignable to the property-key space
//! `string | number | symbol` (the keys `in` probes). When it is not, tsc emits
//! TS2322 at the LHS with the key union rendered structurally (it strips the
//! `PropertyKey` alias from this internal target). These tests pin both the
//! diagnostic firing and its rendered shape, and guard the valid-key controls.
//!
//! Structural rule: when `lhs in rhs` and `lhs` (after stripping nullish) is not
//! assignable to `string | number | symbol`, emit TS2322; valid key types
//! (`string`/`number`/`symbol` and their literals, plus `any`) produce no error.

use tsz_checker::diagnostics::diagnostic_codes;
use tsz_checker::test_utils::check_source_code_messages;

const TS2322: u32 = diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE;

fn in_lhs_errors(source: &str) -> Vec<(u32, String)> {
    check_source_code_messages(source)
        .into_iter()
        .filter(|(code, _)| *code == TS2322)
        .collect()
}

#[test]
fn boolean_lhs_reports_ts2322_with_expanded_key_union() {
    let diags = in_lhs_errors("declare const o: object;\nconst x = true in o;\n");
    assert_eq!(
        diags,
        vec![(
            TS2322,
            "Type 'boolean' is not assignable to type 'string | number | symbol'.".to_string()
        )],
        "expected the canonical key union expanded, not `PropertyKey`"
    );
}

#[test]
fn object_literal_lhs_reports_ts2322() {
    let diags = in_lhs_errors("declare const o: object;\nconst y = {} in o;\n");
    assert_eq!(
        diags,
        vec![(
            TS2322,
            "Type '{}' is not assignable to type 'string | number | symbol'.".to_string()
        )]
    );
}

#[test]
fn incompatible_union_lhs_reports_ts2322() {
    // `cond ? "k" : true` widens to `string | boolean` against the non-literal
    // target; the `boolean` member is the offending key type.
    let diags = in_lhs_errors(
        "declare const o: object;\nconst u = (Math.random() > 0.5 ? \"k\" : true) in o;\n",
    );
    assert_eq!(diags.len(), 1, "expected one TS2322, got {diags:#?}");
    assert_eq!(diags[0].0, TS2322);
    assert!(
        diags[0]
            .1
            .ends_with("is not assignable to type 'string | number | symbol'."),
        "unexpected message: {}",
        diags[0].1
    );
}

#[test]
fn bigint_lhs_reports_ts2322() {
    // bigint is not a property key; this is an adjacent case to the reported repro.
    let diags = in_lhs_errors("declare const o: object;\nconst b = 1n in o;\n");
    assert_eq!(
        diags,
        vec![(
            TS2322,
            "Type 'bigint' is not assignable to type 'string | number | symbol'.".to_string()
        )]
    );
}

#[test]
fn unknown_lhs_reports_ts2322() {
    let diags =
        in_lhs_errors("declare const o: object;\ndeclare const u: unknown;\nconst x = u in o;\n");
    assert_eq!(
        diags,
        vec![(
            TS2322,
            "Type 'unknown' is not assignable to type 'string | number | symbol'.".to_string()
        )]
    );
}

#[test]
fn unconstrained_type_parameter_lhs_reports_ts2322() {
    // An unconstrained `T` is not provably a key type.
    let diags = in_lhs_errors("declare const o: object;\nfunction f<T>(x: T) { return x in o; }\n");
    assert_eq!(
        diags,
        vec![(
            TS2322,
            "Type 'T' is not assignable to type 'string | number | symbol'.".to_string()
        )]
    );
}

#[test]
fn renamed_class_and_interface_still_reports() {
    // The rule is structural, not name-specific (issue boundary control).
    let diags =
        in_lhs_errors("interface Mover {}\ndeclare const car: object;\nconst e = true in car;\n");
    assert_eq!(diags.len(), 1, "expected one TS2322, got {diags:#?}");
}

// ---- Negative controls: valid key LHS produces no TS2322. ----

#[test]
fn string_literal_lhs_is_valid() {
    assert!(in_lhs_errors("declare const o: object;\nconst s = \"k\" in o;\n").is_empty());
}

#[test]
fn number_literal_lhs_is_valid() {
    assert!(in_lhs_errors("declare const o: object;\nconst n = 5 in o;\n").is_empty());
}

#[test]
fn symbol_lhs_is_valid() {
    assert!(in_lhs_errors("declare const o: object;\nconst s = Symbol() in o;\n").is_empty());
}

#[test]
fn any_lhs_is_valid() {
    assert!(in_lhs_errors("declare const o: object;\nconst a = (0 as any) in o;\n").is_empty());
}

#[test]
fn string_typed_variable_lhs_is_valid() {
    assert!(
        in_lhs_errors("declare const o: object;\ndeclare const k: string;\nconst a = k in o;\n")
            .is_empty()
    );
}

#[test]
fn constrained_type_parameter_lhs_is_valid() {
    assert!(
        in_lhs_errors(
            "declare const o: object;\nfunction g<T extends string>(x: T) { return x in o; }\n"
        )
        .is_empty()
    );
}

#[test]
fn nullable_key_lhs_is_not_spuriously_rejected() {
    // `string | undefined` strips its nullish part before the key check, matching
    // tsc's checkNonNullType, so no TS2322 from the key-type relation.
    assert!(
        in_lhs_errors(
            "declare const o: object;\ndeclare const k: string | undefined;\nconst a = k in o;\n"
        )
        .is_empty()
    );
}
