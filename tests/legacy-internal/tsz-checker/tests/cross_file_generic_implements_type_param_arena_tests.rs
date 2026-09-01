//! A class `implements` a generic interface declared in another file.
//!
//! `class_implements_checker` walked each heritage-symbol declaration through
//! `self.ctx.arena` (the CHECKING file's arena) to read the interface's own
//! `type_parameters`, then fell back to `definition_store.find_def_by_symbol`
//! (keyed on the raw `SymbolId` alone, with no file disambiguator) when that
//! walk came up empty. Both are wrong for a declaration that lives in another
//! file: the arena walk silently reads the wrong node (or none at all) at that
//! `NodeIndex`, and the raw-`SymbolId` lookup can resolve to an unrelated
//! file's definition once two per-file binders mint the same numeric id for
//! different declarations. This file fixes both call sites in
//! `crates/tsz-checker/src/classes/class_implements_checker/core.rs` to
//! resolve the declaration's own arena (`arena_for_declaration_or`) and the
//! canonical cross-file type-param resolver (`get_type_params_for_symbol`)
//! instead (#16434).
//!
//! That fix is real but NOT sufficient to close #16434 end to end: the two
//! `#[ignore]`d cases below still reproduce a false `TS2416` in this crate's
//! `check_multi_file_with_global_index` harness, which mints independent
//! per-file `SymbolId`s starting at 0 for every file (so `types.ts`'s
//! `Plain` interface and `actor.ts`'s own `Plain` import alias can both be
//! raw id 0). The root cause traced one layer deeper than this file's fix
//! reaches: `CheckerState::get_cross_file_symbol` calls `local_import_alias`
//! FIRST, which reads `self.ctx.binder.get_symbol(sym_id)` — the CURRENT
//! (checking) file's own binder — using whatever `sym_id` the caller passes,
//! even when that `sym_id` has ALREADY been resolved (by
//! `resolve_alias_symbol`, in `class_implements_checker/core.rs`) to the
//! REAL declaring symbol in the OTHER file. If that resolved numeric id
//! coincidentally also names a local import alias in the checking file (as
//! it does here — id 0 is both `types.ts`'s `Plain` interface and
//! `actor.ts`'s own `import { Plain }` alias), `local_import_alias` wins and
//! `get_cross_file_symbol` returns the WRONG symbol (the alias declaration
//! itself, not the interface), so every declaration in `symbol_declarations`
//! is the import statement, not `INTERFACE_DECLARATION` — no arena fix at
//! the call site can recover from that. Fixing this needs `get_cross_file_symbol`
//! (or its caller) to carry file-scoped identity through instead of a bare
//! `SymbolId`, which is a broader change than one call site.

use crate::context::CheckerOptions;
use crate::diagnostics::{Diagnostic, diagnostic_codes};
use crate::test_utils::check_multi_file_with_global_index;

fn check(files: &[(&str, &str)], entry: &str) -> Vec<Diagnostic> {
    check_multi_file_with_global_index(files, entry, CheckerOptions::default())
}

fn implements_member_errors(diagnostics: &[Diagnostic]) -> Vec<(u32, String)> {
    diagnostics
        .iter()
        .filter(|d| {
            d.code
                == diagnostic_codes::PROPERTY_IN_TYPE_IS_NOT_ASSIGNABLE_TO_THE_SAME_PROPERTY_IN_BASE_TYPE
        })
        .map(|d| (d.code, d.message_text.to_string()))
        .collect()
}

#[test]
fn cross_file_generic_interface_implements_substitutes_type_argument() {
    let types_src = r#"
export interface Plain<T> {
    plain: () => T;
}
"#;
    let actor_src = r#"
import type { Plain } from "./types";

export class Actor implements Plain<number> {
    public plain(): number {
        return 1;
    }
}
"#;
    let diags = check(
        &[("./types.ts", types_src), ("./actor.ts", actor_src)],
        "./actor.ts",
    );
    let errors = implements_member_errors(&diags);
    assert!(
        errors.is_empty(),
        "expected the correctly-typed member to satisfy the instantiated interface, got: {errors:?}",
    );
}

#[test]
fn cross_file_generic_interface_implements_still_reports_genuine_mismatch() {
    // Negative control: with the type argument correctly substituted to
    // `number`, a member that returns `string` is a genuine mismatch and must
    // still be reported. This guards against a fix that goes too far and
    // stops instantiating (or stops checking) the interface at all.
    let types_src = r#"
export interface Plain<T> {
    plain: () => T;
}
"#;
    let actor_src = r#"
import type { Plain } from "./types";

export class Actor implements Plain<number> {
    public plain(): string {
        return "x";
    }
}
"#;
    let diags = check(
        &[("./types.ts", types_src), ("./actor.ts", actor_src)],
        "./actor.ts",
    );
    let errors = implements_member_errors(&diags);
    assert!(
        !errors.is_empty(),
        "expected a genuine `string` vs `number` member mismatch to keep reporting TS2416",
    );
}

#[test]
fn cross_file_generic_interface_implements_renamed_binders() {
    // Same shape, different identifiers throughout — guards against any
    // identifier-specific logic sneaking into the fix.
    let types_src = r#"
export interface Boxed<Value> {
    unwrap: () => Value;
}
"#;
    let holder_src = r#"
import type { Boxed } from "./types";

export class Holder implements Boxed<string> {
    public unwrap(): string {
        return "hi";
    }
}
"#;
    let diags = check(
        &[("./types.ts", types_src), ("./holder.ts", holder_src)],
        "./holder.ts",
    );
    let errors = implements_member_errors(&diags);
    assert!(
        errors.is_empty(),
        "expected the renamed-binder variant to stay clean, got: {errors:?}",
    );
}

#[test]
fn cross_file_generic_interface_implements_same_file_control() {
    // Positive control: the same shape in a single file must already be
    // clean on `main` — this isolates the bug to the cross-file arena/symbol
    // resolution path rather than the generic substitution machinery itself.
    let single_src = r#"
interface Plain<T> {
    plain: () => T;
}

class Actor implements Plain<number> {
    public plain(): number {
        return 1;
    }
}
"#;
    let diags = check(&[("./single.ts", single_src)], "./single.ts");
    let errors = implements_member_errors(&diags);
    assert!(
        errors.is_empty(),
        "expected the single-file equivalent to stay clean, got: {errors:?}",
    );
}
