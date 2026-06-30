//! Cross-crate architecture guard tests.
//!
//! These tests enforce structural invariants from CLAUDE.md that cannot be
//! expressed through Rust's module system or Cargo dependency declarations.
//!
//! Guards:
//! - Emitter must not perform semantic type validation (rule 13)
//! - Binder must not import solver or checker (rule 4)
//!
//! Note: Solver file size ratchets are in `solver_file_size_ceiling_tests.rs`.

use std::fs;
use std::path::{Path, PathBuf};

fn walk_rs_files(dir: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_rs_files(&path, files);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            files.push(path);
        }
    }
}

// =============================================================================
// Emitter semantic validation guard
// =============================================================================

/// Guard that the emitter crate does not perform on-the-fly semantic type validation.
///
/// Per CLAUDE.md section 13: "No on-the-fly semantic type validation."
/// Per CLAUDE.md section 4: "Emitter importing Checker internals for semantic checks"
/// is a forbidden shortcut.
///
/// The emitter may use solver read-only APIs (`TypeInterner`, `type_queries`, visitor)
/// for declaration emit and type printing, but must NOT use relation/compatibility
/// APIs that perform semantic validation.
#[test]
fn emitter_must_not_use_semantic_validation_apis() {
    // These are solver relation/compatibility APIs that the emitter must never use.
    // Using them would mean the emitter is performing semantic type validation.
    const FORBIDDEN_PATTERNS: &[&str] = &[
        "CompatChecker",
        "SubtypeChecker",
        "is_assignable",
        "is_subtype_of",
        "RelationResult",
        "check_assignability",
        "tsz_checker::",
    ];

    let emitter_src = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("tsz-emitter/src");

    if !emitter_src.exists() {
        // Skip if emitter crate doesn't exist in this workspace layout
        return;
    }

    let mut files = Vec::new();
    walk_rs_files(&emitter_src, &mut files);

    let mut violations = Vec::new();

    for path in &files {
        let rel = path
            .strip_prefix(&emitter_src)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");

        let Ok(src) = fs::read_to_string(path) else {
            continue;
        };

        for (line_num, line) in src.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") || trimmed.starts_with("///") {
                continue;
            }

            for &pattern in FORBIDDEN_PATTERNS {
                if line.contains(pattern) {
                    violations.push(format!(
                        "  {}:{} — uses forbidden pattern `{}`",
                        rel,
                        line_num + 1,
                        pattern
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Emitter must not perform semantic type validation (CLAUDE.md section 13). \
         The emitter may use read-only solver APIs (TypeInterner, type_queries, visitor) \
         but must NOT use relation/compatibility/checker APIs. \
         Violations found:\n{}",
        violations.join("\n")
    );
}

// =============================================================================
// Binder semantic isolation guard
// =============================================================================

/// Guard that the binder crate does not import solver or checker types.
///
/// Per CLAUDE.md section 4: "Binder importing Solver for semantic decisions"
/// is a forbidden shortcut.
/// Per CLAUDE.md section 10: "No type inference/subtyping logic in binder."
///
/// This is enforced at the Cargo dependency level, but this test provides
/// a source-level belt-and-suspenders check and clearer error messages.
#[test]
fn binder_must_not_import_solver_or_checker() {
    const FORBIDDEN_IMPORTS: &[&str] = &[
        "tsz_solver::",
        "tsz_checker::",
        "use tsz_solver",
        "use tsz_checker",
    ];

    let binder_src = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("tsz-binder/src");

    if !binder_src.exists() {
        return;
    }

    let mut files = Vec::new();
    walk_rs_files(&binder_src, &mut files);

    let mut violations = Vec::new();

    for path in &files {
        let rel = path
            .strip_prefix(&binder_src)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");

        let Ok(src) = fs::read_to_string(path) else {
            continue;
        };

        for (line_num, line) in src.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue;
            }

            for &forbidden in FORBIDDEN_IMPORTS {
                if line.contains(forbidden) {
                    violations.push(format!(
                        "  {}:{} — imports {}",
                        rel,
                        line_num + 1,
                        forbidden
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Binder must not import solver or checker (CLAUDE.md sections 4, 10). \
         The binder produces symbols, scopes, and control-flow graphs without \
         type computation. Violations found:\n{}",
        violations.join("\n")
    );
}

// =============================================================================
// Solver staged-engine guard
// =============================================================================

fn read_solver_source(rel: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join(rel);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()))
}

fn read_solver_test(rel: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join(rel);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()))
}

#[test]
fn property_access_uses_evaluator_owned_index_signature_resolver() {
    let files = [
        "operations/property.rs",
        "operations/property_helpers.rs",
        "operations/property_visitor.rs",
    ];
    let mut violations = Vec::new();

    for rel in files {
        let src = read_solver_source(rel);
        for (line_num, line) in src.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") || trimmed.starts_with("///") {
                continue;
            }
            if line.contains("IndexSignatureResolver::new(") {
                violations.push(format!(
                    "  {rel}:{} — query through PropertyAccessEvaluator's resolver-aware index-signature helpers",
                    line_num + 1
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Property access must use its evaluator-owned index-signature resolver so \
         deferred application/lazy materialization sees the same resolver surface. \
         Violations found:\n{}",
        violations.join("\n")
    );
}

#[test]
fn evaluation_engine_keeps_request_stage_boundary() {
    let mod_rs = read_solver_source("evaluation/mod.rs");
    let request_rs = read_solver_source("evaluation/request.rs");
    let result_rs = read_solver_source("evaluation/result.rs");
    let evaluate_rs = read_solver_source("evaluation/evaluate.rs");
    // The public staged entry points live in the `evaluate/api.rs` submodule
    // after the engine split; the module boundary is still owned by `evaluate`.
    let evaluate_api_rs = read_solver_source("evaluation/evaluate/api.rs");
    let cross_eval_guard_rs = read_solver_source("evaluation/cross_eval_guard.rs");
    let infer_pattern_rs = read_solver_source("evaluation/evaluate_rules/infer_pattern.rs");
    let instantiation_result_rs = read_solver_source("instantiation/result.rs");
    let instantiation_api_rs = read_solver_source("instantiation/instantiate/api.rs");
    let subtype_core_rs = read_solver_source("relations/subtype/core.rs");
    let function_checking_rs = read_solver_source("relations/subtype/rules/functions/checking.rs");
    let functions_mod_rs = read_solver_source("relations/subtype/rules/functions/mod.rs");
    let query_cache_rs = read_solver_source("caches/query_cache.rs");
    let iteration_incomplete_tests =
        read_solver_test("evaluate_tests_parts/iteration_exceeded_incomplete.rs");

    assert!(
        mod_rs.contains("pub mod request;"),
        "evaluation module must keep its named request stage"
    );
    assert!(
        request_rs.contains("pub struct EvaluationCacheKey")
            && request_rs.contains("pub struct EvaluationOptions")
            && request_rs.contains("pub struct EvaluationRequest"),
        "evaluation/request.rs must own the typed request, options, and cache-key stage"
    );
    assert!(
        mod_rs.contains("pub mod result;")
            && result_rs.contains("pub struct EvaluationResult")
            && result_rs.contains("pub const fn into_type_id"),
        "evaluation/result.rs must own the typed result stage"
    );
    assert!(
        evaluate_rs.contains("use crate::evaluation::request::EvaluationRequest;")
            && evaluate_rs.contains("use crate::evaluation::result::EvaluationResult;")
            && evaluate_rs.contains("evaluate_type_with_request")
            && evaluate_api_rs.contains("pub fn evaluate_type_with_request")
            && evaluate_rs.contains(
                "pub fn evaluate_request_result(&mut self, request: EvaluationRequest) -> EvaluationResult"
            )
            && evaluate_rs
                .contains("pub fn evaluate_request(&mut self, request: EvaluationRequest)"),
        "evaluation/evaluate.rs must consume typed request/result stages instead of owning loose shell setup"
    );
    assert!(
        evaluate_rs.contains("fn with_resolver_and_defaults(")
            && evaluate_rs.matches("TypeEvaluator {").count() == 1
            && evaluate_rs.matches("with_resolver_and_defaults").count() >= 3,
        "TypeEvaluator construction must keep request-local cache/guard defaults in one path"
    );
    assert!(
        query_cache_rs
            .contains("use crate::evaluation::request::{EvaluationCacheKey, EvaluationRequest};")
            && query_cache_rs.contains("request.cache_key()")
            && query_cache_rs.contains("evaluate_request_memo_result(request)")
            && query_cache_rs.contains("is_stable_for_depth_agnostic_cache()")
            && !query_cache_rs
                .contains("evaluation_result.is_complete() && !evaluator.recursion_limit_hit()"),
        "query cache evaluation entries must derive option-sensitive keys from EvaluationRequest and consume EvaluationMemoResult stability instead of rebuilding termination predicates"
    );
    assert!(
        evaluate_rs
            .contains("fn request_result_for_test(&self, type_id: TypeId) -> EvaluationResult")
            && iteration_incomplete_tests.contains("request_result_for_test(TypeId::ERROR)")
            && !iteration_incomplete_tests.contains("evaluator.request_termination_kind"),
        "typed request-verdict tests must consume EvaluationResult through the evaluator boundary instead of reading the raw request_termination_kind slot"
    );
    assert!(
        result_rs.contains("fn unstable_complete(type_id: TypeId) -> Self")
            && result_rs.contains("enum EvaluationRequestStability")
            && result_rs.contains("request_stability: EvaluationRequestStability")
            && !result_rs.contains("request_state_stable: bool")
            && evaluate_rs
                .contains("request_state_cache_stability(&self) -> EvaluationRequestStability")
            && result_rs.contains("enum EvaluationMemoStability")
            && result_rs.contains("cache_stability: EvaluationMemoStability")
            && !result_rs.contains("stable_for_depth_agnostic_cache: bool")
            && cross_eval_guard_rs.contains("EvaluationMemoResult::unstable_complete(TypeId(80))")
            && infer_pattern_rs.contains("EvaluationMemoResult::unstable_complete(type_id)")
            && !cross_eval_guard_rs
                .contains("EvaluationMemoResult::new(EvaluationResult::complete")
            && !infer_pattern_rs.contains("EvaluationMemoResult::new(EvaluationResult::complete"),
        "unstable complete memo results must use the named EvaluationMemoResult boundary instead of rebuilding the stability bit by hand"
    );
    assert!(
        subtype_core_rs.contains("pub(crate) struct RelationEvaluationResult")
            && subtype_core_rs.contains("enum RelationEvaluationStability")
            && subtype_core_rs.contains("cache_stability: RelationEvaluationStability")
            && !subtype_core_rs.contains("stable_for_depth_agnostic_cache: bool")
            && subtype_core_rs
                .contains("eval_cache: FxHashMap<(TypeId, bool), RelationEvaluationResult>")
            && function_checking_rs
                .contains("RelationEvaluationResult::from_depth_agnostic_memo(memo_result)")
            && functions_mod_rs.contains(".is_unstable_unknown()")
            && !functions_mod_rs
                .contains("evaluate_type_with_stability(ret) == (TypeId::UNKNOWN, false)"),
        "function-relation evaluation caches must carry stability through RelationEvaluationResult instead of anonymous (TypeId, bool) tuples"
    );
    assert!(
        instantiation_result_rs.contains("pub(crate) struct InstantiationMemoResult")
            && instantiation_result_rs.contains("pub enum InstantiationTermination")
            && instantiation_result_rs.contains("termination: InstantiationTermination")
            && instantiation_result_rs
                .contains("fn from_walk(type_id: TypeId, termination: InstantiationTermination)")
            && instantiation_result_rs.contains("fn for_project_cache(")
            && instantiation_result_rs.contains("fn is_stable_for_project_cache(self) -> bool")
            && instantiation_api_rs.contains("ProjectInstantiationCacheLimitSnapshot::capture")
            && instantiation_api_rs.contains("InstantiationMemoResult::for_project_cache")
            && instantiation_api_rs.contains("InstantiationTermination::from_depth_exceeded")
            && instantiation_api_rs.contains("is_stable_for_project_cache()")
            && !instantiation_result_rs.contains("overflowed: bool")
            && !instantiation_result_rs
                .contains("fn from_walk(type_id: TypeId, depth_exceeded: bool)")
            && !instantiation_api_rs.contains("let limit_tripped ="),
        "project instantiation cache writes must consume InstantiationMemoResult stability instead of rebuilding a raw limit_tripped predicate"
    );
}

#[test]
fn narrowing_engine_keeps_request_stage_boundary() {
    let mod_rs = read_solver_source("narrowing/mod.rs");
    let request_rs = read_solver_source("narrowing/request.rs");
    let core_rs = read_solver_source("narrowing/core.rs");

    assert!(
        mod_rs.contains("pub mod request;"),
        "narrowing module must expose the named request stage"
    );
    assert!(
        request_rs.contains("pub struct NarrowingOptions")
            && request_rs.contains("pub struct NarrowingRequest")
            && request_rs.contains("pub(crate) struct NarrowTypeStableCacheKey"),
        "narrowing/request.rs must own the typed options, request, and cache-key stage"
    );
    assert!(
        core_rs.contains("use crate::narrowing::request::")
            && core_rs.contains("pub fn narrow_type_with_request"),
        "narrowing/core.rs must import from the request stage and expose the typed entry point"
    );
    assert!(
        !core_rs.contains("compiler_flags: u8"),
        "narrowing/core.rs must not own the anonymous packed compiler-flags byte — use NarrowingOptions"
    );
}

#[test]
fn relation_queries_keep_overflow_flags_on_relation_result() {
    let relation_queries_rs = read_solver_source("relations/relation_queries.rs");

    assert!(
        relation_queries_rs.contains("pub struct RelationResult")
            && relation_queries_rs.contains("fn relation_result_from_compat_checker")
            && relation_queries_rs.contains("fn relation_result_from_subtype_checker")
            && relation_queries_rs.contains("let result = match kind")
            && relation_queries_rs
                .contains("relation_result_from_compat_checker(kind, related, &checker)")
            && relation_queries_rs
                .contains("relation_result_from_subtype_checker(kind, related, &checker)")
            && !relation_queries_rs
                .contains("let (related, depth_exceeded, iteration_exceeded) = match kind"),
        "relation query dispatch must keep related/depth/iteration verdicts bundled \
         as RelationResult instead of passing around anonymous overflow tuples"
    );

    let conditional_phases_rs =
        read_solver_source("evaluation/evaluate_rules/conditional/phases.rs");
    assert!(
        conditional_phases_rs.contains("struct ConditionalSubtypeDepthEntry")
            && conditional_phases_rs.contains("fn enter() -> ConditionalSubtypeDepthEntry")
            && conditional_phases_rs
                .contains("let depth_entry = ConditionalSubtypeDepthGuard::enter()")
            && conditional_phases_rs.contains("depth_entry.prior_depth()")
            && conditional_phases_rs.contains("depth_entry.exit()")
            && !conditional_phases_rs
                .contains("let (prev_depth, depth_guard) = ConditionalSubtypeDepthGuard::enter()"),
        "conditional subtype depth probes must carry prior-depth plus RAII guard \
         as a named entry object instead of an anonymous tuple"
    );
}

// =============================================================================
// Identity bridge guard — DefId-as-SymbolId raw-symbol fallback budget (#14344)
// =============================================================================

/// Guard the solver-side `DefId`-as-`SymbolId` reinterpretation surface.
///
/// `TypeEnvironment::raw_symbol_fallback_def` recovers a real `DefId` when a
/// `Lazy(DefId)` actually wrapped a raw `SymbolId.0`. The two are independent
/// identity spaces that happen to collide on a raw `u32` (see
/// `crates/tsz-solver/src/def/resolver.rs` and
/// `docs/architecture/DEFID_RAW_SYMBOL_FALLBACK_PRODUCERS.md`). It is a
/// compatibility path for the non-canonical identity model tracked by
/// tsz-org/tsz#14344, and the documented intent is to keep this fallback budget
/// from growing.
///
/// This mirrors the checker's zero `.reference(...)` construction guard on the
/// solver side: it pins the number of call sites that reinterpret a `DefId` as a
/// `SymbolId` so the surface cannot GROW. The migration toward content-canonical
/// identity (#14344) should ratchet `BUDGET` DOWN to 0 as callers are retired; it
/// must never be raised. Counting `.raw_symbol_fallback_def(` (the method-call
/// form) excludes the `fn` definition and doc comments.
#[test]
fn solver_raw_symbol_fallback_def_budget_does_not_grow() {
    const BUDGET: usize = 4;
    const PATTERN: &str = ".raw_symbol_fallback_def(";

    let solver_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let sites = pattern_call_sites(&solver_src, PATTERN, false);

    assert!(
        sites.len() <= BUDGET,
        "Solver `raw_symbol_fallback_def` call sites grew to {} (budget {}). \
         Each reinterprets a `DefId` as a `SymbolId`; this surface must not grow \
         while the content-canonical identity migration (tsz-org/tsz#14344) is \
         retiring it. New code should resolve a real `DefId` and `lazy(def_id)` \
         instead. See docs/architecture/DEFID_RAW_SYMBOL_FALLBACK_PRODUCERS.md.\n\
         Call sites:\n{}",
        sites.len(),
        BUDGET,
        sites.join("\n"),
    );
}

/// Collect `dir`/**.rs source locations (`  rel:line`) whose non-comment text
/// contains `pattern`. When `skip_tests` is set, files under any `tests`
/// directory (e.g. `src/tests/...`) are ignored so in-tree fixtures do not count
/// against a budget. Shared by the `#14344` identity-bridge surface guards.
fn pattern_call_sites(dir: &Path, pattern: &str, skip_tests: bool) -> Vec<String> {
    let mut files = Vec::new();
    walk_rs_files(dir, &mut files);

    let mut sites = Vec::new();
    for path in &files {
        if skip_tests
            && path
                .components()
                .any(|component| component.as_os_str() == "tests")
        {
            continue;
        }
        let rel = path
            .strip_prefix(dir)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        let Ok(src) = fs::read_to_string(path) else {
            continue;
        };
        for (line_num, line) in src.lines().enumerate() {
            if line.trim_start().starts_with("//") {
                continue;
            }
            if line.contains(pattern) {
                sites.push(format!("  {}:{}", rel, line_num + 1));
            }
        }
    }
    sites
}

/// Guard the solver-side raw `reference(SymbolRef)` minting surface (#14344).
///
/// `TypeInterner::reference(SymbolRef(n))` mints a *zombie* `Lazy(DefId(n))`
/// whose numeric payload is really a `SymbolId`, not a store-registered `DefId`.
/// That conflation of two disjoint identity spaces is the lead evidence in
/// tsz-org/tsz#14344, and the source the companion
/// `solver_raw_symbol_fallback_def_budget_does_not_grow` guard exists to recover
/// from: every minting site is a future `raw_symbol_fallback_def` collision risk
/// (the documented `#13862` `HTMLDivElement` -> `FileSystemEntry` corruption).
///
/// The checker already forbids new `.reference(...)` construction outright
/// (budget 0). The solver still owns the two legacy minting points
/// (`intern/type_factory.rs`, `caches/query_cache.rs`); this pins them so the
/// surface can only SHRINK toward content-addressed `DefId`s. Ratchet `BUDGET`
/// DOWN to 0 as the sites are retired; never raise it.
#[test]
fn solver_raw_symbol_reference_minting_budget_does_not_grow() {
    const BUDGET: usize = 2;
    const PATTERN: &str = ".reference(";

    let solver_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let sites = pattern_call_sites(&solver_src, PATTERN, true);

    assert!(
        sites.len() <= BUDGET,
        "Solver raw `reference(SymbolRef)` minting sites grew to {} (budget {}). \
         Each mints a zombie `Lazy(DefId(symbol_id))` whose payload is a \
         `SymbolId`, not a registered `DefId` — the cross-identity root tracked by \
         tsz-org/tsz#14344 and the source of the `#13862` wrong-decl collision. \
         Resolve a real, store-registered `DefId` and use `lazy(def_id)` instead. \
         See docs/architecture/DEFID_RAW_SYMBOL_FALLBACK_PRODUCERS.md.\n\
         Minting sites:\n{}",
        sites.len(),
        BUDGET,
        sites.join("\n"),
    );
}

/// Guard that the content-addressed `DefId` generator stays a pure function of
/// declaration content (#14344).
///
/// `ContentAddressedDefIds` is the migration's target identity scheme: a `DefId`
/// must be `hash(name, file, span)`, never a function of allocation order. The
/// order-derived `next_id` counter may only *assign* an id for a previously
/// unseen hash (after `finish()`), never feed the hash itself. This pins that
/// boundary so a future edit cannot quietly fold an order/time/thread component
/// back into the canonical key — which would reintroduce the exact
/// allocation-order identity tsz-org/tsz#14344 removes.
#[test]
fn content_addressed_def_id_hashes_only_declaration_content() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/def/core/content_addressed.rs");
    let source = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));

    let body = source
        .split("fn get_or_create(")
        .nth(1)
        .and_then(|rest| rest.split("\n    }").next())
        .expect("content_addressed.rs must define ContentAddressedDefIds::get_or_create");

    // The content hash is everything fed to the hasher before it is finalized.
    let content_hash_region = body
        .split("hasher.finish()")
        .next()
        .expect("get_or_create must finalize the content hasher");

    for input in ["name.hash(", "file_id.hash(", "span_start.hash("] {
        assert!(
            content_hash_region.contains(input),
            "ContentAddressedDefIds::get_or_create must hash `{input}` — the \
             #14344 canonical id is a pure function of declaration content \
             (name, file, span)."
        );
    }

    for order_source in ["next_id", "fetch_add", "Instant", "thread::"] {
        assert!(
            !content_hash_region.contains(order_source),
            "ContentAddressedDefIds::get_or_create must not mix the order-derived \
             source `{order_source}` into the content hash (it may only assign the \
             id after `hasher.finish()`) — that is the allocation-order identity \
             tsz-org/tsz#14344 removes."
        );
    }
}
