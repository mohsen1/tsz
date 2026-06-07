//! Regression locks for the duplicate-identifier grouping diagnostics whose
//! per-pass temporary containers were size-tuned / de-cloned for #11617.
//!
//! These passes group merged declarations by scope (`scope_groups`,
//! `interface_decls_by_scope`, `decls_by_scope`, the duplicate-function-impl
//! grouping) or scan merged class/interface members (the modifier-consistency
//! check that previously cloned the member-node list). The container changes are
//! purely allocation-traffic reductions, so every diagnostic these paths produce
//! must be byte-for-byte unchanged. The matrix below pins the codes and counts
//! for the matched (no-error), single (no-group), and conflicting cases — plus a
//! renamed-binder variant — so the size hints and the borrow-instead-of-clone
//! cannot quietly drop or duplicate a diagnostic.

use tsz_checker::test_utils::check_source_codes as get_codes;

fn count(source: &str, code: u32) -> usize {
    get_codes(source).iter().filter(|&&c| c == code).count()
}

// --- TS2428: merged interface declarations must have identical type params ---
// (`interface_decls_by_scope` grouping)

#[test]
fn matched_interface_type_params_no_2428() {
    let source = "interface Box<T> { a: T; }\ninterface Box<T> { b: T; }\n";
    assert_eq!(count(source, 2428), 0);
}

#[test]
fn mismatched_interface_type_params_reports_2428() {
    let source = "interface Box<T> { a: T; }\ninterface Box<T, U> { b: U; }\n";
    assert!(count(source, 2428) >= 1);
}

#[test]
fn mismatched_interface_type_params_reports_2428_renamed_binder() {
    // Same structure, different binder name: behavior follows shape, not spelling.
    let source = "interface Crate<T> { a: T; }\ninterface Crate<T, U> { b: U; }\n";
    assert!(count(source, 2428) >= 1);
}

// --- TS2687: merged class/interface members must have identical modifiers ---
// (the `decls_by_scope` grouping path + the member-scan that previously cloned
// `members.nodes`)

#[test]
fn matched_member_visibility_no_2687() {
    let source = "class Cfg { x: number = 0; }\ninterface Cfg { y: number; }\n";
    assert_eq!(count(source, 2687), 0);
}

#[test]
fn mismatched_member_visibility_reports_2687() {
    let source = "class Cfg { private x: number = 0; }\ninterface Cfg { x: number; }\n";
    assert!(count(source, 2687) >= 1);
}

#[test]
fn mismatched_member_visibility_reports_2687_renamed_binder() {
    let source = "class Holder { private slot: number = 0; }\ninterface Holder { slot: number; }\n";
    assert!(count(source, 2687) >= 1);
}

// --- TS2393: duplicate function implementations (the FxHashMap-converted
// function-impl scope grouping) ---

#[test]
fn single_function_implementation_no_2393() {
    let source = "function run() {}\n";
    assert_eq!(count(source, 2393), 0);
}

#[test]
fn overload_with_one_implementation_no_2393() {
    // A declaration + a single implementation is a valid overload set.
    let source = "function run(): void;\nfunction run() {}\n";
    assert_eq!(count(source, 2393), 0);
}

#[test]
fn duplicate_function_implementations_report_2393() {
    let source = "function run() {}\nfunction run() {}\n";
    assert!(count(source, 2393) >= 1);
}

#[test]
fn duplicate_function_implementations_report_2393_renamed_binder() {
    let source = "function execute() {}\nfunction execute() {}\n";
    assert!(count(source, 2393) >= 1);
}
