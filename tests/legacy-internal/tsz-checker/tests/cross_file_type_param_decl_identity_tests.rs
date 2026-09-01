//! Type-parameter declaration identity across scope-push paths (#13044).
//!
//! tsc has exactly one type per type-parameter declaration (symbol-keyed). tsz
//! mints `TypeParameter` `TypeId`s when pushing scopes, and historically two
//! push paths used different interning disciplines:
//! `push_type_parameters` minted per-declaration through
//! `intern_type_param_for_decl`, while `push_enclosing_type_parameters` (used
//! by method-signature computation) interned structurally. The same declared
//! class parameter `DB` therefore closed over *different* `TypeId`s in a
//! member annotation versus the `implements`-clause type arguments. A
//! cross-file generic alias annotation `ReferenceExpression<DB, TB>` then
//! never matched the instantiated interface constraint by identity, the
//! relation eagerly expanded both alias applications, and the
//! `DB`/`TB`-dependent union members mismatched as distinct type parameters —
//! false `TS2416` on every such method of an implementing class (Kysely
//! `SelectQueryBuilderImpl` family, #10663).
//!
//! Both push paths now mint through the `(name_node, info)`-keyed declaration
//! cache, which is also consulted for `DefId`-registered parameters (the
//! def-keyed slot is single-entry and the two-phase unconstrained/constrained
//! push pattern ping-pongs it).
//!
//! Harness note: the multi-file CLI witness (two files, alias module +
//! implementing class) does NOT reproduce under
//! `check_multi_file_with_libs` — that harness mis-resolves the
//! `implements` heritage symbol to an unrelated interface from the sibling
//! file (raw file-local `SymbolId` collision; the same pre-existing gap PR
//! #13137 hit). The identity invariant is therefore pinned directly at the
//! scope-push unit level, plus single-file behavioral controls.

use crate::context::{CheckerOptions, EnclosingClassInfo};
use crate::diagnostics::{Diagnostic, diagnostic_codes};
use crate::state::CheckerState;
use crate::test_utils::{check_multi_file_with_libs, load_lib_files};
use tsz_binder::BinderState;
use tsz_common::common::ModuleKind;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, ParserState};
use tsz_solver::{TypeId, construction::TypeInterner};

fn check(files: &[(&str, &str)], entry: &str) -> Vec<Diagnostic> {
    check_multi_file_with_libs(
        files,
        entry,
        CheckerOptions {
            module: ModuleKind::ESNext,
            strict: true,
            ..CheckerOptions::default()
        },
        &load_lib_files(&["es5.d.ts"]),
    )
}

fn ts2416_errors(diagnostics: &[Diagnostic]) -> Vec<String> {
    diagnostics
        .iter()
        .filter(|d| {
            d.code
                == diagnostic_codes::PROPERTY_IN_TYPE_IS_NOT_ASSIGNABLE_TO_THE_SAME_PROPERTY_IN_BASE_TYPE
        })
        .map(|d| d.message_text.to_string())
        .collect()
}

/// One `TypeId` per type-parameter declaration: the direct declaration push
/// (`push_type_parameters`, used when checking the class and resolving
/// `implements`-clause type arguments) and the enclosing-scope push
/// (`push_enclosing_type_parameters`, used by `call_signature_from_method`
/// when computing a member annotation) must bind the same names to the same
/// `TypeId`s — including the constrained second parameter, whose two-phase
/// (unconstrained, then refined) minting previously ping-ponged the
/// def-keyed cache. Repeated pushes must also stay stable.
#[test]
fn enclosing_and_direct_scope_pushes_mint_identical_type_param_ids() {
    let source = r#"
class Box<Alpha, Beta extends keyof Alpha> {
  pick(key: Beta): Alpha[Beta] {
    return null as any;
  }
}
"#;
    let mut parser = ParserState::new("box.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);
    let arena = parser.get_arena().clone();
    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        &arena,
        &binder,
        &types,
        "box.ts".to_string(),
        CheckerOptions {
            no_lib: true,
            ..CheckerOptions::default()
        },
    );

    // Locate the class declaration and its method.
    let source_file = arena.get_source_file_at(root).expect("source file data");
    let class_idx = source_file
        .statements
        .nodes
        .iter()
        .copied()
        .find(|&idx| {
            arena
                .get(idx)
                .is_some_and(|n| n.kind == syntax_kind_ext::CLASS_DECLARATION)
        })
        .expect("class declaration");
    let class_data = arena
        .get(class_idx)
        .and_then(|n| arena.get_class(n))
        .expect("class data");
    let method_idx: NodeIndex = class_data
        .members
        .nodes
        .iter()
        .copied()
        .find(|&idx| {
            arena
                .get(idx)
                .is_some_and(|n| n.kind == syntax_kind_ext::METHOD_DECLARATION)
        })
        .expect("method declaration");

    let scope_ids = |checker: &CheckerState<'_>| {
        (
            checker
                .ctx
                .type_parameter_scope
                .get("Alpha")
                .copied()
                .expect("Alpha in scope"),
            checker
                .ctx
                .type_parameter_scope
                .get("Beta")
                .copied()
                .expect("Beta in scope"),
        )
    };

    // Path A: direct declaration push (class check / implements-clause args).
    let (_, updates_a) = checker.push_type_parameters(&class_data.type_parameters);
    let direct_first = scope_ids(&checker);
    checker.pop_type_parameters(updates_a);

    // Path B: enclosing push from the method (member annotation context).
    let updates_b = checker.push_enclosing_type_parameters(method_idx);
    let enclosing = scope_ids(&checker);
    checker.pop_type_parameters(updates_b);

    // Path A again: repeated pushes must not re-mint (two-phase stability).
    let (_, updates_c) = checker.push_type_parameters(&class_data.type_parameters);
    let direct_second = scope_ids(&checker);
    checker.pop_type_parameters(updates_c);

    assert_eq!(
        direct_first, enclosing,
        "direct and enclosing scope pushes must bind the same TypeIds for the \
         same type-parameter declarations"
    );
    assert_eq!(
        direct_first, direct_second,
        "repeated direct pushes must reuse the declaration's TypeIds"
    );
}

/// An active class owns the canonical identity and recovery state of its type
/// parameters. Reconstructing a method signature must reuse those exact
/// binders instead of re-resolving an invalid constraint and minting a cleaner
/// but distinct type parameter for the same declaration.
#[test]
fn active_class_enclosing_push_reuses_error_constrained_binder_identity() {
    let source = r#"
class Box<Element extends MissingConstraint> {
  read(): Element {
    return null as any;
  }
}
"#;
    let mut parser = ParserState::new("box.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);
    let arena = parser.get_arena().clone();
    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        &arena,
        &binder,
        &types,
        "box.ts".to_string(),
        CheckerOptions {
            no_lib: true,
            ..CheckerOptions::default()
        },
    );

    let source_file = arena.get_source_file_at(root).expect("source file data");
    let class_idx = source_file
        .statements
        .nodes
        .iter()
        .copied()
        .find(|&idx| {
            arena
                .get(idx)
                .is_some_and(|n| n.kind == syntax_kind_ext::CLASS_DECLARATION)
        })
        .expect("class declaration");
    let class_data = arena
        .get(class_idx)
        .and_then(|n| arena.get_class(n))
        .expect("class data");
    let method_idx = class_data
        .members
        .nodes
        .iter()
        .copied()
        .find(|&idx| {
            arena
                .get(idx)
                .is_some_and(|n| n.kind == syntax_kind_ext::METHOD_DECLARATION)
        })
        .expect("method declaration");

    let (class_type_parameters, class_updates) =
        checker.push_type_parameters(&class_data.type_parameters);
    let class_type_parameter_ids = checker
        .exact_type_parameter_ids_in_scope(&class_type_parameters)
        .expect("canonical class parameter identities");
    let direct_id = class_type_parameter_ids[0];
    let direct_info = crate::query_boundaries::common::type_param_info(&types, direct_id)
        .expect("class parameter info");
    assert_eq!(
        direct_info.constraint,
        Some(TypeId::ERROR),
        "the canonical class binder must retain its invalid-constraint recovery state"
    );

    checker.ctx.enclosing_class = Some(EnclosingClassInfo {
        name: "Box".to_string(),
        class_idx,
        member_nodes: class_data.members.nodes.clone(),
        in_constructor: false,
        is_declared: false,
        in_static_property_initializer: false,
        in_static_member: false,
        has_super_call_in_current_constructor: false,
        cached_instance_this_type: None,
        type_param_names: vec!["Element".to_string()],
        class_type_parameters,
        class_type_parameter_ids,
        enclosing_async_depth: 0,
    });

    let enclosing_updates = checker.push_enclosing_type_parameters(method_idx);
    assert_eq!(
        checker.ctx.type_parameter_scope.get("Element").copied(),
        Some(direct_id),
        "a method signature must close over the active class's exact binder"
    );
    checker.pop_type_parameters(enclosing_updates);
    assert_eq!(
        checker.ctx.type_parameter_scope.get("Element").copied(),
        Some(direct_id),
        "popping the reconstructed scope must restore the active class binder"
    );

    checker.ctx.enclosing_class = None;
    checker.pop_type_parameters(class_updates);
}

/// Single-file behavioral control for the kysely `whereRef` shape (tsc 5.9.3
/// clean): a non-generic impl method whose parameter annotation is the same
/// generic alias instance as the generic interface method's type-parameter
/// constraint must not raise TS2416.
const SINGLE_SRC: &str = r#"
interface Expression<T> {
  readonly expressionType?: T | undefined;
}

interface RowsExpression<O> {
  readonly isRowsExpression: true;
  readonly expressionType?: O | undefined;
}

type OperandExpression<V> = Expression<V> | RowsExpression<Record<string, V>>;

interface ExpressionBuilder<DB, TB extends keyof DB> {
  ref(reference: TB & string): unknown;
}

type ExpressionOrFactory<DB, TB extends keyof DB, V> =
  | OperandExpression<V>
  | OperandExpressionFactory<DB, TB, V>;

type OperandExpressionFactory<DB, TB extends keyof DB, V> = (
  eb: ExpressionBuilder<DB, TB>,
) => OperandExpression<V>;

type AnyColumn<DB, TB extends keyof DB> = keyof DB[TB] & string;

type ReferenceExpression<DB, TB extends keyof DB> =
  | AnyColumn<DB, TB>
  | ExpressionOrFactory<DB, TB, any>;

interface SelectQueryBuilder<DB, TB extends keyof DB, O> {
  whereRef<LRE extends ReferenceExpression<DB, TB>, RRE extends ReferenceExpression<DB, TB>>(
    lhs: LRE,
    op: string,
    rhs: RRE,
  ): SelectQueryBuilder<DB, TB, O>;
}

class SelectQueryBuilderImpl<DB, TB extends keyof DB, O>
  implements SelectQueryBuilder<DB, TB, O>
{
  whereRef(
    lhs: ReferenceExpression<DB, TB>,
    op: string,
    rhs: ReferenceExpression<DB, TB>,
  ): SelectQueryBuilder<DB, TB, O> {
    return this;
  }
}
"#;

#[test]
fn single_file_generic_alias_param_annotation_implements_clean() {
    let diagnostics = check(&[("single.ts", SINGLE_SRC)], "single.ts");
    assert_eq!(
        ts2416_errors(&diagnostics),
        Vec::<String>::new(),
        "single-file whereRef shape must not raise TS2416; got: {diagnostics:#?}"
    );
}

/// Negative control (tsc emits TS2416 here too): the impl narrows `op` from
/// `string` to `number` — identity convergence must not over-suppress.
#[test]
fn single_file_genuine_mismatch_still_reported() {
    let bad = SINGLE_SRC.replace(
        "    lhs: ReferenceExpression<DB, TB>,\n    op: string,",
        "    lhs: ReferenceExpression<DB, TB>,\n    op: number,",
    );
    assert_ne!(bad, SINGLE_SRC, "mutation must apply");
    let diagnostics = check(&[("single.ts", bad.as_str())], "single.ts");
    let errors = ts2416_errors(&diagnostics);
    assert!(
        errors.iter().any(|m| m.contains("whereRef")),
        "impl `op: number` against interface `op: string` must raise TS2416; \
         got: {diagnostics:#?}"
    );
}
