//! A plain `import ... from "m"` nested inside a *block* that itself sits
//! inside a namespace still resolved its module specifier, reporting a
//! spurious TS2307 alongside the correct TS1232 placement diagnostic.
//!
//! `namespace P { { import { a } from "m"; } }` is TS1232 alone in `tsc`
//! (`typescript@7.0.2`, both the default and `--singleThreaded` scheduler
//! modes — this row is oracle-independent, unlike the #16413/#16490 family).
//! `tsz` additionally reported TS2307.
//!
//! Root cause: `check_import_declaration`'s `wrong_context_allows_module_semantics`
//! gate (`crates/tsz-checker/src/declarations/import/declaration_check_body.rs`)
//! already computed the correct answer (`false`) for this shape — it walks
//! `is_inside_function_body` and `is_inside_namespace_declaration`, and the
//! latter finds the enclosing namespace regardless of how many plain blocks
//! sit between it and the import. But the computed value was only consulted
//! through a `has_real_syntax_errors`-gated formula, not wired into an actual
//! early return; only the narrower `is_inside_function_body` case got its own
//! `return`. A file with no *other* real syntax error therefore fell through
//! and resolved the specifier anyway. The fix generalizes that single early
//! return from `is_inside_function_body` to `!wrong_context_allows_module_semantics`,
//! which both cases already computed correctly.
//!
//! `import x = require(...)` (the other import production, owned by
//! `equals.rs`) already got this right — see
//! `position_invalid_import_equals_container_scope_tests.rs`. This suite is
//! the `import ... from` twin, restricted to the container-scope question;
//! it does not touch the referenced-alias family.

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

fn assert_codes(source: &str, expected: &[u32], what: &str) {
    let actual = check(source);
    assert_eq!(actual, expected, "{what}\nsource:\n{source}");
}

// ---------------------------------------------------------------------------
// The fix: every container whose chain reaches a namespace's ModuleBlock
// before it reaches SourceFile suppresses resolution, not just a bare block.
// ---------------------------------------------------------------------------

#[test]
fn namespace_bare_block_suppresses_resolution() {
    assert_codes(
        r#"namespace Alpha {
  {
    import { a } from "nonexistent-module";
  }
}"#,
        &[1232],
        "the primary repro: a bare block inside a namespace",
    );
}

#[test]
fn namespace_if_block_suppresses_resolution() {
    assert_codes(
        r#"namespace Bravo {
  if (true) {
    import { a } from "nonexistent-module";
  }
}"#,
        &[1232],
        "an `if` block inside a namespace",
    );
}

#[test]
fn namespace_loop_body_suppresses_resolution() {
    assert_codes(
        r#"namespace Charlie {
  for (;;) {
    import { a } from "nonexistent-module";
    break;
  }
}"#,
        &[1232],
        "a loop body inside a namespace",
    );
}

#[test]
fn namespace_while_body_suppresses_resolution() {
    assert_codes(
        r#"namespace Delta {
  while (true) {
    import { a } from "nonexistent-module";
    break;
  }
}"#,
        &[1232],
        "a `while` body inside a namespace",
    );
}

#[test]
fn namespace_try_block_suppresses_resolution() {
    assert_codes(
        r#"namespace Echo {
  try {
    import { a } from "nonexistent-module";
  } catch {}
}"#,
        &[1232],
        "a `try` block inside a namespace",
    );
}

#[test]
fn namespace_labeled_block_suppresses_resolution() {
    assert_codes(
        r#"namespace Foxtrot {
  outer: {
    import { a } from "nonexistent-module";
  }
}"#,
        &[1232],
        "a labeled block inside a namespace",
    );
}

#[test]
fn namespace_switch_case_block_suppresses_resolution() {
    assert_codes(
        r#"namespace Golf {
  switch (1) {
    case 1: {
      import { a } from "nonexistent-module";
    }
  }
}"#,
        &[1232],
        "a `switch` case block inside a namespace",
    );
}

#[test]
fn namespace_doubly_nested_block_suppresses_resolution() {
    assert_codes(
        r#"namespace Hotel {
  {
    {
      import { a } from "nonexistent-module";
    }
  }
}"#,
        &[1232],
        "the walk must not stop at the first block — it needs to reach the ModuleBlock",
    );
}

#[test]
fn namespace_block_renamed_named_import_suppresses_resolution() {
    assert_codes(
        r#"namespace India {
  {
    import { renamed as alias } from "nonexistent-module";
  }
}"#,
        &[1232],
        "renamed binder: the gate is structural, not name-keyed",
    );
}

#[test]
fn namespace_block_namespace_import_suppresses_resolution() {
    assert_codes(
        r#"namespace Juliett {
  {
    import * as ns from "nonexistent-module";
  }
}"#,
        &[1232],
        "the `import * as` form takes the same gate as named imports",
    );
}

#[test]
fn namespace_block_type_only_import_suppresses_resolution() {
    assert_codes(
        r#"namespace Kilo {
  {
    import type { a } from "nonexistent-module";
  }
}"#,
        &[1232],
        "a type-only import is still gated the same way",
    );
}

// ---------------------------------------------------------------------------
// Negative controls: shapes that must keep resolving, or take a different
// diagnostic entirely, unchanged by this fix.
// ---------------------------------------------------------------------------

#[test]
fn top_level_bare_block_still_resolves() {
    assert_codes(
        r#"{
  import { a } from "nonexistent-module";
}"#,
        &[1232, 2307],
        "a bare block with no enclosing namespace/function still resolves (31/32 baseline)",
    );
}

#[test]
fn top_level_control_resolves_with_no_placement_error() {
    assert_codes(
        r#"import { a } from "nonexistent-module";"#,
        &[2307],
        "a legal top-level import is untouched",
    );
}

#[test]
fn function_body_import_still_suppresses_resolution() {
    assert_codes(
        r#"function f() {
  import { a } from "nonexistent-module";
}"#,
        &[1232],
        "function bodies already suppressed resolution before this fix; must still do so",
    );
}

#[test]
fn namespace_block_inside_function_still_suppresses_resolution() {
    assert_codes(
        r#"namespace Lima {
  function f() {
    import { a } from "nonexistent-module";
  }
}"#,
        &[1232],
        "a function nested in a namespace is still function-scoped, not namespace-scoped",
    );
}

#[test]
fn namespace_direct_child_import_is_ts1147_not_ts1232() {
    assert_codes(
        r#"namespace Mike {
  import { a } from "nonexistent-module";
}"#,
        &[1147],
        "an import directly in a namespace body takes the unrelated TS1147 rule, untouched by this fix",
    );
}
