//! Tests for `useDefineForClassFields` erasure of no-initializer **static**
//! class fields.
//!
//! Structural rule: when a class field has no initializer **and** carries a
//! `static` modifier, `tsc`'s field-lowering (`transformPropertyWorker`) emits
//! no runtime statement for it:
//!
//! ```ts
//! if ((isPrivateIdentifier(name) || hasStaticModifier(property)) && !property.initializer) {
//!     return undefined;
//! }
//! ```
//!
//! So such a field is never materialized as
//! `Object.defineProperty(C, <name>, { ... value: void 0 })`. Emitting one
//! produced a define with an empty `value:` (invalid JS) and would clobber the
//! constructor function's own non-writable slots (`name`, `length`,
//! `prototype`, ...). A no-initializer **instance** field is unaffected and
//! still materializes with `value: void 0`. An initialized static field is also
//! unaffected and still emits its define.
//!
//! Witness: `staticPropertyNameConflicts(target=es2015,usedefineforclassfields=true)`.

#[path = "test_support.rs"]
mod test_support;

use test_support::parse_and_print_with_opts;
use tsz_emitter::output::printer::PrintOptions;

/// ES2015 (or ES5) options with `useDefineForClassFields` enabled, which is the
/// only configuration where bare no-init fields are lowered to define calls.
fn es2015_udfcf() -> PrintOptions {
    PrintOptions {
        use_define_for_class_fields: true,
        ..PrintOptions::es6()
    }
}

fn es5_udfcf() -> PrintOptions {
    PrintOptions {
        use_define_for_class_fields: true,
        ..PrintOptions::es5()
    }
}

/// No emit site may ever produce a `defineProperty` descriptor whose `value:`
/// is empty. This is the concrete invalid-JS defect the fix eliminates.
fn assert_no_empty_value(output: &str) {
    for line in output.lines() {
        let trimmed = line.trim_end();
        assert!(
            !(trimmed.ends_with("value:") || trimmed.ends_with("value: ")),
            "emitted a defineProperty descriptor with an empty `value:`\nOutput:\n{output}"
        );
    }
}

// ── Reported witness: no-init static field with a reserved Function name ──────

/// `static name;` (no initializer) conflicts with `Function.name`. `tsc` erases
/// the static define entirely; only the instance field materializes.
#[test]
fn static_no_init_reserved_name_field_is_erased() {
    let source = "class StaticName {\n    static name: number;\n    name: string;\n}\n";
    let output = parse_and_print_with_opts(source, es2015_udfcf());
    assert_no_empty_value(&output);
    // No static define for the conflicting static field.
    assert!(
        !output.contains("Object.defineProperty(StaticName, \"name\""),
        "no-init static field must not be materialized\nOutput:\n{output}"
    );
    // The instance field IS still materialized (`this`, not the class).
    assert!(
        output.contains("Object.defineProperty(this, \"name\""),
        "no-init instance field must still materialize with value: void 0\nOutput:\n{output}"
    );
    assert!(
        output.contains("value: void 0"),
        "instance field materializes value: void 0\nOutput:\n{output}"
    );
}

/// Same rule with a DIFFERENT Function own-property key (`length`). Proves the
/// rule is the static-no-init family, not the literal `name`.
#[test]
fn static_no_init_reserved_length_field_is_erased() {
    let source = "class StaticLength {\n    static length: number;\n    length: string;\n}\n";
    let output = parse_and_print_with_opts(source, es2015_udfcf());
    assert_no_empty_value(&output);
    assert!(
        !output.contains("Object.defineProperty(StaticLength, \"length\""),
        "no-init static `length` field must not be materialized\nOutput:\n{output}"
    );
    assert!(
        output.contains("Object.defineProperty(this, \"length\""),
        "no-init instance `length` field must still materialize\nOutput:\n{output}"
    );
}

/// The rule is keyed on `static` + `no initializer`, not on reserved names: a
/// non-conflicting user-chosen no-init static field is erased exactly the same
/// way (this is `tsc`'s general rule, line 2579). Renamed to prove no
/// name-string dependence.
#[test]
fn static_no_init_nonconflicting_user_name_field_is_erased() {
    let source = "class Widget {\n    static notReserved: number;\n    inst: string;\n}\n";
    let output = parse_and_print_with_opts(source, es2015_udfcf());
    assert_no_empty_value(&output);
    assert!(
        !output.contains("Object.defineProperty(Widget, \"notReserved\""),
        "no-init static field is erased regardless of name\nOutput:\n{output}"
    );
    // The instance field still materializes.
    assert!(
        output.contains("Object.defineProperty(this, \"inst\""),
        "no-init instance field still materializes\nOutput:\n{output}"
    );
}

// ── Computed-name variant: temp still captured, define still erased ───────────

/// A no-init static field with a *computed* name still has its computed-name
/// temp hoisted (matching `tsc`'s retained `_a = expr`), but the runtime define
/// is erased. Renamed key proves no name-string dependence.
#[test]
fn static_no_init_computed_name_field_erases_define_keeps_temp() {
    let source = "const Keys = { k: 'k' } as const;\nclass C {\n    static [Keys.k]: number;\n    [Keys.k]: string;\n}\n";
    let output = parse_and_print_with_opts(source, es2015_udfcf());
    assert_no_empty_value(&output);
    // The computed-name capture temp is still emitted (instance field needs it,
    // and tsc retains the static one too).
    assert!(
        output.contains("Keys.k"),
        "computed-name temp capture must be retained\nOutput:\n{output}"
    );
    // The instance field materializes via the captured temp on `this`.
    assert!(
        output.contains("Object.defineProperty(this, _"),
        "instance computed field materializes via temp\nOutput:\n{output}"
    );
    // No static-class define using the captured temp.
    assert!(
        !output.contains("Object.defineProperty(C, _"),
        "no-init static computed field define must be erased\nOutput:\n{output}"
    );
}

// ── Negative / fallback cases: must keep correct (non-erased) emission ────────

/// An *initialized* static field whose name conflicts with a Function own
/// property is NOT erased: it still emits its define with the real value.
/// Proves the predicate keys on initializer-presence, not on the name.
#[test]
fn static_initialized_reserved_name_field_is_kept() {
    let source = "class InitConflict {\n    static name = 123;\n}\n";
    let output = parse_and_print_with_opts(source, es2015_udfcf());
    assert_no_empty_value(&output);
    assert!(
        output.contains("Object.defineProperty(InitConflict, \"name\""),
        "initialized static field must still emit its define\nOutput:\n{output}"
    );
    assert!(
        output.contains("value: 123"),
        "initialized static field keeps its real value\nOutput:\n{output}"
    );
}

/// A no-init *instance* field with a reserved name is materialized (only the
/// `static` modifier triggers erasure). Negative control for the static rule.
#[test]
fn instance_no_init_reserved_name_field_is_materialized() {
    let source = "class OnlyInstance {\n    name: string;\n}\n";
    let output = parse_and_print_with_opts(source, es2015_udfcf());
    assert_no_empty_value(&output);
    assert!(
        output.contains("Object.defineProperty(this, \"name\""),
        "no-init instance field must materialize on `this`\nOutput:\n{output}"
    );
}

/// The same erasure holds at the ES5 target (different class-IIFE form), so the
/// fix is not target-specific. The ES5 emitter must not produce an empty-value
/// static define for a no-init static field.
#[test]
fn static_no_init_reserved_name_field_is_erased_es5() {
    let source = "class StaticName {\n    static name: number;\n    name: string;\n}\n";
    let output = parse_and_print_with_opts(source, es5_udfcf());
    assert_no_empty_value(&output);
    assert!(
        !output.contains("Object.defineProperty(StaticName, \"name\""),
        "no-init static field must not be materialized at ES5\nOutput:\n{output}"
    );
}
