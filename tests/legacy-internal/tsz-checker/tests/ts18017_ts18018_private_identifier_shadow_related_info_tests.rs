//! Regression tests for the `TS18017`/`TS18018` related-info pointers `tsc`
//! attaches to `TS18014` ("The property '#x' cannot be accessed on type 'Y'
//! within this class because it is shadowed by another private identifier
//! with the same spelling.").
//!
//! Structural rule (pinned against `typescript@7.0.2`): `tsc`'s
//! `checkPrivateIdentifierPropertyAccess` unconditionally attaches two
//! `relatedInformation` entries once it reports `TS18014` — `TS18017` at the
//! *closest lexically-scoped* `#name` declaration (the one that shadows the
//! access), and `TS18018` at the outer `#name` declaration actually present
//! on the accessed object's type (the one "probably intended"). Both anchor
//! on the private-identifier name token alone (`#x`, not the whole member),
//! and both are `RelatedInformationKind::LocationPointer`, so `--pretty
//! false` output is unchanged — this class of fix is corpus-neutral by
//! construction (see #16338).
//!
//! tsz owns this in `report_private_identifier_shadowed`
//! (`state/type_analysis/computed_helpers_binding.rs`), reached from the
//! shadow-detection branch in `computed_helpers_private.rs`.

use crate::diagnostics::Diagnostic;
use crate::test_utils::check_source_diagnostics;
use tsz_common::diagnostics::diagnostic_codes;

const TS18014: u32 = diagnostic_codes::THE_PROPERTY_CANNOT_BE_ACCESSED_ON_TYPE_WITHIN_THIS_CLASS_BECAUSE_IT_IS_SHADOWED;
const TS18017: u32 = diagnostic_codes::THE_SHADOWING_DECLARATION_OF_IS_DEFINED_HERE;
const TS18018: u32 =
    diagnostic_codes::THE_DECLARATION_OF_THAT_YOU_PROBABLY_INTENDED_TO_USE_IS_DEFINED_HERE;

fn only(diags: &[Diagnostic], code: u32) -> Diagnostic {
    let matching: Vec<_> = diags.iter().filter(|d| d.code == code).collect();
    assert_eq!(
        matching.len(),
        1,
        "expected exactly one TS{code}; got {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
    matching[0].clone()
}

fn related_pointer(
    diagnostic: &Diagnostic,
    code: u32,
) -> tsz_common::diagnostics::DiagnosticRelatedInformation {
    let matching: Vec<_> = diagnostic
        .related_information
        .iter()
        .filter(|info| info.code == code)
        .collect();
    assert_eq!(
        matching.len(),
        1,
        "expected exactly one TS{code} related pointer; got {:?}",
        diagnostic
            .related_information
            .iter()
            .map(|info| (info.code, info.message_text.clone()))
            .collect::<Vec<_>>()
    );
    matching[0].clone()
}

/// Method-declared private identifiers: nested `Derived` shadows outer `Base`.
#[test]
fn method_shadow_carries_both_pointers() {
    let source = r#"
class Base {
    #x() { };
    constructor() {
        class Derived {
            #x() { };
            testBase(x: Base) {
                x.#x;
            }
        }
    }
}
"#;
    let diagnostics = check_source_diagnostics(source);
    let ts18014 = only(&diagnostics, TS18014);
    assert_eq!(
        ts18014.message_text,
        "The property '#x' cannot be accessed on type 'Base' within this class because it is shadowed by another private identifier with the same spelling."
    );

    let shadowing = related_pointer(&ts18014, TS18017);
    assert_eq!(
        shadowing.message_text,
        "The shadowing declaration of '#x' is defined here"
    );
    assert_eq!(
        shadowing.length, 2,
        "anchor must be '#x' alone, not the whole member"
    );
    let derived_decl_start = source.find("#x() { };\n            testBase").unwrap() as u32;
    assert_eq!(shadowing.start, derived_decl_start);

    let intended = related_pointer(&ts18014, TS18018);
    assert_eq!(
        intended.message_text,
        "The declaration of '#x' that you probably intended to use is defined here"
    );
    assert_eq!(intended.length, 2);
    let base_decl_start = source.find("#x() { };\n    constructor").unwrap() as u32;
    assert_eq!(intended.start, base_decl_start);
}

/// Field-declared (not method) private identifiers take the same two
/// pointers, anchored on the field's own name.
#[test]
fn property_field_shadow_carries_both_pointers() {
    let source = r#"
class Base {
    #x = 1;
    constructor() {
        class Derived {
            #x = 2;
            testBase(x: Base) {
                x.#x;
            }
        }
    }
}
"#;
    let diagnostics = check_source_diagnostics(source);
    let ts18014 = only(&diagnostics, TS18014);
    let shadowing = related_pointer(&ts18014, TS18017);
    assert_eq!(shadowing.length, 2);
    let intended = related_pointer(&ts18014, TS18018);
    assert_eq!(intended.length, 2);
}

/// An outer *static* member shadowed by an inner *instance* member of the
/// same spelling: `private_member_declaring_type`'s static/instance split
/// must not stop the intended-declaration lookup from finding the outer
/// static field's own name node.
#[test]
fn static_outer_shadowed_by_instance_inner_carries_both_pointers() {
    let source = r#"
class Base {
    static #x() { };
    constructor() {
        class Derived {
            #x() { };
            testBase(x: typeof Base) {
                x.#x;
            }
        }
    }
}
"#;
    let diagnostics = check_source_diagnostics(source);
    let ts18014 = only(&diagnostics, TS18014);
    assert_eq!(
        ts18014.message_text,
        "The property '#x' cannot be accessed on type 'typeof Base' within this class because it is shadowed by another private identifier with the same spelling."
    );
    let shadowing = related_pointer(&ts18014, TS18017);
    assert_eq!(shadowing.length, 2);
    let intended = related_pointer(&ts18014, TS18018);
    assert_eq!(intended.length, 2);
    let base_decl_start = source.find("#x() { };\n    constructor").unwrap() as u32;
    assert_eq!(intended.start, base_decl_start);
}

/// A plain, unshadowed private-identifier access reports neither TS18014 nor
/// either pointer — the negative control.
#[test]
fn unshadowed_private_access_carries_no_pointers() {
    let source = r#"
class Base {
    #x() { };
    test() {
        this.#x();
    }
}
"#;
    let diagnostics = check_source_diagnostics(source);
    assert!(
        diagnostics
            .iter()
            .all(|d| d.code != TS18014 && d.code != TS18017 && d.code != TS18018),
        "expected no TS18014/TS18017/TS18018 for an unshadowed access; got {:?}",
        diagnostics
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}
