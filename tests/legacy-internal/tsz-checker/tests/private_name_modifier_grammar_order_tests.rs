//! `TS18010`/`TS18019` on class members named by a private identifier.
//!
//! `tsc` reports both codes from `checkGrammarModifiers`, which walks a
//! member's modifier list in source order and returns at the first error. Two
//! consequences drive every expectation below, and each was pinned against
//! `typescript@7.0.2`:
//!
//! 1. **At most one** private-identifier diagnostic per member, anchored at the
//!    first modifier that conflicts — `declare abstract #x` reports one
//!    `TS18019`, not two.
//! 2. A modifier error that `tsc` reaches **earlier in the walk** preempts the
//!    private-identifier check entirely: container-abstractness
//!    (`TS1244`/`TS1253`), modifier ordering (`TS1029`) and modifier pairs
//!    (`TS1243`) all win when they come first.
//!
//! Member kind is not part of the rule. A private-named method or accessor
//! carrying `abstract` reports `TS18019` exactly as a property does.

use crate::test_utils::check_source_diagnostics;

fn codes(source: &str) -> Vec<u32> {
    check_source_diagnostics(source)
        .iter()
        .map(|d| d.code)
        .collect()
}

fn count(source: &str, code: u32) -> usize {
    codes(source).iter().filter(|&&c| c == code).count()
}

/// Underlined source text of the single diagnostic with `code`.
fn anchor(source: &str, code: u32) -> String {
    let diags = check_source_diagnostics(source);
    let matching: Vec<_> = diags.iter().filter(|d| d.code == code).collect();
    assert_eq!(
        matching.len(),
        1,
        "expected exactly one TS{code}, got: {diags:#?}"
    );
    let d = matching[0];
    source[d.start as usize..(d.start + d.length) as usize].to_string()
}

const TS18010: u32 = 18010;
const TS18019: u32 = 18019;
const TS1244: u32 = 1244;
const TS1253: u32 = 1253;

// --- member kind is not part of the rule -----------------------------------

#[test]
fn abstract_on_private_named_method_reports_ts18019() {
    let source = "abstract class C { abstract #m(): void; }\n";
    assert_eq!(count(source, TS18019), 1, "codes: {:?}", codes(source));
    assert_eq!(anchor(source, TS18019), "abstract");
}

#[test]
fn abstract_on_private_named_get_accessor_reports_ts18019() {
    let source = "abstract class C { abstract get #x(): number; }\n";
    assert_eq!(count(source, TS18019), 1, "codes: {:?}", codes(source));
}

#[test]
fn abstract_on_private_named_set_accessor_reports_ts18019() {
    let source = "abstract class C { abstract set #x(v: number); }\n";
    assert_eq!(count(source, TS18019), 1, "codes: {:?}", codes(source));
}

#[test]
fn abstract_on_private_named_auto_accessor_reports_ts18019() {
    let source = "abstract class C { abstract accessor #x: number; }\n";
    assert_eq!(count(source, TS18019), 1, "codes: {:?}", codes(source));
}

#[test]
fn each_abstract_private_named_member_reports_its_own_ts18019() {
    let source = "abstract class C {\n    abstract #m(): void;\n    abstract get #g(): number;\n    abstract set #s(v: number);\n    abstract #p: number;\n}\n";
    assert_eq!(count(source, TS18019), 4, "codes: {:?}", codes(source));
}

/// Anti-hardcoding cover: the rule is structural, so renaming every binder
/// (class and members alike) must not change the count.
#[test]
fn renamed_binders_report_the_same_ts18019_count() {
    let source = "abstract class Renamed {\n    abstract #alpha(): void;\n    abstract get #bravo(): number;\n}\n";
    assert_eq!(count(source, TS18019), 2, "codes: {:?}", codes(source));
}

// --- an earlier error in the walk preempts the private-identifier check -----

#[test]
fn abstract_private_named_property_in_non_abstract_class_reports_only_ts1253() {
    let source = "class C { abstract #x: number; }\n";
    assert_eq!(count(source, TS1253), 1, "codes: {:?}", codes(source));
    assert_eq!(
        count(source, TS18019),
        0,
        "TS1253 preempts TS18019; codes: {:?}",
        codes(source)
    );
}

#[test]
fn abstract_private_named_method_in_non_abstract_class_reports_only_ts1244() {
    let source = "class C { abstract #m(): void; }\n";
    assert_eq!(count(source, TS1244), 1, "codes: {:?}", codes(source));
    assert_eq!(count(source, TS18019), 0, "codes: {:?}", codes(source));
}

#[test]
fn abstract_private_named_auto_accessor_in_non_abstract_class_reports_only_ts1253() {
    let source = "class C { abstract accessor #x: number; }\n";
    assert_eq!(count(source, TS1253), 1, "codes: {:?}", codes(source));
    assert_eq!(count(source, TS18019), 0, "codes: {:?}", codes(source));
}

#[test]
fn static_written_before_abstract_preempts_ts18019() {
    // `static abstract` is TS1243, reported at `abstract`, and tsc returns
    // there. The mirror spelling below still reports TS18019.
    let source = "abstract class C { static abstract #x: number; }\n";
    assert_eq!(count(source, TS18019), 0, "codes: {:?}", codes(source));
}

#[test]
fn abstract_written_before_static_still_reports_ts18019() {
    let source = "abstract class C { abstract static #x: number; }\n";
    assert_eq!(count(source, TS18019), 1, "codes: {:?}", codes(source));
    assert_eq!(anchor(source, TS18019), "abstract");
}

#[test]
fn accessibility_written_before_abstract_reports_only_ts18010() {
    let source = "abstract class C { private abstract #x: number; }\n";
    assert_eq!(count(source, TS18010), 1, "codes: {:?}", codes(source));
    assert_eq!(
        count(source, TS18019),
        0,
        "TS18010 fires first and returns; codes: {:?}",
        codes(source)
    );
}

#[test]
fn accessibility_written_after_static_is_preempted_by_the_ordering_error() {
    // `static public #x` is TS1029 ("'public' modifier must precede 'static'"),
    // which tsc reports instead of TS18010.
    let source = "class C { static public #x = 1; }\n";
    assert_eq!(count(source, TS18010), 0, "codes: {:?}", codes(source));
}

#[test]
fn declare_on_a_private_named_method_is_preempted_by_ts1031() {
    // `declare` is not a valid modifier on a method at all, so tsc reports
    // TS1031 rather than TS18019.
    let source = "class C { declare #m(): void; }\n";
    assert_eq!(count(source, TS18019), 0, "codes: {:?}", codes(source));
}

// --- only the first conflicting modifier reports ---------------------------

#[test]
fn declare_before_abstract_reports_one_ts18019_anchored_at_declare() {
    let source = "abstract class C { declare abstract #x: number; }\n";
    assert_eq!(count(source, TS18019), 1, "codes: {:?}", codes(source));
    assert_eq!(anchor(source, TS18019), "declare");
}

#[test]
fn abstract_before_declare_reports_one_ts18019_anchored_at_abstract() {
    let source = "abstract class C { abstract declare #x: number; }\n";
    assert_eq!(count(source, TS18019), 1, "codes: {:?}", codes(source));
    assert_eq!(anchor(source, TS18019), "abstract");
}

// --- rows that already matched tsc, held unchanged -------------------------

#[test]
fn declare_on_a_private_named_property_still_reports_ts18019() {
    let source = "class C { declare #x: number; }\n";
    assert_eq!(count(source, TS18019), 1, "codes: {:?}", codes(source));
    assert_eq!(anchor(source, TS18019), "declare");
}

#[test]
fn accessibility_on_private_named_members_still_reports_ts18010() {
    for source in [
        "class C { public #x = 1; }\n",
        "class C { private #x = 1; }\n",
        "class C { protected #x = 1; }\n",
        "class C { private #m() {} }\n",
        "class C { public get #x() { return 1; } }\n",
        "class C { protected accessor #x = 1; }\n",
    ] {
        assert_eq!(
            count(source, TS18010),
            1,
            "expected one TS18010 for {source:?}, codes: {:?}",
            codes(source)
        );
    }
}

// --- negative controls ------------------------------------------------------

#[test]
fn abstract_members_with_ordinary_names_report_nothing_from_this_family() {
    let source = "abstract class C {\n    abstract m(): void;\n    abstract get g(): number;\n    abstract p: number;\n}\n";
    assert_eq!(count(source, TS18019), 0, "codes: {:?}", codes(source));
    assert_eq!(count(source, TS18010), 0, "codes: {:?}", codes(source));
}

#[test]
fn private_named_members_without_conflicting_modifiers_stay_clean() {
    let source = "class C {\n    #x = 1;\n    static #y = 2;\n    readonly #z = 3;\n    accessor #a = 4;\n    #m() {}\n    get #g() { return 1; }\n}\n";
    assert_eq!(count(source, TS18019), 0, "codes: {:?}", codes(source));
    assert_eq!(count(source, TS18010), 0, "codes: {:?}", codes(source));
}

#[test]
fn private_names_in_an_ambient_class_without_declare_modifiers_stay_clean() {
    // The members carry no `declare` modifier of their own; the containing
    // class being ambient must not synthesize one.
    let source = "declare class C {\n    #x: number;\n    #m(): void;\n    get #g(): boolean;\n}\n";
    assert_eq!(count(source, TS18019), 0, "codes: {:?}", codes(source));
}

#[test]
fn explicit_declare_on_a_private_name_inside_an_ambient_class_reports_ts18019() {
    let source = "declare class C { declare #x: number; }\n";
    assert_eq!(count(source, TS18019), 1, "codes: {:?}", codes(source));
}
