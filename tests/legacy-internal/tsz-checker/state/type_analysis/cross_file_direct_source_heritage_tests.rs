use crate::context::{CheckerContext, CheckerOptions};
use crate::diagnostics::diagnostic_codes;
use crate::query_boundaries::common::TypeInterner;
use crate::state::CheckerState;
use crate::test_utils::{
    check_multi_file, check_multi_file_with_libs_unique_module_locals, load_compiled_lib_files,
};
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_common::common::{ModuleKind, ScriptTarget};
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

// -----------------------------------------------------------------------------
// #16308 — cross-file generic-heritage member resolution (working boundary).
//
// When an interface `extends` a *generic* base (`interface Crate<T> extends
// Array<T>`) and is consumed from another file, every member inherited from that
// base must remain visible at the use site — exactly mobx's `IObservableArray<T>
// extends Array<T>`. The arena-local heritage merge reads `extends` clauses
// through the *checking* file's arena, so the cross-arena `merge_cross_file_
// heritage` path is what actually supplies the base; if any body-computation
// path the receiver reads skips it, the inherited members vanish as `TS2339`.
//
// These pin the *reproducible* boundary of that family with the real
// `lib.es5.d.ts` and the driver's globally-unique-`SymbolId` invariant (so a
// green here is the production heritage mechanism, not a base-0 harness id
// collision). Every simple/adjacent shape below resolves correctly today —
// single hop, one- and two-level re-export barrels, an imported program-symbol
// base, a `declare global` augmentation base, `ReadonlyArray`, and a defaulted
// type parameter. They are controls: the still-open mobx row (#16308) needs the
// deep circular `internal.ts` barrel graph that prior sessions could not
// minimize, and closing it is the cross-arena canonical-materialize-once work
// tracked by #14345. These guard that work from silently regressing the shapes
// that already work.
// -----------------------------------------------------------------------------

/// Build a multi-file project with the real `lib.es5.d.ts` and the driver's
/// globally-unique-`SymbolId` invariant, returning the `TS2339` messages the
/// entry file reports (empty when every inherited member resolved).
fn generic_heritage_ts2339(files: &[(&str, &str)], entry: &str) -> Vec<String> {
    let lib_files = load_compiled_lib_files(&["lib.es5.d.ts"]);
    let diagnostics = check_multi_file_with_libs_unique_module_locals(
        files,
        entry,
        CheckerOptions {
            target: ScriptTarget::ES2020,
            module: ModuleKind::ESNext,
            module_explicitly_set: true,
            strict: true,
            ..CheckerOptions::default()
        },
        &lib_files,
    );
    diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == 2339)
        .map(|diagnostic| diagnostic.message_text.clone())
        .collect()
}

/// Single hop: `Crate<T> extends Array<T>` imported directly.
#[test]
fn cross_file_interface_extends_generic_lib_base_keeps_inherited_members() {
    let missing = generic_heritage_ts2339(
        &[
            (
                "decl.ts",
                "export interface Crate<T> extends Array<T> { extra(): T; }\n",
            ),
            (
                "main.ts",
                "import { Crate } from \"./decl\";\ndeclare const c: Crate<number>;\nc.map(v => v);\nc.length;\nc.extra();\n",
            ),
        ],
        "main.ts",
    );
    assert!(
        missing.is_empty(),
        "inherited generic-lib-base members must resolve, got TS2339: {missing:?}"
    );
}

/// One-level re-export barrel (`export * from "./decl"`), mobx's `internal.ts`
/// routing at depth 1.
#[test]
fn cross_file_interface_extends_generic_lib_base_through_barrel_keeps_members() {
    let missing = generic_heritage_ts2339(
        &[
            (
                "decl.ts",
                "export interface Crate<T> extends Array<T> { extra(): T; }\n",
            ),
            ("barrel.ts", "export * from \"./decl\";\n"),
            (
                "main.ts",
                "import { Crate } from \"./barrel\";\ndeclare const c: Crate<number>;\nc.map(v => v);\nc.length;\nc.extra();\n",
            ),
        ],
        "main.ts",
    );
    assert!(
        missing.is_empty(),
        "inherited generic-lib-base members must resolve through a barrel, got TS2339: {missing:?}"
    );
}

/// Two-level re-export barrel (`main -> barrel -> inner -> decl`).
#[test]
fn cross_file_interface_extends_generic_lib_base_through_nested_barrel_keeps_members() {
    let missing = generic_heritage_ts2339(
        &[
            (
                "decl.ts",
                "export interface Crate<T> extends Array<T> { extra(): T; }\n",
            ),
            ("inner.ts", "export * from \"./decl\";\n"),
            ("barrel.ts", "export * from \"./inner\";\n"),
            (
                "main.ts",
                "import { Crate } from \"./barrel\";\ndeclare const c: Crate<number>;\nc.map(v => v);\nc.length;\nc.extra();\n",
            ),
        ],
        "main.ts",
    );
    assert!(
        missing.is_empty(),
        "inherited members must resolve through a two-level barrel, got TS2339: {missing:?}"
    );
}

/// The base is a generic *program-symbol* interface imported into the derived
/// module (not a lib global), consumed a further hop away.
#[test]
fn cross_file_interface_extends_imported_generic_program_base_keeps_members() {
    let missing = generic_heritage_ts2339(
        &[
            (
                "base.ts",
                "export interface Bag<T> { grab(): T; size: number; }\n",
            ),
            (
                "decl.ts",
                "import { Bag } from \"./base\";\nexport interface Crate<T> extends Bag<T> { extra(): T; }\n",
            ),
            (
                "main.ts",
                "import { Crate } from \"./decl\";\ndeclare const c: Crate<number>;\nc.grab();\nc.size;\nc.extra();\n",
            ),
        ],
        "main.ts",
    );
    assert!(
        missing.is_empty(),
        "inherited imported-program-base members must resolve, got TS2339: {missing:?}"
    );
}

/// The base is a generic interface introduced by a `declare global`
/// augmentation in another module.
#[test]
fn cross_file_interface_extends_declare_global_generic_base_keeps_members() {
    let missing = generic_heritage_ts2339(
        &[
            (
                "globals.ts",
                "export {};\ndeclare global { interface GBag<T> { grab(): T; size: number; } }\n",
            ),
            (
                "decl.ts",
                "export interface Crate<T> extends GBag<T> { extra(): T; }\n",
            ),
            (
                "main.ts",
                "import { Crate } from \"./decl\";\ndeclare const c: Crate<number>;\nc.grab();\nc.size;\nc.extra();\n",
            ),
        ],
        "main.ts",
    );
    assert!(
        missing.is_empty(),
        "inherited declare-global-base members must resolve, got TS2339: {missing:?}"
    );
}

/// `extends ReadonlyArray<T>` — the base's own-name and variance differ from
/// `Array`, so it exercises a distinct lib generic.
#[test]
fn cross_file_interface_extends_generic_readonly_array_base_keeps_members() {
    let missing = generic_heritage_ts2339(
        &[
            (
                "decl.ts",
                "export interface Crate<T> extends ReadonlyArray<T> { extra(): T; }\n",
            ),
            (
                "main.ts",
                "import { Crate } from \"./decl\";\ndeclare const c: Crate<number>;\nc.map(v => v);\nc.length;\nc.extra();\n",
            ),
        ],
        "main.ts",
    );
    assert!(
        missing.is_empty(),
        "inherited ReadonlyArray members must resolve, got TS2339: {missing:?}"
    );
}

/// A defaulted type parameter on the derived interface must not perturb the
/// inherited member set at a fully-applied use site.
#[test]
fn cross_file_interface_extends_generic_lib_base_with_default_type_param_keeps_members() {
    let missing = generic_heritage_ts2339(
        &[
            (
                "decl.ts",
                "export interface Crate<T = number> extends Array<T> { extra(): T; }\n",
            ),
            (
                "main.ts",
                "import { Crate } from \"./decl\";\ndeclare const c: Crate;\nc.map(v => v);\nc.length;\nc.extra();\n",
            ),
        ],
        "main.ts",
    );
    assert!(
        missing.is_empty(),
        "inherited members must resolve with a defaulted type param, got TS2339: {missing:?}"
    );
}
