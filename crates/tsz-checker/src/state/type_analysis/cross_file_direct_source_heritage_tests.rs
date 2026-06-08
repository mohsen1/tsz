use crate::context::{CheckerContext, CheckerOptions};
use crate::diagnostics::diagnostic_codes;
use crate::query_boundaries::common::TypeInterner;
use crate::state::CheckerState;
use crate::test_utils::check_multi_file;
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_common::common::ModuleKind;
use tsz_parser::parser::ParserState;

fn parse_bound_source(
    source: &str,
) -> (
    Arc<tsz_parser::parser::node::NodeArena>,
    Arc<BinderState>,
    TypeInterner,
) {
    let mut parser = ParserState::new("fixture.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);
    (
        Arc::new(parser.get_arena().clone()),
        Arc::new(binder),
        TypeInterner::new(),
    )
}

#[test]
fn direct_cross_file_interface_lowering_expands_source_option_bag_heritage() {
    let (arena, binder, types) = parse_bound_source(
        r#"
                interface BaseShape { title: string; logo: string; }
                interface DerivedShape extends BaseShape { count: number; }
            "#,
    );
    let ctx = CheckerContext::new(
        arena.as_ref(),
        binder.as_ref(),
        &types,
        "fixture.ts".to_string(),
        CheckerOptions::default(),
    );
    let state = CheckerState { ctx };
    let derived_sym = binder
        .file_locals
        .get("DerivedShape")
        .expect("derived symbol");

    let (derived_type, params) = state
        .direct_cross_file_interface_lowering(
            derived_sym,
            binder.as_ref(),
            arena.as_ref(),
            false,
            true,
        )
        .expect("simple same-file option-bag heritage should lower directly");

    assert!(params.is_empty());
    for property in ["title", "logo", "count"] {
        let atom = types.intern_string(property);
        assert!(
            crate::query_boundaries::common::raw_property_type(
                state.ctx.types.as_type_database(),
                derived_type,
                atom,
            )
            .is_some(),
            "directly lowered derived interface should include {property}"
        );
    }
}

#[test]
fn direct_cross_file_interface_lowering_rejects_generic_source_heritage() {
    let (arena, binder, types) = parse_bound_source(
        r#"
                interface Boxed<T> { value: T; }
                interface Wrapped extends Boxed<string> { label: string; }
            "#,
    );
    let ctx = CheckerContext::new(
        arena.as_ref(),
        binder.as_ref(),
        &types,
        "fixture.ts".to_string(),
        CheckerOptions::default(),
    );
    let state = CheckerState { ctx };
    let wrapped_sym = binder.file_locals.get("Wrapped").expect("wrapped symbol");

    assert!(
        state
            .direct_cross_file_interface_lowering(
                wrapped_sym,
                binder.as_ref(),
                arena.as_ref(),
                false,
                true,
            )
            .is_none(),
        "generic source heritage stays on the child-checker path"
    );
}

/// A non-generic method signature whose parameter/return annotations are
/// option-bag lowerable now lowers on the direct path, and the lowered member
/// is callable.
#[test]
fn direct_cross_file_interface_lowering_admits_source_method_signatures() {
    let (arena, binder, types) = parse_bound_source(
        r#"
                interface Gauge {
                    label: string;
                    measure(sample: number, scale: number): number;
                    reset(): void;
                }
            "#,
    );
    let ctx = CheckerContext::new(
        arena.as_ref(),
        binder.as_ref(),
        &types,
        "fixture.ts".to_string(),
        CheckerOptions::default(),
    );
    let state = CheckerState { ctx };
    let gauge_sym = binder.file_locals.get("Gauge").expect("gauge symbol");

    let (gauge_type, params) = state
        .direct_cross_file_interface_lowering(
            gauge_sym,
            binder.as_ref(),
            arena.as_ref(),
            false,
            true,
        )
        .expect("method-bearing option-bag interface should lower directly");

    assert!(params.is_empty());
    let db = state.ctx.types.as_type_database();
    for property in ["label", "measure", "reset"] {
        let atom = types.intern_string(property);
        assert!(
            crate::query_boundaries::common::raw_property_type(db, gauge_type, atom).is_some(),
            "directly lowered interface should include {property}"
        );
    }
    for method in ["measure", "reset"] {
        let atom = types.intern_string(method);
        let member = crate::query_boundaries::common::raw_property_type(db, gauge_type, atom)
            .expect("method member type");
        assert!(
            crate::query_boundaries::common::has_call_signatures(db, member),
            "lowered method {method} should be callable"
        );
    }
}

/// Heritage expansion now flattens inherited method signatures too, so a derived
/// interface exposes both its own and its base's methods on the direct path.
#[test]
fn direct_cross_file_interface_lowering_expands_source_heritage_methods() {
    let (arena, binder, types) = parse_bound_source(
        r#"
                interface Loader {
                    open(target: string): void;
                    size: number;
                }
                interface CachingLoader extends Loader {
                    prime(count: number): boolean;
                }
            "#,
    );
    let ctx = CheckerContext::new(
        arena.as_ref(),
        binder.as_ref(),
        &types,
        "fixture.ts".to_string(),
        CheckerOptions::default(),
    );
    let state = CheckerState { ctx };
    let derived_sym = binder
        .file_locals
        .get("CachingLoader")
        .expect("derived symbol");

    let (derived_type, params) = state
        .direct_cross_file_interface_lowering(
            derived_sym,
            binder.as_ref(),
            arena.as_ref(),
            false,
            true,
        )
        .expect("method heritage should lower directly");

    assert!(params.is_empty());
    let db = state.ctx.types.as_type_database();
    for member in ["open", "size", "prime"] {
        let atom = types.intern_string(member);
        assert!(
            crate::query_boundaries::common::raw_property_type(db, derived_type, atom).is_some(),
            "directly lowered derived interface should include {member}"
        );
    }
    for method in ["open", "prime"] {
        let atom = types.intern_string(method);
        let member = crate::query_boundaries::common::raw_property_type(db, derived_type, atom)
            .expect("method member type");
        assert!(
            crate::query_boundaries::common::has_call_signatures(db, member),
            "inherited/own method {method} should be callable"
        );
    }
}

/// A method with its own type parameters needs the mature generic path.
#[test]
fn direct_cross_file_interface_lowering_rejects_generic_method_signature() {
    let (arena, binder, types) = parse_bound_source(
        r#"
                interface Mapper {
                    convert<T>(input: T): T;
                }
            "#,
    );
    let ctx = CheckerContext::new(
        arena.as_ref(),
        binder.as_ref(),
        &types,
        "fixture.ts".to_string(),
        CheckerOptions::default(),
    );
    let state = CheckerState { ctx };
    let mapper_sym = binder.file_locals.get("Mapper").expect("mapper symbol");

    assert!(
        state
            .direct_cross_file_interface_lowering(
                mapper_sym,
                binder.as_ref(),
                arena.as_ref(),
                false,
                true,
            )
            .is_none(),
        "generic method signatures stay on the child-checker path"
    );
}

/// A method whose parameter references a type the option-bag guard cannot prove
/// resolvable falls back to the child-checker path.
#[test]
fn direct_cross_file_interface_lowering_rejects_unresolvable_method_param() {
    let (arena, binder, types) = parse_bound_source(
        r#"
                interface Sink {
                    accept(value: Unresolved): void;
                }
            "#,
    );
    let ctx = CheckerContext::new(
        arena.as_ref(),
        binder.as_ref(),
        &types,
        "fixture.ts".to_string(),
        CheckerOptions::default(),
    );
    let state = CheckerState { ctx };
    let sink_sym = binder.file_locals.get("Sink").expect("sink symbol");

    assert!(
        state
            .direct_cross_file_interface_lowering(
                sink_sym,
                binder.as_ref(),
                arena.as_ref(),
                false,
                true,
            )
            .is_none(),
        "unresolvable method parameter types stay on the child-checker path"
    );
}

/// A method declaring an explicit `this` parameter needs the mature self path.
#[test]
fn direct_cross_file_interface_lowering_rejects_this_parameter_method() {
    let (arena, binder, types) = parse_bound_source(
        r#"
                interface Handle {
                    detach(this: Handle, force: boolean): void;
                }
            "#,
    );
    let ctx = CheckerContext::new(
        arena.as_ref(),
        binder.as_ref(),
        &types,
        "fixture.ts".to_string(),
        CheckerOptions::default(),
    );
    let state = CheckerState { ctx };
    let handle_sym = binder.file_locals.get("Handle").expect("handle symbol");

    assert!(
        state
            .direct_cross_file_interface_lowering(
                handle_sym,
                binder.as_ref(),
                arena.as_ref(),
                false,
                true,
            )
            .is_none(),
        "`this`-parameter methods stay on the child-checker path"
    );
}

/// End-to-end: a cross-file interface method (including one inherited through
/// heritage) is callable with correct arguments and reports `TS2345` on a
/// mismatched argument — proving the directly-lowered method type carries the
/// right parameter types through the real relation engine.
#[test]
fn cross_file_interface_method_argument_checks_match_tsc() {
    let files = &[
        (
            "./contract.ts",
            r#"
export interface Probe {
    inspect(token: string, depth: number): boolean;
}
export interface DeepProbe extends Probe {
    drill(levels: number): void;
}
"#,
        ),
        (
            "./main.ts",
            r#"
import type { DeepProbe } from "./contract";

declare const probe: DeepProbe;

const ok: boolean = probe.inspect("alpha", 3);
probe.drill(2);

probe.inspect("beta", "not-a-number");
"#,
        ),
    ];

    let diagnostics = check_multi_file(
        files,
        "./main.ts",
        CheckerOptions {
            module: ModuleKind::CommonJS,
            strict: true,
            ..CheckerOptions::default()
        },
    );

    let argument_errors: Vec<_> = diagnostics
        .iter()
        .filter(|diagnostic| {
            diagnostic.code
                == diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE
        })
        .collect();
    assert_eq!(
        argument_errors.len(),
        1,
        "exactly one mismatched argument should report TS2345, got: {:?}",
        diagnostics
            .iter()
            .map(|diagnostic| (diagnostic.code, diagnostic.message_text.as_str()))
            .collect::<Vec<_>>()
    );
}
