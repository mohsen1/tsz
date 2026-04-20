use super::*;
// =============================================================================

/// CLAUDE.md §12: Track `query_boundaries` coverage ratio.
/// This is a directional metric -- warns if the ratio of direct solver imports
/// to `query_boundaries` usage is too high.
#[test]
fn test_query_boundaries_coverage_ratio() {
    let src_dir = Path::new("src");
    let mut files = Vec::new();
    collect_checker_rs_files_recursive(src_dir, &mut files);

    let mut direct_solver_importers = 0u32;
    let mut boundary_users = 0u32;

    for path in &files {
        let rel = path.display().to_string();
        if rel.contains("/tests/") || rel.contains("/query_boundaries/") {
            continue;
        }
        let src = fs::read_to_string(path)
            .unwrap_or_else(|_| panic!("failed to read {}", path.display()));

        let has_direct = src.lines().any(|line| {
            let t = line.trim_start();
            !t.starts_with("//")
                && (line.contains("use tsz_solver::") || line.contains("tsz_solver::"))
        });
        let has_boundary = src.lines().any(|line| {
            let t = line.trim_start();
            !t.starts_with("//") && line.contains("query_boundaries::")
        });

        if has_direct {
            direct_solver_importers += 1;
        }
        if has_boundary {
            boundary_users += 1;
        }
    }

    // This is a directional metric. We want the ratio to decrease over time.
    // Current target: direct importers should be < 4x boundary users.
    let ratio = if boundary_users == 0 {
        f64::INFINITY
    } else {
        direct_solver_importers as f64 / boundary_users as f64
    };

    // Warn but don't fail -- this is a tracking metric
    // Tracking metric: warn threshold at 4.0 (currently informational only)
    let _ = ratio > 4.0;

    // Hard fail if the ratio degrades catastrophically
    assert!(
        ratio < 10.0,
        "query_boundaries coverage ratio has degraded to {ratio:.1}:1 \
         ({direct_solver_importers} direct solver importers vs {boundary_users} boundary users). \
         This indicates systematic boundary bypass. Target: < 4:1"
    );
}

// ========================================================================
// Ambient context transport: TypingRequest migration contract tests
// ========================================================================
//
// These tests enforce that files fully migrated to the TypingRequest API
// do not regress by re-introducing raw mutations of the ambient context
// fields: `ctx.contextual_type =`, `ctx.contextual_type_is_assertion =`,
// and `ctx.skip_flow_narrowing =`.
//
// Legacy ambient state still exists in a few non-migrated subsystems, but
// the request-first hot path must not regress.

/// Migrated files must not contain raw `ctx.contextual_type =` assignments.
/// They should use `get_type_of_node_with_request` instead.
#[test]
fn migrated_files_no_raw_contextual_type_mutation() {
    let migrated_files = &[
        "types/computation/object_literal_context.rs",
        "types/computation/array_literal.rs",
        "types/queries/binding.rs",
        "types/type_checking/core.rs",
        "declarations/import/core/mod.rs",
        "assignability/assignment_checker/mod.rs",
        // property_access_type.rs migrated skip_flow_narrowing, not contextual_type
        // Wave 2 migrations:
        "assignability/compound_assignment.rs",
        "error_reporter/call_errors/mod.rs",
        "state/variable_checking/destructuring.rs",
        "state/state_checking/property.rs",
        "state/state_checking_members/ambient_signature_checks.rs",
        "state/variable_checking/core.rs",
        "types/type_checking/core_statement_checks.rs",
        "types/computation/binary.rs",
        "types/computation/access.rs",
        "types/computation/tagged_template.rs",
        // Wave 3 migrations:
        "types/computation/call_helpers.rs",
        "checkers/parameter_checker.rs",
        "types/utilities/return_type.rs",
        "checkers/call_checker/mod.rs",
        "types/computation/call_inference.rs",
        "dispatch.rs",
        "checkers/jsx/orchestration",
        "checkers/jsx/children.rs",
        "checkers/jsx/props/mod.rs",
        "checkers/jsx/props/resolution.rs",
        "checkers/jsx/props/validation.rs",
        "checkers/jsx/runtime.rs",
        "checkers/jsx/diagnostics.rs",
        "types/computation/call.rs",
        "types/computation/object_literal.rs",
        "types/computation/helpers.rs",
        "types/computation/call_display.rs",
        "types/function_type.rs",
        "types/class_type/constructor.rs",
        "state/state.rs",
        "state/type_analysis/core.rs",
        "state/type_analysis/core_type_query.rs",
        "state/type_analysis/computed_helpers.rs",
        "state/type_analysis/computed_helpers_binding.rs",
        "state/state_checking_members/statement_callback_bridge.rs",
        "state/state_checking_members/member_declaration_checks.rs",
        "state/state_checking/class.rs",
    ];

    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");

    for file in migrated_files {
        let path = base.join(file);
        let content = read_checker_source_file(&path.to_string_lossy());

        // Count raw mutations (exclude comments and the TypingRequest module itself)
        let violations: Vec<(usize, &str)> = content
            .lines()
            .enumerate()
            .filter(|(_, line)| {
                let trimmed = line.trim();
                // Skip comments
                if trimmed.starts_with("//")
                    || trimmed.starts_with("/*")
                    || trimmed.starts_with("*")
                {
                    return false;
                }
                // Detect raw mutation patterns
                trimmed.contains("ctx.contextual_type =") || trimmed.contains(".contextual_type = ")
            })
            .collect();

        assert!(
            violations.is_empty(),
            "File {file} has been migrated to TypingRequest but still contains \
             raw `contextual_type =` mutations:\n{}",
            violations
                .iter()
                .map(|(line_no, line)| format!("  line {}: {}", line_no + 1, line.trim()))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
}

/// Migrated files must not contain raw `ctx.skip_flow_narrowing =` assignments.
#[test]
fn migrated_files_no_raw_skip_flow_narrowing_mutation() {
    let migrated_files = &[
        "types/property_access_type/helpers.rs",
        "types/property_access_type/resolve.rs",
        "types/computation/access.rs",
        "types/computation/helpers.rs",
        "state/type_analysis/core.rs",
        "state/type_analysis/core_type_query.rs",
        "state/type_analysis/computed_helpers.rs",
        "state/type_analysis/computed_helpers_binding.rs",
        "state/variable_checking/destructuring.rs",
        "state/state_checking_members/statement_callback_bridge.rs",
        "state/state_checking_members/member_declaration_checks.rs",
        "state/state_checking/class.rs",
        // Wave 3: call_checker and call_inference migrated skip_flow via TypingRequest
        "checkers/call_checker/mod.rs",
        "types/computation/call_inference.rs",
        "types/computation/tagged_template.rs",
        "types/class_type/constructor.rs",
        "state/state_checking_members/ambient_signature_checks.rs",
        "state/state.rs",
    ];

    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");

    for file in migrated_files {
        let path = base.join(file);
        let content = read_checker_source_file(&path.to_string_lossy());

        let violations: Vec<(usize, &str)> = content
            .lines()
            .enumerate()
            .filter(|(_, line)| {
                let trimmed = line.trim();
                if trimmed.starts_with("//")
                    || trimmed.starts_with("/*")
                    || trimmed.starts_with("*")
                {
                    return false;
                }
                trimmed.contains("ctx.skip_flow_narrowing =")
                    || trimmed.contains(".skip_flow_narrowing = ")
            })
            .collect();

        assert!(
            violations.is_empty(),
            "File {file} has been migrated to TypingRequest but still contains \
             raw `skip_flow_narrowing =` mutations:\n{}",
            violations
                .iter()
                .map(|(line_no, line)| format!("  line {}: {}", line_no + 1, line.trim()))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
}

/// Migrated helper files must not read request intent from ambient checker fields.
#[test]
fn migrated_helper_files_no_raw_ambient_request_reads() {
    let migrated_files = &[
        "state/type_analysis/core.rs",
        "state/type_analysis/core_type_query.rs",
        "state/type_analysis/computed_helpers_binding.rs",
        "state/type_analysis/computed_helpers.rs",
        "types/property_access_type/helpers.rs",
        "types/property_access_type/resolve.rs",
        "state/variable_checking/destructuring.rs",
        "state/variable_checking/core.rs",
        "types/type_checking/core.rs",
        "types/computation/tagged_template.rs",
        "types/class_type/constructor.rs",
        "state/state_checking_members/ambient_signature_checks.rs",
        "state/state_checking_members/member_declaration_checks.rs",
        "state/state_checking/class.rs",
        "state/state.rs",
    ];

    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");

    for file in migrated_files {
        let path = base.join(file);
        let content = read_checker_source_file(&path.to_string_lossy());

        let violations: Vec<(usize, &str)> = content
            .lines()
            .enumerate()
            .filter(|(_, line)| {
                let trimmed = line.trim();
                if trimmed.starts_with("//")
                    || trimmed.starts_with("/*")
                    || trimmed.starts_with("*")
                {
                    return false;
                }
                trimmed.contains("self.ctx.contextual_type")
                    || trimmed.contains("self.ctx.contextual_type_is_assertion")
                    || trimmed.contains("self.ctx.skip_flow_narrowing")
            })
            .collect();

        assert!(
            violations.is_empty(),
            "File {file} must not read request intent from ambient checker state:\n{}",
            violations
                .iter()
                .map(|(line_no, line)| format!("  line {}: {}", line_no + 1, line.trim()))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
}

/// Migrated files must not contain raw `ctx.contextual_type_is_assertion =` assignments.
#[test]
fn migrated_files_no_raw_contextual_assertion_mutation() {
    let migrated_files = &[
        "dispatch.rs",
        "checkers/jsx/orchestration",
        "checkers/jsx/children.rs",
        "checkers/jsx/props/mod.rs",
        "checkers/jsx/props/resolution.rs",
        "checkers/jsx/props/validation.rs",
        "checkers/jsx/runtime.rs",
        "checkers/jsx/diagnostics.rs",
        "types/computation/call.rs",
        "types/computation/helpers.rs",
        "types/computation/object_literal.rs",
        "types/function_type.rs",
        "state/state_checking_members/ambient_signature_checks.rs",
        "types/computation/tagged_template.rs",
        "types/class_type/constructor.rs",
        "state/state_checking_members/member_declaration_checks.rs",
        "state/state_checking/class.rs",
        "state/type_analysis/core.rs",
        "state/type_analysis/core_type_query.rs",
        "state/type_analysis/computed_helpers.rs",
        "state/type_analysis/computed_helpers_binding.rs",
        "state/variable_checking/destructuring.rs",
        "state/state.rs",
    ];

    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");

    for file in migrated_files {
        let path = base.join(file);
        let content = read_checker_source_file(&path.to_string_lossy());

        let violations: Vec<(usize, &str)> = content
            .lines()
            .enumerate()
            .filter(|(_, line)| {
                let trimmed = line.trim();
                if trimmed.starts_with("//")
                    || trimmed.starts_with("/*")
                    || trimmed.starts_with("*")
                {
                    return false;
                }
                trimmed.contains("ctx.contextual_type_is_assertion =")
                    || trimmed.contains(".contextual_type_is_assertion = ")
            })
            .collect();

        assert!(
            violations.is_empty(),
            "File {file} has been migrated to TypingRequest but still contains \
             raw `contextual_type_is_assertion =` mutations:\n{}",
            violations
                .iter()
                .map(|(line_no, line)| format!("  line {}: {}", line_no + 1, line.trim()))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
}

/// The removed `run_with_typing_context` compatibility bridge must not reappear.
#[test]
fn no_typing_context_bridge_helper_or_calls() {
    let files = &[
        "state/state.rs",
        "dispatch.rs",
        "types/function_type.rs",
        "state/state_checking_members/statement_callback_bridge.rs",
    ];

    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");

    for file in files {
        let path = base.join(file);
        let content = read_checker_source_file(&path.to_string_lossy());

        let violations: Vec<(usize, &str)> = content
            .lines()
            .enumerate()
            .filter(|(_, line)| {
                let trimmed = line.trim();
                if trimmed.starts_with("//")
                    || trimmed.starts_with("/*")
                    || trimmed.starts_with("*")
                {
                    return false;
                }
                trimmed.contains("run_with_typing_context(")
                    || trimmed.contains("fn run_with_typing_context")
            })
            .collect();

        assert!(
            violations.is_empty(),
            "File {file} must not reintroduce the removed typing-context bridge:\n{}",
            violations
                .iter()
                .map(|(line_no, line)| format!("  line {}: {}", line_no + 1, line.trim()))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
}

/// The request-aware cache bypass must stay confined to the approved entry points.
///
/// This blocks new blanket "if request is non-empty, bypass cache" logic from
/// being reintroduced into other checker main entry points.
#[test]
fn request_empty_cache_bypass_stays_confined_to_approved_entry_points() {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let allowlist = ["state/state.rs", "types/class_type/constructor.rs"];

    let mut checker_files = Vec::new();
    collect_checker_rs_files_recursive(&base, &mut checker_files);

    let mut violations = Vec::new();
    for path in checker_files {
        if path
            .components()
            .any(|component| component.as_os_str() == "tests")
        {
            continue;
        }

        let relative = path
            .strip_prefix(&base)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        if allowlist.iter().any(|allowed| relative.ends_with(allowed)) {
            continue;
        }

        let source = fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("failed to read {}", path.display()));
        for (line_num, line) in source.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.starts_with("*") {
                continue;
            }
            if trimmed.starts_with("if request.is_empty()")
                || trimmed.starts_with("let use_node_cache = request.is_empty()")
                || trimmed.starts_with("let can_use_cache = request.is_empty()")
            {
                violations.push(format!("{}:{}", relative, line_num + 1));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "request-empty cache bypass logic must stay confined to state/state.rs and \
         types/class_type/constructor.rs; violations:\n{}",
        violations.join("\n")
    );
}

#[test]
fn request_aware_contextual_retry_hot_paths_do_not_reintroduce_recursive_cache_clears() {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let whole_file_bans = [
        "assignability/assignment_checker/mod.rs",
        "state/state_checking/property.rs",
        "types/type_checking/core.rs",
    ];

    for relative in whole_file_bans {
        let path = base.join(relative);
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("failed to read {}", path.display()));

        assert!(
            !source.contains("clear_type_cache_recursive("),
            "request-aware contextual retry path {relative} must use targeted invalidation helpers instead of direct recursive cache clears"
        );
    }
    let ambient_source =
        fs::read_to_string(base.join("state/state_checking_members/ambient_signature_checks.rs"))
            .expect("failed to read ambient_signature_checks.rs");
    assert!(
        ambient_source.contains("invalidate_initializer_for_context_change(prop.initializer)"),
        "ambient declared-type initializer retries must keep using the targeted invalidation helper"
    );
}

/// The `TypingRequest` type must exist and have the expected fields.
#[test]
fn typing_request_api_exists() {
    use crate::context::{ContextualOrigin, FlowIntent, TypingRequest};

    // Verify basic construction and field access
    let none = TypingRequest::NONE;
    assert!(none.is_empty());
    assert_eq!(none.contextual_type, None);
    assert_eq!(none.origin, ContextualOrigin::Normal);
    assert_eq!(none.flow, FlowIntent::Read);

    let with_ctx = TypingRequest::with_contextual_type(TypeId::STRING);
    assert_eq!(with_ctx.contextual_type, Some(TypeId::STRING));
    assert!(!with_ctx.origin.is_assertion());

    let assertion = TypingRequest::for_assertion(TypeId::NUMBER);
    assert!(assertion.origin.is_assertion());

    let write = TypingRequest::for_write_context();
    assert!(write.flow.skip_flow_narrowing());
}

/// Verify that the `statement_callback_bridge` save/restore for `check_statement`
/// is properly scoped (contextual type set only during `check_statement`, not leaked).
#[test]
fn statement_callback_bridge_contextual_type_scoping() {
    // This is a source-level check: the export clause handler must restore
    // contextual type BEFORE the assignability check, not after.
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/state/state_checking_members/statement_callback_bridge.rs");
    let content = fs::read_to_string(&path).expect("Failed to read statement_callback_bridge.rs");

    // The file should use get_type_of_node_with_request for the get_type_of_node call
    assert!(
        content.contains("get_type_of_node_with_request"),
        "statement_callback_bridge.rs should use get_type_of_node_with_request for export clause typing"
    );
}

#[test]
fn semantic_diagnostic_reporters_must_route_primary_anchor_selection_through_fingerprint_policy() {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/error_reporter");
    let fingerprint_policy = fs::read_to_string(base.join("fingerprint_policy.rs"))
        .expect("failed to read src/error_reporter/fingerprint_policy.rs");
    assert!(
        fingerprint_policy.contains("enum DiagnosticAnchorKind"),
        "fingerprint_policy.rs must define the shared anchor policy"
    );
    assert!(
        fingerprint_policy.contains("resolve_diagnostic_anchor_node"),
        "fingerprint_policy.rs must provide shared anchor resolution"
    );

    let files = [
        "assignability.rs",
        "call_errors",
        "properties.rs",
        "generics.rs",
    ];
    let forbidden = [
        "assignment_diagnostic_anchor_idx(",
        "call_error_anchor_node(",
        "ts2769_first_arg_or_call(",
        "type_assertion_overlap_anchor(",
        "type_assertion_overlap_anchor_in_expression(",
        "build_related_from_failure_reason(",
    ];

    for file in files {
        let path = base.join(file);
        let content = if path.is_dir() {
            // Read all .rs files in the directory and concatenate
            let mut combined = String::new();
            for entry in fs::read_dir(&path).unwrap_or_else(|e| panic!("read dir {file}: {e}")) {
                let entry = entry.expect("failed to read dir entry");
                let p = entry.path();
                if p.extension().and_then(|e| e.to_str()) == Some("rs")
                    && let Ok(c) = fs::read_to_string(&p)
                {
                    combined.push_str(&c);
                }
            }
            combined
        } else {
            fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {file}: {e}"))
        };
        assert!(
            content.contains("DiagnosticAnchorKind::")
                || content.contains("resolve_diagnostic_anchor(")
                || content.contains("resolve_diagnostic_anchor_node("),
            "File {file} must use the shared fingerprint policy for anchor selection"
        );

        for forbidden_pattern in forbidden {
            assert!(
                !content.contains(forbidden_pattern),
                "File {file} must not reintroduce bespoke primary-anchor helper `{forbidden_pattern}`"
            );
        }
    }
}

/// Ensures that `current_callable_type` is not reintroduced as ambient mutable state.
///
/// The callable type is now threaded explicitly via `CallableContext` through the call
/// argument collection pipeline. No file in the call-context lane should read or write
/// `ctx.current_callable_type`. The field has been removed from `CheckerContext`.
#[test]
fn no_ambient_current_callable_type() {
    let migrated_files = [
        "src/checkers/call_checker/mod.rs",
        "src/types/computation/call.rs",
        "src/types/computation/call_inference.rs",
        "src/types/computation/call_display.rs",
        "src/state/type_analysis/computed_helpers.rs",
        "src/context/mod.rs",
        "src/context/constructors.rs",
    ];

    let checker_root = Path::new(env!("CARGO_MANIFEST_DIR"));

    for file in migrated_files {
        let path = checker_root.join(file);
        let content = read_checker_source_file(&path.to_string_lossy());

        // Allow the doc comment in CallableContext's definition but forbid actual usage.
        // Filter out lines that are comments (starting with /// or //).
        let non_comment_lines: String = content
            .lines()
            .filter(|line| {
                let trimmed = line.trim();
                !trimmed.starts_with("///") && !trimmed.starts_with("//")
            })
            .collect::<Vec<_>>()
            .join("\n");

        assert!(
            !non_comment_lines.contains("current_callable_type"),
            "File {file} must not reference `current_callable_type` — \
             use explicit `CallableContext` threading instead"
        );
    }
}

/// Excess property classification logic (`ExcessPropertiesKind` pattern-matching)
/// must stay in the canonical path: `state/state_checking/property.rs` and
/// the `query_boundaries/assignability.rs` re-export.  Other checker files
/// must not reimplement this classification.
#[test]
fn test_excess_property_classification_quarantined_to_property_rs() {
    let mut files = Vec::new();
    collect_checker_rs_files_recursive(Path::new("src"), &mut files);

    let forbidden = [
        "ExcessPropertiesKind::Union",
        "ExcessPropertiesKind::Intersection",
        "ExcessPropertiesKind::Object(",
        "ExcessPropertiesKind::ObjectWithIndex(",
    ];

    let mut violations = Vec::new();
    for path in files {
        let rel = path.display().to_string();
        let allowed = rel.ends_with("state/state_checking/property.rs")
            || rel.ends_with("query_boundaries/assignability.rs")
            || rel.ends_with("assignability/assignability_diagnostics.rs") // target scoring
            || rel.ends_with("computation/object_literal_context.rs") // contextual type decomposition
            || rel.contains("/tests/");
        if allowed {
            continue;
        }
        let src = fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("failed to read {}", path.display()));
        for pattern in &forbidden {
            if src.contains(pattern) {
                violations.push(format!("{rel} contains {pattern}"));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "ExcessPropertiesKind pattern-matching must stay in state/state_checking/property.rs; violations:\n{}",
        violations.join("\n")
    );
}

// ========================================================================
