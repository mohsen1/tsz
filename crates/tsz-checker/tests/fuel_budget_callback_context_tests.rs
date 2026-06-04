//! Regression coverage for issue #10677.
//!
//! The solver bounds generic `Application` evaluation with a per-file
//! instantiation-fuel budget (`MAX_GLOBAL_INSTANTIATION_FUEL`). That budget is
//! cumulative across every top-level statement in a file. Before the fix, a
//! file whose earlier statements performed heavy generic evaluation (deep
//! builder/query chains over large `keyof` unions — kysely is the canonical
//! witness) could exhaust the budget, leaving `instantiation_limits_exceeded()`
//! permanently true. Every following statement then resolved generic builder
//! receiver types to opaque/`any`, so contextual typing of a later callback
//! argument collapsed: the callback parameter was reported as implicitly `any`
//! (TS7006) and the generic calls inside the callback were then treated as
//! untyped, spuriously reporting TS2347.
//!
//! The checker now resets the session instantiation fuel between top-level
//! statements (mirroring the existing per-statement resolution-fuel reset for
//! issue #12144), so one statement can no longer starve the next.

use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::{
    check_source_with_libs, diagnostic_line_column, load_default_lib_files,
};

/// The kysely query-builder type machinery (`selectFrom`/`innerJoin`/`select`
/// with template-literal column references and conditional/mapped result
/// aliases). Evaluating these generic applications is exactly the kind of work
/// that spends the session instantiation-fuel budget.
const PRELUDE: &str = include_str!("kysely_prelude.txt");

fn strict_default_lib_diagnostics(source: &str) -> Vec<Diagnostic> {
    let lib_files = load_default_lib_files();
    check_source_with_libs(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            no_implicit_any: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
        &lib_files,
    )
    .into_iter()
    .filter(|diagnostic| diagnostic.code != 2318)
    .collect()
}

fn format_diagnostics(source: &str, diagnostics: &[Diagnostic]) -> Vec<String> {
    diagnostics
        .iter()
        .map(|diagnostic| {
            let (line, column) = diagnostic_line_column(source, diagnostic);
            format!(
                "TS{} at {line}:{column}: {}",
                diagnostic.code, diagnostic.message_text
            )
        })
        .collect()
}

/// Earlier statements drain the per-file instantiation-fuel budget; the final
/// `.select` callback must still receive its contextual parameter type instead
/// of degrading to implicit `any` and reporting a false TS2347 on the generic
/// `$castTo<...>()` calls inside the callback.
#[test]
fn fuel_drained_earlier_statements_keep_select_callback_context() {
    let mut schema = String::from("type FuelDB = {\n");
    for i in 0..3 {
        schema.push_str(&format!(
            "  \"tbl{i}\": {{ id: number; name: string; ref_id: number; kind: \"k{i}\"; note: string }};\n"
        ));
    }
    schema.push_str("};\ndeclare const fdb: QueryCreator<FuelDB>;\n");

    // Each of these statements evaluates fresh, uncached generic Applications
    // (distinct table alias per statement), spending instantiation fuel. The
    // count is chosen to push the cumulative total comfortably past the 2000
    // budget before the target statement is reached.
    let mut drainer = String::new();
    for k in 0..130 {
        drainer.push_str(&format!(
            "fdb.selectFrom(\"tbl0 as q{k}\").select([\"q{k}.name as n{k}\"]);\n"
        ));
    }

    // The target statement mirrors kysely's `mssql-introspector.ts` `select`
    // callback: a chain of joins/`$if` callbacks followed by a `select`
    // callback that builds expressions with generic `$castTo<...>()` calls.
    let target = r#"
fdb.selectFrom("tbl0 as a0")
  .innerJoin("tbl1 as a1", "a1.ref_id", "a0.id")
  .leftJoin("tbl2 as a2", (join) => join.onRef("a2.ref_id", "=", "a0.id"))
  .$if(!withInternalKyselyTables, (qb) => qb.where("a0.name", "!=", "x"))
  .select((eb) => [
    "a0.name as name0",
    "a1.name as name1",
    eb.ref("a0.kind").$castTo<FuelDB["tbl0"]["kind"]>().as("a0_kind"),
    eb.ref("a1.id").$castTo<number>().as("a1_id"),
  ]);
"#;

    let source = format!("{PRELUDE}\n{schema}\n{drainer}\n{target}");

    let diagnostics = strict_default_lib_diagnostics(&source);
    assert!(
        !diagnostics.iter().any(|d| matches!(d.code, 7006 | 2347)),
        "fuel exhausted by earlier statements must not strip callback context. Got: {:#?}",
        format_diagnostics(&source, &diagnostics)
    );
}

/// Same fuel-exhaustion mechanism, but the draining statements and the target
/// `select` callback live inside a single *function body* rather than at the
/// top level. The per-file instantiation-fuel budget is also cumulative across
/// the statements of a block/function body, and kysely's introspectors (the
/// real witness — `mssql-introspector.ts`, `sqlite-introspector.ts`,
/// `migrator.ts`) are methods that run many heavy query-builder statements
/// before a `select`/`map` callback. The fuel reset must therefore apply
/// between block-body statements too, not only between top-level statements,
/// or the later callback parameter degrades to implicit `any` (TS7006) with a
/// downstream TS2347 (issue #10683 / #10677).
///
/// Binder names here deliberately differ from the top-level test (`builder`,
/// `cond`, `expr` instead of `eb`/`qb`) so the fix cannot be keyed to any
/// Kysely-specific identifier.
#[test]
fn fuel_drained_inside_function_body_keeps_select_callback_context() {
    let mut schema = String::from("type BodyDB = {\n");
    for i in 0..3 {
        schema.push_str(&format!(
            "  \"rel{i}\": {{ id: number; name: string; ref_id: number; kind: \"k{i}\"; note: string }};\n"
        ));
    }
    schema.push_str("};\ndeclare const bdb: QueryCreator<BodyDB>;\n");

    // Heavy draining statements, this time as statements *inside* a function
    // body (indented so they read as a method-like body).
    let mut drainer = String::new();
    for k in 0..130 {
        drainer.push_str(&format!(
            "  bdb.selectFrom(\"rel0 as s{k}\").select([\"s{k}.name as m{k}\"]);\n"
        ));
    }

    let target = r#"
  bdb.selectFrom("rel0 as a0")
    .innerJoin("rel1 as a1", "a1.ref_id", "a0.id")
    .leftJoin("rel2 as a2", (link) => link.onRef("a2.ref_id", "=", "a0.id"))
    .$if(!withInternalKyselyTables, (cond) => cond.where("a0.name", "!=", "x"))
    .select((builder) => [
      "a0.name as name0",
      "a1.name as name1",
      builder.ref("a0.kind").$castTo<BodyDB["rel0"]["kind"]>().as("a0_kind"),
      builder.ref("a1.id").$castTo<number>().as("a1_id"),
    ]);
"#;

    let source =
        format!("{PRELUDE}\n{schema}\nfunction introspectBody() {{\n{drainer}\n{target}\n}}\n");

    let diagnostics = strict_default_lib_diagnostics(&source);
    assert!(
        !diagnostics.iter().any(|d| matches!(d.code, 7006 | 2347)),
        "fuel exhausted inside a function body must not strip callback context. Got: {:#?}",
        format_diagnostics(&source, &diagnostics)
    );
}
