//! A position-invalid `import x = require("m")` inside a **declaration scope**
//! resolves its module specifier only when the alias is referenced.
//!
//! What reaches `resolveExternalModuleName` in tsc is `markAliasReferenced`, so the
//! discriminator is the *use*, not the position:
//!
//! ```text
//! function f() { import x = require("nope"); }      tsc: TS1232 alone
//! function f() { import x = require("nope"); x; }   tsc: TS1232 + TS2307
//! ```
//!
//! tsz already modelled this for a namespace body
//! (`namespace_import_alias_is_referenced`). A function body, a method body and a
//! class static block took a different path — an unqualified early return under the
//! comment *"tsc doesn't resolve require() inside functions"*. It does, when the
//! alias is referenced, so that arm **swallowed** a TS2307 tsc reports.
//!
//! The scope test is what keeps the rule narrow, and it is the half that is easy to
//! get backwards. A bare block, an `if` body and a loop body are **not** declaration
//! scopes: tsc resolves the specifier in all three *however the alias is used*, so
//! they must keep reporting TS2307 even when nothing references the alias. #16489
//! draws the same line for `export ... from`. Every one of those rows is pinned
//! below as a negative control, because a "suppress when unreferenced" rule applied
//! uniformly across containers passes the declaration-scope rows and silently breaks
//! these.
//!
//! Expectations were taken from `scripts/conformance/oracle.sh` — the pinned
//! `typescript@7.0.2` run with the same `--singleThreaded --stableTypeOrdering`
//! flags the conformance cache generator uses. Per #16413 that matters here: a bare
//! block's diagnostics differ between schedulers, so a plain `tsc file.ts` disagrees
//! with what the gate actually scores for exactly this family.

use crate::context::CheckerOptions;
use crate::state::CheckerState;
use tsz_binder::BinderState;
use tsz_common::common::{ModuleKind, ScriptTarget};
use tsz_parser::parser::ParserState;
use tsz_solver::construction::TypeInterner;

/// TS1232: An import declaration can only be used at the top level of a namespace or module.
const TS1232: u32 = 1232;
/// TS1147: Import declarations in a namespace cannot reference a module.
const TS1147: u32 = 1147;
/// TS1202: Import assignment cannot be used when targeting ECMAScript modules.
const TS1202: u32 = 1202;
/// TS2307: Cannot find module '...' or its corresponding type declarations.
const TS2307: u32 = 2307;

/// Check a single source with unresolved-import reporting on, so that a module
/// specifier naming a nonexistent module reports TS2307 if it is resolved at all.
fn check(source: &str) -> Vec<u32> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions {
            module: ModuleKind::ESNext,
            target: ScriptTarget::ESNext,
            ..CheckerOptions::default()
        },
    );
    checker.ctx.report_unresolved_imports = true;
    checker.check_source_file(root);
    let mut codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    codes.sort_unstable();
    codes
}

fn assert_codes(source: &str, expected: &[u32], what: &str) {
    let actual = check(source);
    assert_eq!(actual, expected, "{what}\nsource:\n{source}");
}

const DECL: &str = r#"import x = require("nonexistent-module");"#;

fn unreferenced(container: &str) -> String {
    container.replace("BODY", DECL)
}

fn referenced(container: &str) -> String {
    container.replace("BODY", &format!("{DECL}\nx;"))
}

// Declaration scopes: the referenced-alias rule applies.
const FUNCTION_BODY: &str = "function f() {\nBODY\n}";
const METHOD_BODY: &str = "class C { m() {\nBODY\n} }";
const STATIC_BLOCK: &str = "class C { static {\nBODY\n} }";
const NAMESPACE_BODY: &str = "namespace N {\nBODY\n}";

// Not declaration scopes: resolution runs regardless of use.
const BARE_BLOCK: &str = "{\nBODY\n}";
const IF_BLOCK: &str = "if (true) {\nBODY\n}";
const LOOP_BODY: &str = "for (;;) {\nBODY\n}";

// ---------------------------------------------------------------------------
// Declaration scope, alias unreferenced: the placement diagnostic alone.
// These already held before the fix; they are here to keep it that way.
// ---------------------------------------------------------------------------

#[test]
fn unreferenced_alias_in_function_body_does_not_resolve_the_specifier() {
    assert_codes(
        &unreferenced(FUNCTION_BODY),
        &[TS1232],
        "function body, alias never used",
    );
}

#[test]
fn unreferenced_alias_in_method_body_does_not_resolve_the_specifier() {
    assert_codes(
        &unreferenced(METHOD_BODY),
        &[TS1232],
        "method body, alias never used",
    );
}

#[test]
fn unreferenced_alias_in_static_block_does_not_resolve_the_specifier() {
    assert_codes(
        &unreferenced(STATIC_BLOCK),
        &[TS1232],
        "class static block, alias never used",
    );
}

#[test]
fn unreferenced_alias_in_namespace_body_does_not_resolve_the_specifier() {
    assert_codes(
        &unreferenced(NAMESPACE_BODY),
        &[TS1147],
        "namespace body, alias never used",
    );
}

// ---------------------------------------------------------------------------
// Declaration scope, alias referenced: resolution runs. This is the half the
// unqualified early return was swallowing.
// ---------------------------------------------------------------------------

#[test]
fn referenced_alias_in_function_body_resolves_the_specifier() {
    assert_codes(
        &referenced(FUNCTION_BODY),
        &[TS1232, TS2307],
        "function body, alias used",
    );
}

#[test]
fn referenced_alias_in_method_body_resolves_the_specifier() {
    assert_codes(
        &referenced(METHOD_BODY),
        &[TS1232, TS2307],
        "method body, alias used",
    );
}

#[test]
fn referenced_alias_in_static_block_resolves_the_specifier() {
    assert_codes(
        &referenced(STATIC_BLOCK),
        &[TS1232, TS2307],
        "class static block, alias used",
    );
}

#[test]
fn referenced_alias_in_namespace_body_resolves_the_specifier() {
    assert_codes(
        &referenced(NAMESPACE_BODY),
        &[TS1147, TS2307],
        "namespace body, alias used",
    );
}

#[test]
fn alias_referenced_only_from_a_nested_closure_resolves_the_specifier() {
    assert_codes(
        "function f() {\nimport x = require(\"nonexistent-module\");\nconst g = () => x;\n}",
        &[TS1232, TS2307],
        "function body, alias used only inside a nested arrow",
    );
}

// ---------------------------------------------------------------------------
// NEGATIVE CONTROLS — a block-like container is not a declaration scope, so it
// resolves the specifier whether or not the alias is used. A "suppress when
// unreferenced" rule applied uniformly across containers passes every test above
// and breaks all five of these.
// ---------------------------------------------------------------------------

#[test]
fn a_bare_block_resolves_the_specifier_even_when_unreferenced() {
    assert_codes(
        &unreferenced(BARE_BLOCK),
        &[TS1232, TS2307],
        "bare block is not a declaration scope",
    );
}

#[test]
fn an_if_block_resolves_the_specifier_even_when_unreferenced() {
    assert_codes(
        &unreferenced(IF_BLOCK),
        &[TS1232, TS2307],
        "`if` body is not a declaration scope",
    );
}

#[test]
fn a_loop_body_resolves_the_specifier_even_when_unreferenced() {
    assert_codes(
        &unreferenced(LOOP_BODY),
        &[TS1232, TS2307],
        "loop body is not a declaration scope",
    );
}

#[test]
fn a_bare_block_resolves_the_specifier_when_referenced_too() {
    assert_codes(
        &referenced(BARE_BLOCK),
        &[TS1232, TS2307],
        "bare block, alias used",
    );
}

#[test]
fn a_shadowed_alias_in_a_bare_block_still_resolves_the_specifier() {
    // The use resolves to the inner `let x`, so the alias is unreferenced — and it
    // still resolves, because the container is a block. Same verdict as the plain
    // unreferenced block row, reached by a different route.
    assert_codes(
        "{\nimport x = require(\"nonexistent-module\");\n{ let x = 1; x; }\n}",
        &[TS1232, TS2307],
        "bare block, alias shadowed at the use site",
    );
}

// ---------------------------------------------------------------------------
// Symbol identity, not spelling: the anti-hardcoding axis.
// ---------------------------------------------------------------------------

#[test]
fn a_shadowing_local_is_not_a_reference_to_the_alias() {
    // Inside a declaration scope, where the referenced-alias rule is live, the
    // shadowing local must not count as a use.
    assert_codes(
        "function f() {\nimport x = require(\"nonexistent-module\");\n{ let x = 1; x; }\n}",
        &[TS1232],
        "function body, the only `x` at the use site is a shadowing local",
    );
}

#[test]
fn the_rule_does_not_depend_on_the_binder_name() {
    assert_codes(
        "function f() {\nimport someLongAliasName = require(\"nonexistent-module\");\n}",
        &[TS1232],
        "renamed binder, alias never used",
    );
    assert_codes(
        "function f() {\nimport someLongAliasName = require(\"nonexistent-module\");\nsomeLongAliasName;\n}",
        &[TS1232, TS2307],
        "renamed binder, alias used",
    );
}

#[test]
fn each_alias_in_a_declaration_scope_is_judged_on_its_own_use() {
    // Only `a` is used, so only `a`'s specifier resolves — one TS2307, not two.
    assert_codes(
        "function f() {\nimport a = require(\"nonexistent-a\");\nimport b = require(\"nonexistent-b\");\na;\n}",
        &[TS1232, TS1232, TS2307],
        "function body, two aliases, one of them used",
    );
}

// ---------------------------------------------------------------------------
// Controls: a valid position resolves regardless of use.
// ---------------------------------------------------------------------------

#[test]
fn top_level_import_equals_resolves_when_unreferenced() {
    assert_codes(
        "import x = require(\"nonexistent-module\");",
        &[TS1202, TS2307],
        "control: valid position, alias never used",
    );
}

#[test]
fn top_level_import_equals_resolves_when_referenced() {
    assert_codes(
        "import x = require(\"nonexistent-module\");\nx;",
        &[TS1202, TS2307],
        "control: valid position, alias used",
    );
}
