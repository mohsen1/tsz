//! A position-invalid `import x = ...` alias binds to the nearest *declaration
//! container*, not to the plain `Block` that encloses it.
//!
//! `tsc`'s binder carries two cursors: `container` (source file, module body,
//! class body, or any function-like node) and `blockScopeContainer` (which
//! additionally advances into every plain `Block`). Only block-scoped
//! declarations — `let`, `const`, `class` — are recorded in the block scope;
//! everything else goes to the container. An import alias is not block-scoped,
//! so `{ import x = require("m"); }` records `x` in the enclosing *container*
//! even though the declaration's position is illegal (TS1232).
//!
//! That is observable two ways, and `tsz` was wrong in both before this suite
//! existed: a reference outside the block reported TS2304 ("Cannot find name")
//! on a name `tsc` resolves, and the resolution that `tsc` performs for the
//! reference — TS2307 on the specifier — never happened.
//!
//! The container is the *nearest* one, not the file: an alias in a block inside
//! a function is visible in that function and nowhere else. A class static
//! block is function-like in `tsc`, so it is itself a container and does not
//! leak into the class body, even though `tsz` models its body as a block
//! scope.
//!
//! All expectations were measured against `typescript@7.0.2` with
//! `--noEmit --strict --pretty false --target es2015 --module commonjs`.
//!
//! Rows whose *only* residual delta is the separate unreferenced-alias gate
//! (#16410 item 2: `tsz` reports TS2307 for an unreferenced block-scoped alias
//! that `tsc` leaves alone, and swallows the one `tsc` reports inside a
//! function body) assert the absence of TS2304 rather than a full code vector,
//! so this suite pins the scoping rule without freezing a bug it does not own.

use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::construction::TypeInterner;

/// Check a single source with unresolved-import reporting on, so a specifier
/// naming a nonexistent module reports TS2307 exactly when it is resolved.
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

fn assert_codes(source: &str, expected: &[u32], what: &str) {
    let actual = check(source);
    assert_eq!(actual, expected, "{what}\nsource:\n{source}");
}

/// The half of a row this change owns: the alias resolves, so no TS2304.
fn assert_alias_resolves(source: &str, what: &str) {
    let actual = check(source);
    assert!(
        !actual.contains(&2304),
        "{what}: expected the alias to resolve (no TS2304), got {actual:?}\nsource:\n{source}"
    );
}

// ---------------------------------------------------------------------------
// Block-like containers: the alias escapes to the enclosing container.
//
// Every binder name below is distinct so no assertion can be satisfied by a
// name-keyed shortcut.
// ---------------------------------------------------------------------------

#[test]
fn bare_block_alias_is_visible_after_the_block() {
    assert_codes(
        r#"{
  import alpha = require("nonexistent-module");
}
alpha;"#,
        &[1232, 2307],
        "the primary repro from #16410 item 4",
    );
}

#[test]
fn bare_block_alias_is_visible_from_a_sibling_block() {
    assert_codes(
        r#"{
  import bravo = require("nonexistent-module");
}
{
  bravo;
}"#,
        &[1232, 2307],
        "a sibling block sees it too — the alias is not in either block's scope",
    );
}

#[test]
fn bare_block_alias_is_visible_before_the_declaring_block() {
    assert_codes(
        r#"charlie;
{
  import charlie = require("nonexistent-module");
}"#,
        &[1232, 2307],
        "container-scoped declarations are visible before their declaration site",
    );
}

#[test]
fn if_block_alias_is_visible_after_the_statement() {
    assert_codes(
        r#"if (true) {
  import delta = require("nonexistent-module");
}
delta;"#,
        &[1232, 2307],
        "an `if` block is a block scope, not a container",
    );
}

#[test]
fn loop_body_alias_is_visible_after_the_loop() {
    assert_codes(
        r#"for (;;) {
  import echo = require("nonexistent-module");
  break;
}
echo;"#,
        &[1232, 2307],
        "a loop body is a block scope, not a container",
    );
}

#[test]
fn try_block_alias_is_visible_after_the_statement() {
    assert_codes(
        r#"try {
  import foxtrot = require("nonexistent-module");
} catch {}
foxtrot;"#,
        &[1232, 2307],
        "a `try` block is a block scope, not a container",
    );
}

#[test]
fn switch_case_block_alias_is_visible_after_the_statement() {
    assert_codes(
        r#"switch (1) {
  case 1: {
    import golf = require("nonexistent-module");
  }
}
golf;"#,
        &[1232, 2307],
        "a case block is a block scope, not a container",
    );
}

#[test]
fn nested_blocks_walk_all_the_way_out_to_the_container() {
    assert_codes(
        r#"{
  {
    import hotel = require("nonexistent-module");
  }
}
hotel;"#,
        &[1232, 2307],
        "the walk skips every intervening block, not just the innermost one",
    );
}

#[test]
fn qualified_name_form_takes_the_same_scope() {
    // `import x = N.M` is the other `import =` production. tsc reports TS1232
    // alone here: the alias resolves at top level, so there is no TS2304 and no
    // module specifier to fail on.
    assert_codes(
        r#"{
  import india = November.Mike;
}
india;
namespace November {
  export namespace Mike {
    export const q = 1;
  }
}"#,
        &[1232],
        "the qualified-name form is not block-scoped either",
    );
}

// ---------------------------------------------------------------------------
// The container is the *nearest* one, not the source file.
// ---------------------------------------------------------------------------

#[test]
fn block_inside_a_function_binds_to_that_function_not_the_file() {
    assert_codes(
        r#"function outer() {
  {
    import juliett = require("nonexistent-module");
  }
}
juliett;"#,
        &[1232, 2304],
        "the alias stops at the function; the file-level reference stays unresolved",
    );
}

#[test]
fn block_inside_a_function_is_visible_within_that_function() {
    assert_alias_resolves(
        r#"function outer() {
  {
    import kilo = require("nonexistent-module");
  }
  kilo;
}"#,
        "the same alias resolves inside its own function",
    );
}

#[test]
fn block_inside_a_namespace_binds_to_that_namespace() {
    assert_codes(
        r#"namespace Lima {
  {
    import mike = require("nonexistent-module");
  }
  mike;
}"#,
        &[1232, 2307],
        "a namespace body is a container, so the alias stops there and resolves",
    );
}

// ---------------------------------------------------------------------------
// Negative half: containers that must NOT leak.
// ---------------------------------------------------------------------------

#[test]
fn function_body_alias_does_not_escape_the_function() {
    assert_codes(
        r#"function outer() {
  import november = require("nonexistent-module");
}
november;"#,
        &[1232, 2304],
        "a function body is a container — TS2304 outside it is correct",
    );
}

#[test]
fn method_body_alias_does_not_escape_the_method() {
    assert_codes(
        r#"class Oscar {
  m() {
    import papa = require("nonexistent-module");
  }
}
papa;"#,
        &[1232, 2304],
        "a method body is a container",
    );
}

#[test]
fn class_static_block_alias_does_not_escape_the_class() {
    // A class static block is function-like in tsc, so it is a container even
    // though tsz gives its body a `ContainerKind::Block` scope. The carve-out
    // in `nearest_declaration_container_scope` is what keeps TS2304 here.
    assert_codes(
        r#"class Quebec {
  static {
    import romeo = require("nonexistent-module");
  }
}
romeo;"#,
        &[1232, 2304],
        "a static block must not leak into the class body, and (#16450) must not resolve the specifier either",
    );
}

// ---------------------------------------------------------------------------
// #16450: a class static block must not resolve an *unreferenced*
// position-invalid `import =` specifier, same as any other function-like
// container's unused row. `tsc` reports TS1232 alone for the unreferenced row
// below; tsz additionally reported a spurious TS2307 because
// `is_inside_function_body` did not recognize `CLASS_STATIC_BLOCK_DECLARATION`
// as a function-like ancestor, so the `in_wrong_context &&
// is_inside_function_body` short-circuit in `check_import_equals_declaration`
// never fired for a static block.
//
// A *referenced* alias is a different row of the table, and tsz was already
// correct there: measured against the pinned oracle (#16527 review), a
// static block resolves on a genuine reference exactly like any other
// function-like container — there is no static-block carve-out for that row.
// An earlier revision of this suite pinned the opposite (no resolution even
// when referenced) without oracling it, which made a correct row read as
// failing; a #16533 revision oracled it and corrected the assertion instead
// of "fixing" already-correct code to match the wrong pin.
// ---------------------------------------------------------------------------

#[test]
fn static_block_single_alias_does_not_resolve_the_specifier() {
    assert_codes(
        r#"class E {
  static {
    import sierra = require("nonexistent-module");
  }
}"#,
        &[1232],
        "#16450: one alias, unreferenced",
    );
}

#[test]
fn static_block_referenced_alias_resolves_the_specifier_like_any_function_body() {
    assert_codes(
        r#"class E {
  static {
    import tango = require("nonexistent-module");
    tango;
  }
}"#,
        &[1232, 2307],
        "referencing the alias resolves the specifier — a static block is not exempt from \
         the general 'bound & used' row (pinned oracle, #16527 review)",
    );
}

#[test]
fn nested_block_inside_a_static_block_still_does_not_resolve_the_specifier() {
    assert_codes(
        r#"class E {
  static {
    {
      import uniform = require("nonexistent-module");
    }
  }
}"#,
        &[1232],
        "#16450: a plain block nested inside the static block is still part of that container",
    );
}

#[test]
fn nested_block_referenced_alias_inside_static_block_resolves_the_specifier() {
    assert_codes(
        r#"class E {
  static {
    {
      import whiskey = require("nonexistent-module");
      whiskey;
    }
  }
}"#,
        &[1232, 2307],
        "a reference from a plain block nested inside the static block resolves the \
         specifier the same as a direct reference — nesting does not change the container",
    );
}

// #16527 item 2 (colliding aliases inside a static block) is a real, still
// open, oracle-confirmed defect — the row below stays red. It is deeper than
// "skip a sibling declaration's own subtree": each `import x = ...` alias
// binds its own distinct `SymbolId` (aliases deliberately do not merge,
// `crates/tsz-binder/src/nodes/binding.rs` around the `ALIAS && ALIAS` branch
// of `declare_symbol`), and only the *first* declaration for a colliding name
// ever reaches `resolveExternalModuleName` in `tsc` — verified with the
// pinned oracle:
//
// - 2/3 colliding aliases, no reference: no specifier ever resolves.
// - 2/3 colliding aliases, one explicit reference: only the *first*
//   declaration's specifier resolves (`TS2307` lands on its position, not the
//   later duplicates' — confirmed by column offset).
// - Even the "unused resolves in a script's top-level block" row (#16505) is
//   suppressed once a name collides — a bare top-level block with two
//   colliding aliases and no reference at all resolves neither, where a
//   single non-colliding alias in the same position would resolve.
//
// A correct fix needs a first-declaration-in-group gate plus a group-wide
// (not single-symbol) reference scan, and the full unused/script/module axis
// re-measured under collision — scoped as its own session rather than
// folded into this one.
#[test]
#[ignore = "#16527 item 2: still open. Each colliding `import x = ...` alias binds its own \
     distinct SymbolId (aliases deliberately do not merge), so a naive own-subtree skip \
     cannot see a sibling duplicate as a binding site — the sibling's own name resolves \
     through scope shadowing to whichever declaration is currently live, not to its \
     originally-bound symbol. A real fix needs a first-declaration-in-group gate plus a \
     group-wide reference scan; see the module comment above this test."]
fn static_block_two_colliding_aliases_do_not_resolve_the_specifier() {
    assert_codes(
        r#"class E {
  static {
    import victor = require("nonexistent-a");
    import victor = require("nonexistent-b");
  }
}"#,
        &[1232, 1232, 2300, 2300],
        "#16450: the duplicate-alias TS2300 pair (#16437) rides along, but neither specifier resolves",
    );
}

// ---------------------------------------------------------------------------
// Positions where the retarget must be a no-op: the container already *is* the
// current scope, so nothing about a legal `import =` moves.
// ---------------------------------------------------------------------------

#[test]
fn top_level_import_equals_is_unchanged() {
    assert_codes(
        r#"import sierra = require("nonexistent-module");
sierra;"#,
        &[2307],
        "a legal top-level `import =` still resolves and reports only TS2307",
    );
}

#[test]
fn namespace_body_import_equals_is_unchanged() {
    assert_codes(
        r#"namespace Tango {
  import uniform = require("nonexistent-module");
  uniform;
}"#,
        // A namespace body is already a container, so the walk is a no-op here.
        // TS1147 rides along because a namespace `import =` may not reference an
        // external module at all — an orthogonal rule, and one tsz already
        // matches; the point of the row is that the alias still resolves.
        &[1147, 2307],
        "a namespace-body `import =` is untouched by the container walk",
    );
}

#[test]
fn top_level_qualified_import_equals_is_unchanged() {
    assert_codes(
        r#"namespace Victor {
  export const w = 1;
}
import whiskey = Victor;
whiskey.w;"#,
        &[],
        "the legal qualified form stays clean",
    );
}
