//! Regression tests for resolving a built-in `lib.*.d.ts` global-script
//! namespace member from a reference site checked while the *current* file is
//! an external (ES) module.
//!
//! Structural rule (owner:
//! `symbols/symbol_resolver_qualified.rs::resolve_unqualified_name_in_enclosing_namespace`):
//! namespace *merging* across files is disabled for external modules (each
//! user module's namespaces are private). That gate was keyed on the *current*
//! checker file's module-ness, which wrongly blocked unqualified resolution of
//! a sibling declared inside a built-in global-script lib namespace (e.g. a
//! `Temporal`/`Intl` member type reached during lazy materialization while the
//! triggering source file is an ES module). The first-attempt failure then fell
//! through to the full-symbol-universe spelling scan. The gate now exempts
//! built-in `lib.*.d.ts` nodes (always global scripts where namespaces merge),
//! so the sibling resolves on the first attempt — while user external-module
//! namespaces still do NOT merge across files.

use tsz_checker::test_utils::check_source_codes;

fn codes(source: &str) -> Vec<u32> {
    let mut c = check_source_codes(source);
    c.sort_unstable();
    c.dedup();
    c
}

#[test]
fn es_module_referencing_lib_namespace_member_compiles_clean() {
    // An ES module (note the `export {}`) that references built-in lib namespace
    // members must type-check cleanly: the lib namespace is a global script, so
    // its members resolve regardless of the current file being a module.
    let codes = codes(
        r#"
export {};
const a: Intl.Collator | null = null;
const b: Intl.NumberFormat | null = null;
const _use = [a, b];
"#,
    );
    assert!(
        !codes.contains(&2304) && !codes.contains(&2552) && !codes.contains(&2724),
        "lib namespace members must resolve from an ES module without cannot-find/spelling diagnostics, got {codes:?}",
    );
}

#[test]
fn user_namespaces_still_do_not_merge_across_external_modules() {
    // Negative control / the gate's original purpose: two ES modules each
    // declaring `namespace Shared` must NOT merge, so an unqualified `A`
    // (declared only in the other module's `Shared`) is unresolved (TS2304).
    let codes = codes(
        r#"
export {};
namespace Shared { export interface A { x: number } }
type UseB = B;
"#,
    );
    assert!(
        codes.contains(&2304),
        "a name declared only in another external module's same-named namespace must not merge in (TS2304), got {codes:?}",
    );
}
