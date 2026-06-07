//! Tests for TS2636 ("Type 'X<sub-T>' is not assignable to type 'X<super-T>'
//! as implied by variance annotation.") behavior on method-bivariant
//! parameter positions.
//!
//! Closes #5883. The structural rule:
//!
//! > tsc's variance model treats method parameters as bivariant — the
//! > position recorded as COVARIANT with `REJECTION_UNRELIABLE` set in
//! > the variance bitflag. TS2636 must only fire when the variance signal
//! > is RELIABLE (i.e., `REJECTION_UNRELIABLE` is clear), which happens
//! > when T appears at at least one strict (direct property / direct
//! > callback) position. Purely method-bivariant occurrences satisfy both
//! > `in T` and `out T` declarations and must not emit TS2636.

use tsz_checker::test_utils::{check_source_codes, check_source_diagnostics, diagnostic_count};

// =========================================================================
// Issue repro — purely method-bivariant T with `in` annotation
// =========================================================================

/// Direct repro from #5883. `interface Consumer<in T> { consume(T): void }`
/// uses T only in a method parameter (a bivariant position). Declared
/// contravariant `in T` is satisfied by bivariance, so no TS2636.
#[test]
fn in_annotation_with_method_param_only_no_ts2636() {
    let diags = check_source_codes("interface Consumer<in T> { consume(value: T): void }\n");
    assert!(
        !diags.contains(&2636),
        "interface Consumer<in T> {{ consume(T): void }} must not emit TS2636; got: {:?}",
        diags.to_vec(),
    );
}

/// Symmetric: `out T` on a purely method-bivariant occurrence should also
/// not fire TS2636.
#[test]
fn out_annotation_with_method_param_only_no_ts2636() {
    let diags = check_source_codes("interface Producer<out T> { produce(value: T): void }\n");
    assert!(
        !diags.contains(&2636),
        "interface Producer<out T> {{ produce(T): void }} must not emit TS2636; got: {:?}",
        diags.to_vec(),
    );
}

/// Anti-hardcoding (CLAUDE.md §25): the rule must not depend on the
/// spelling of the interface or method.
#[test]
fn method_bivariance_independent_of_names() {
    let diags = check_source_codes("interface Handler<in U> { handle(payload: U): void }\n");
    assert!(
        !diags.contains(&2636),
        "Different names but same shape must also pass; got: {:?}",
        diags.to_vec(),
    );
}

// =========================================================================
// Regression guards — TS2636 must still fire for reliable violations
// =========================================================================

/// `in T` (contravariant) but T appears at a direct COVARIANT property —
/// strict signal, TS2636 must fire.
#[test]
fn in_annotation_with_direct_covariant_property_emits_ts2636() {
    let diags = check_source_codes("interface Bad<in T> { value: T }\n");
    assert!(
        diags.contains(&2636),
        "interface Bad<in T> {{ value: T }} must still emit TS2636 (direct covariant position); got: {:?}",
        diags.to_vec(),
    );
}

/// `out T` but T appears at a direct contravariant callback position
/// (`(x: T) => void` as a property, not a method) — strict contravariant
/// signal, TS2636 must fire.
#[test]
fn out_annotation_with_direct_callback_param_emits_ts2636() {
    let diags = check_source_codes("interface Bad<out T> { callback: (value: T) => void }\n");
    assert!(
        diags.contains(&2636),
        "interface Bad<out T> {{ callback: (T) => void }} must still emit TS2636 \
         (function-property param is strict contravariant); got: {:?}",
        diags.to_vec(),
    );
}

/// Mixed: `in T` with both a direct property (strict covariant) AND a
/// method parameter (bivariant). The direct property's strict-covariant
/// signal pins the variance — TS2636 must fire.
#[test]
fn in_annotation_mixed_with_strict_property_emits_ts2636() {
    let diags = check_source_codes("interface MixedBad<in T> { value: T; consume(v: T): void }\n");
    assert!(
        diags.contains(&2636),
        "interface MixedBad<in T> with strict-covariant `value: T` must still emit TS2636; got: {:?}",
        diags.to_vec(),
    );
}

/// `in out T` (invariant) is always valid regardless of position.
#[test]
fn invariant_annotation_always_ok() {
    let diags =
        check_source_codes("interface AnyVariance<in out T> { value: T; consume(v: T): void }\n");
    assert!(
        !diags.contains(&2636),
        "in out T must not emit TS2636; got: {:?}",
        diags.to_vec(),
    );
}

#[test]
fn conflicting_merged_interface_annotations_are_invariant() {
    let source = r#"
interface Channel<out T> {}
interface Channel<in T> {}

declare let wide: Channel<unknown>;
declare let narrow: Channel<string>;

wide = narrow;
narrow = wide;
"#;

    let diags = check_source_diagnostics(source);

    assert_eq!(
        diagnostic_count(&diags, 2322),
        2,
        "out+in merged interface annotations must reject both assignment directions; got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn matching_merged_interface_annotations_keep_direction() {
    let covariant_source = r#"
interface Reader<out T> {}
interface Reader<out T> {}

declare let wide: Reader<unknown>;
declare let narrow: Reader<string>;

wide = narrow;
narrow = wide;
"#;
    let covariant_diags = check_source_diagnostics(covariant_source);
    assert_eq!(
        diagnostic_count(&covariant_diags, 2322),
        1,
        "out+out merged interface annotations must remain covariant; got: {:?}",
        covariant_diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );

    let contravariant_source = r#"
interface Sink<in T> {}
interface Sink<in T> {}

declare let wide: Sink<unknown>;
declare let narrow: Sink<string>;

wide = narrow;
narrow = wide;
"#;
    let contravariant_diags = check_source_diagnostics(contravariant_source);
    assert_eq!(
        diagnostic_count(&contravariant_diags, 2322),
        1,
        "in+in merged interface annotations must remain contravariant; got: {:?}",
        contravariant_diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}
