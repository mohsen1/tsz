//! Two `import x = ...` aliases sharing a name in the same declaration
//! container report TS2300, even when they sit in different nested blocks
//! of that container.
//!
//! `tsc`'s `AliasExcludes` is exactly `Alias`, so a second alias declared
//! with the same name as an existing one is a redeclaration conflict and
//! `declareSymbol` reports TS2300 for both. Position-invalid `import =`
//! binds to its nearest *declaration container*, not the `Block` that
//! encloses it (#16428) — so two aliases nested in two different blocks of
//! one container (`{ import x = ...; } { import x = ...; }`) are, in
//! `tsc`'s model, two declarations of the same name in the same container
//! and collide exactly like two adjacent ones would.
//!
//! `tsz`'s checker-side duplicate-alias scan (`check_import_alias_duplicates`)
//! only walked the direct statement list it was handed (source file top
//! level, or one namespace body), so two aliases in sibling blocks were
//! invisible to it even though the binder now (correctly, since #16428)
//! resolves them through the same container scope. This suite pins the fix:
//! the scan recurses through the same "transparent" block-like statements
//! (`if`/`for`/`while`/`try`/`switch`/labeled/bare `{ }`) the binder treats
//! as non-containers, stopping at genuine declaration-container boundaries
//! (function bodies, class bodies/static blocks, nested namespaces) exactly
//! like `nearest_declaration_container_scope` does.
//!
//! All expectations were measured against `typescript@7.0.2` with
//! `--noEmit --strict --pretty false --target es2015 --module commonjs`.
//!
//! ## Known residual, not owned by this suite
//!
//! Once two aliases collide, `tsc` resolves the module specifier of only the
//! *first* declaration (one TS2307), because the colliding declarations are
//! one merged symbol in `tsc`'s model and `resolveAlias` follows a single
//! declaration. `tsz` keeps a distinct symbol per colliding alias declaration
//! (deliberately — see `declare_symbol` in `crates/tsz-binder/src/nodes/binding.rs`,
//! which preserves each duplicate as its own binding so a later value-bearing
//! alias can still shadow an earlier type-only one in expression resolution),
//! so each still resolves its own specifier independently. That is the same
//! "resolve only when referenced" alias-merge mechanism tracked separately
//! (#16410 item 2 / #16411) and predates this fix — it does not regress here,
//! and rows below assert the TS2300 count rather than the full TS2307 count
//! where the two diverge.

use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::construction::TypeInterner;

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
            module: tsz_common::common::ModuleKind::CommonJS,
            target: tsz_common::common::ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );
    checker.ctx.report_unresolved_imports = true;
    checker.check_source_file(root);
    let mut codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    codes.sort_unstable();
    codes
}

fn duplicate_count(codes: &[u32]) -> usize {
    codes.iter().filter(|&&c| c == 2300).count()
}

// ---------------------------------------------------------------------------
// Positive: same container, different nested blocks -> collides.
// ---------------------------------------------------------------------------

#[test]
fn cross_block_same_name_aliases_collide() {
    let source = r#"{ import x = require("nonexistent-a"); }
{ import x = require("nonexistent-b"); x; }
"#;
    let codes = check(source);
    assert_eq!(
        duplicate_count(&codes),
        2,
        "two same-name aliases in sibling blocks of the file container must both report TS2300, got {codes:?}"
    );
    assert_eq!(
        codes.iter().filter(|&&c| c == 1232).count(),
        2,
        "both aliases stay position-invalid (TS1232), got {codes:?}"
    );
}

#[test]
fn three_same_name_aliases_all_flagged() {
    let source = r#"{ import x = require("nonexistent-a"); }
{ import x = require("nonexistent-b"); }
{ import x = require("nonexistent-c"); }
"#;
    let codes = check(source);
    assert_eq!(
        duplicate_count(&codes),
        3,
        "every one of 3 colliding aliases reports its own TS2300, got {codes:?}"
    );
}

#[test]
fn same_namespace_two_blocks_collide() {
    let source = r#"namespace N {
  { import x = require("nonexistent-a"); }
  { import x = require("nonexistent-b"); x; }
}
"#;
    let codes = check(source);
    assert_eq!(
        codes,
        vec![1232, 1232, 2300, 2300, 2307],
        "namespace-body container: matches tsc exactly (single TS2307, the alias-merge \
         residual noted above does not appear here because only the referenced alias's \
         group needed to resolve once), got {codes:?}"
    );
}

#[test]
fn switch_case_and_try_block_aliases_collide() {
    let source = r#"switch (1) {
  case 1:
    import x = require("nonexistent-a");
    break;
}
try {
  import x = require("nonexistent-b");
  x;
} catch {}
"#;
    let codes = check(source);
    assert_eq!(
        duplicate_count(&codes),
        2,
        "a `switch`-case alias and a `try`-block alias in the same file container collide, got {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// Negative: a genuine container boundary must not collide across it.
// ---------------------------------------------------------------------------

#[test]
fn different_function_bodies_do_not_collide() {
    let source = r#"function f() { import a = require("nonexistent-a"); }
function g() { import a = require("nonexistent-b"); }
"#;
    let codes = check(source);
    assert_eq!(
        codes,
        vec![1232, 1232],
        "each function body is its own container; same-name aliases in two different \
         functions must not report TS2300, got {codes:?}"
    );
}

#[test]
fn different_namespaces_do_not_collide() {
    let source = r#"namespace N1 { import x = require("nonexistent-a"); }
namespace N2 { import x = require("nonexistent-b"); }
"#;
    let codes = check(source);
    assert_eq!(
        codes,
        vec![1147, 1147],
        "two different namespace bodies are two different containers, got {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// Negative: an alias never conflicts with a block-scoped variable, in either
// order (`AliasExcludes` is only `Alias`; oracle-confirmed both directions).
// ---------------------------------------------------------------------------

#[test]
fn let_then_alias_stays_clean() {
    let source = r#"let x = 1;
{ import x = require("nonexistent-a"); }
"#;
    let codes = check(source);
    assert_eq!(
        duplicate_count(&codes),
        0,
        "a block-scoped `let` does not exclude a later alias, got {codes:?}"
    );
}

#[test]
fn alias_then_let_stays_clean() {
    let source = r#"{ import x = require("nonexistent-a"); }
let x = 1;
"#;
    let codes = check(source);
    assert_eq!(
        duplicate_count(&codes),
        0,
        "an alias does not exclude a later block-scoped `let` either — oracle-confirmed \
         symmetric, not the asymmetry #16429 speculated, got {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// Position-valid form (not this bug's trigger condition): already correct
// before this fix, pinned so a future change to the shared scan can't move it.
// ---------------------------------------------------------------------------

#[test]
fn top_level_position_valid_duplicate_unchanged() {
    let source = r#"import a = require("nonexistent-a");
import a = require("nonexistent-b");
"#;
    let codes = check(source);
    assert_eq!(
        codes,
        vec![2300, 2300, 2307, 2307],
        "ordinary top-level duplicate aliases were already correct, got {codes:?}"
    );
}
