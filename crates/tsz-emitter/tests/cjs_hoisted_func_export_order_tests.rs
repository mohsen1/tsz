//! Regression tests for the emission order of hoisted CommonJS function
//! exports.
//!
//! A hoisted function declaration can be exported under several names at once:
//! its own `export` / `export default` modifier plus any number of separate
//! `export { fn as alias }` clauses. `tsc` emits all of these assignments in a
//! single preamble group (before the function body, because JS function
//! declarations hoist) and, crucially, in a fixed order dictated by
//! `appendExportsOfHoistedDeclaration`: the declaration's **own** export first,
//! then each clause alias in **specifier source order**.
//!
//! tsz previously sorted the group with an alphabetical tiebreaker, which put
//! `exports.bar = foo;` ahead of `exports.default = foo;` and reordered
//! `export { f as z, f as m }` to `m` before `z`. The fix keys the within-group
//! order on (own-first, then specifier source position) instead.
//!
//! Source: `crates/tsz-emitter/src/emitter/source_file/emit.rs`
//! (hoisted function export preamble).

use tsz_common::common::{ModuleKind, ScriptTarget};
use tsz_emitter::output::printer::PrintOptions;

#[path = "test_support.rs"]
mod test_support;

use test_support::parse_and_lower_print as parse_lower_emit;

fn cjs(target: ScriptTarget) -> PrintOptions {
    PrintOptions {
        target,
        module: ModuleKind::CommonJS,
        ..Default::default()
    }
}

/// Extract the ordered sequence of exported names from `exports.<name> = ...;`
/// assignment lines, ignoring `void 0` initializers and re-export bookkeeping.
fn export_assignment_names(output: &str) -> Vec<String> {
    output
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            let rest = line.strip_prefix("exports.")?;
            let (name, value) = rest.split_once(" = ")?;
            // Skip the `exports.x = void 0;` initialization preamble.
            if value.trim_start().starts_with("void 0") {
                return None;
            }
            Some(name.to_string())
        })
        .collect()
}

fn assert_order(source: &str, expected: &[&str]) {
    for target in [
        ScriptTarget::ES5,
        ScriptTarget::ES2015,
        ScriptTarget::ES2020,
    ] {
        let output = parse_lower_emit(source, cjs(target));
        let names = export_assignment_names(&output);
        assert_eq!(
            names, expected,
            "hoisted function export order mismatch at {target:?}.\nSource:\n{source}\nOutput:\n{output}"
        );
    }
}

/// `export default function` followed by aliases: `default` (the declaration's
/// own export) must come first, then the clause aliases in written order.
#[test]
fn default_function_then_named_reexports() {
    assert_order(
        "export default function foo() { return 1; }\nexport { foo as bar, foo as baz };\n",
        &["default", "bar", "baz"],
    );
}

/// An `export function` (own named export) precedes its `export { as }` alias,
/// even though `bar` sorts before `foo` alphabetically.
#[test]
fn named_function_then_alias() {
    assert_order(
        "export function foo() { return 1; }\nexport { foo as bar };\n",
        &["foo", "bar"],
    );
}

/// Clause aliases keep specifier source order (`zeta` before `alpha`), not
/// alphabetical order.
#[test]
fn alias_specifier_source_order_is_preserved() {
    assert_order(
        "export default function foo() {}\nexport { foo as zeta, foo as alpha };\n",
        &["default", "zeta", "alpha"],
    );
}

/// Two exported functions each carry their own group; aliases stay attached to
/// the function they reference, in declaration order.
#[test]
fn multiple_functions_group_with_their_aliases() {
    assert_order(
        "export function a() {}\nexport { a as z, a as m };\nexport function b() {}\nexport { b as y };\n",
        &["a", "z", "m", "b", "y"],
    );
}

/// `exports.j = j;` before `exports.jj = j;` — own export first, alias after
/// (this case is consistent under both alphabetical and source order, and must
/// keep working).
#[test]
fn own_export_before_alias_alphabetical_consistent() {
    assert_order(
        "export function j() {}\nexport { j as jj };\n",
        &["j", "jj"],
    );
}
