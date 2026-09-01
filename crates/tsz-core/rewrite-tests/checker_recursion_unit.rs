use std::sync::Arc;

use crate::semantics::types::{
    Completion, DeferredBinaryOperator, DeferredType, DeferredUnaryOperator, LiteralProvenance,
    Property, TypeKind,
};
use crate::source::{DeclId, FileId};
use crate::{Compiler, CompilerOptions, SourceInput};

use super::*;

fn parameter(declaration: DeclId, index: u32) -> TypeKind {
    TypeKind::TypeParameter {
        declaration,
        index,
        name: "ignored".to_string(),
    }
}

fn with_checker(source: &str, test: impl FnOnce(&mut Checker<'_>)) {
    let options = CompilerOptions {
        no_emit: true,
        ..CompilerOptions::default()
    };
    let output = Compiler::new().compile(
        vec![SourceInput::new("case.ts", Arc::<str>::from(source))],
        &options,
    );
    let mut checker = Checker::new(&output.program, &options, &output.capabilities);
    test(&mut checker);
}

fn deferred_non_null(checker: &mut Checker<'_>, operand: TypeId) -> TypeId {
    checker
        .store
        .intern(TypeKind::Deferred(DeferredType::Unary {
            operator: DeferredUnaryOperator::NonNull,
            operand,
        }))
}

fn nested_non_null(checker: &mut Checker<'_>, mut operand: TypeId, count: usize) -> TypeId {
    for _ in 0..count {
        operand = deferred_non_null(checker, operand);
    }
    operand
}

fn deferred_string_add(checker: &mut Checker<'_>, left: TypeId) -> TypeId {
    checker
        .store
        .intern(TypeKind::Deferred(DeferredType::Binary {
            operator: DeferredBinaryOperator::Add,
            left,
            right: checker.store.builtins.string,
        }))
}

fn loop_reference(checker: &mut Checker<'_>) -> TypeId {
    let declaration = checker
        .program
        .files
        .iter()
        .flat_map(|file| &file.bindings.declarations)
        .find(|declaration| declaration.name == "Loop")
        .expect("Loop declaration")
        .id;
    checker.store.symbolic_reference(declaration, Vec::new())
}

#[test]
fn typed_growth_distinguishes_expansion_shrinking_and_permutation() {
    let declaration = DeclId {
        file: FileId(0),
        local: 0,
    };
    let other = DeclId {
        file: FileId(0),
        local: 1,
    };
    let kinds = [
        parameter(declaration, 0),
        TypeKind::Array(TypeId(0)),
        TypeKind::Array(TypeId(1)),
        parameter(declaration, 1),
    ];
    let kind = |ty: TypeId| kinds[ty.0 as usize].clone();
    let mut stack = ReferenceExpansionStack::new(ReferenceDemand::RequiredType);
    stack.push(TypeId(10), declaration, &[TypeId(0), TypeId(3)]);

    assert_eq!(
        stack.classify(TypeId(11), declaration, &[TypeId(1), TypeId(3)], &kind,),
        ReferenceRecursion::Generative
    );
    assert_eq!(
        stack.classify(TypeId(12), declaration, &[TypeId(3), TypeId(0)], &kind,),
        ReferenceRecursion::Distinct
    );
    assert_eq!(
        stack.classify(TypeId(13), declaration, &[TypeId(0), TypeId(3)], &kind,),
        ReferenceRecursion::Exact
    );
    assert_eq!(
        stack.classify(TypeId(14), other, &[TypeId(1), TypeId(3)], &kind,),
        ReferenceRecursion::Distinct
    );

    let mut shrinking = ReferenceExpansionStack::new(ReferenceDemand::RequiredType);
    shrinking.push(TypeId(20), declaration, &[TypeId(2)]);
    assert_eq!(
        shrinking.classify(TypeId(21), declaration, &[TypeId(1)], &kind),
        ReferenceRecursion::Distinct
    );
}

#[test]
fn structural_containment_terminates_on_a_cyclic_type_graph() {
    let kinds = [TypeKind::Array(TypeId(0)), TypeKind::String];
    assert!(!type_contains_nested(TypeId(0), TypeId(1), &|ty| {
        kinds[ty.0 as usize].clone()
    }));
}

#[test]
fn stable_force_results_retain_dependency_completion_without_entering_the_cache() {
    with_checker("", |checker| {
        let string = checker.store.builtins.string;
        let deferred = checker.store.deferred_template_value();
        let deferred_query = deferred_string_add(checker, deferred);
        assert_eq!(
            checker.force_type(deferred_query, 0),
            Completion::Complete(string)
        );
        assert_eq!(
            checker.completion.program(),
            crate::program::SemanticCompletion::Deferred
        );
        assert!(!checker.force_queries.contains_key(&deferred_query));

        let cycle = checker.store.deferred_template_value();
        checker.force_queries.insert(cycle, QueryState::Computing);
        let cycle_query = deferred_string_add(checker, cycle);
        assert_eq!(
            checker.force_type(cycle_query, 0),
            Completion::Complete(string)
        );
        assert_eq!(
            checker.completion.program(),
            crate::program::SemanticCompletion::Cycle
        );
        assert!(!checker.force_queries.contains_key(&cycle_query));

        let limit = nested_non_null(checker, string, 105);
        let limit_query = deferred_string_add(checker, limit);
        assert_eq!(
            checker.force_type(limit_query, 0),
            Completion::Complete(string)
        );
        assert_eq!(
            checker.completion.program(),
            crate::program::SemanticCompletion::Limit
        );
        assert!(!checker.force_queries.contains_key(&limit_query));
    });
}

#[test]
fn first_incomplete_operand_blocks_later_materialization_diagnostic_and_cache() {
    with_checker("type Loop = Loop;", |checker| {
        let string = checker.store.builtins.string;
        let materialized = deferred_non_null(checker, string);
        let diagnostic = loop_reference(checker);

        let deferred = checker.store.deferred_template_value();
        assert_eq!(
            checker.force_operands([deferred, materialized, diagnostic], 0),
            [
                Completion::Deferred,
                Completion::Deferred,
                Completion::Deferred,
            ]
        );
        assert!(!checker.force_queries.contains_key(&materialized));
        assert!(checker.diagnostics.is_empty());

        let cycle = checker.store.deferred_template_value();
        checker.force_queries.insert(cycle, QueryState::Computing);
        assert_eq!(
            checker.force_operands([cycle, materialized, diagnostic], 0),
            [Completion::Cycle, Completion::Cycle, Completion::Cycle]
        );
        assert!(!checker.force_queries.contains_key(&materialized));
        assert!(checker.diagnostics.is_empty());

        let limit = nested_non_null(checker, string, 105);
        assert_eq!(
            checker.force_operands([limit, materialized, diagnostic], 0),
            [Completion::Limit, Completion::Limit, Completion::Limit]
        );
        assert!(!checker.force_queries.contains_key(&materialized));
        assert!(checker.diagnostics.is_empty());
    });
}

#[test]
fn complete_operands_materialize_in_order_and_enter_the_existing_cache() {
    with_checker("", |checker| {
        let first_value = checker.store.builtins.string;
        let second_value = checker.store.builtins.number;
        let first = deferred_non_null(checker, first_value);
        let second = deferred_non_null(checker, second_value);

        assert_eq!(
            checker.force_operands([first, second], 0),
            [
                Completion::Complete(first_value),
                Completion::Complete(second_value),
            ]
        );
        assert!(matches!(
            checker.force_queries.get(&first),
            Some(QueryState::Ready(value)) if *value == first_value
        ));
        assert!(matches!(
            checker.force_queries.get(&second),
            Some(QueryState::Ready(value)) if *value == second_value
        ));
        assert!(checker.diagnostics.is_empty());
    });
}

#[test]
fn provisional_value_dependencies_never_enter_transitive_projection_caches() {
    with_checker(
        "const callback: (input: { x: number }) => void = input => {};",
        |checker| {
            let declaration = checker.program.files[0]
                .bindings
                .declarations
                .iter()
                .rev()
                .find(|declaration| declaration.name == "input")
                .expect("function-expression parameter")
                .id;
            checker.value_queries.insert(
                declaration,
                super::super::declaration_value::ValueQueryState::Provisional,
            );
            let value = checker
                .store
                .intern(TypeKind::Deferred(DeferredType::Value(declaration)));
            let index = checker.store.intern(TypeKind::LiteralString(
                "x".to_string(),
                LiteralProvenance::Regular,
            ));
            let indexed = checker
                .store
                .intern(TypeKind::Deferred(DeferredType::IndexedAccess {
                    object: value,
                    index,
                }));
            let property = checker
                .store
                .intern(TypeKind::Deferred(DeferredType::Property {
                    object: value,
                    name: "x".to_string(),
                }));
            let keyof = checker
                .store
                .intern(TypeKind::Deferred(DeferredType::KeyOf(value)));

            for query in [indexed, property, keyof] {
                assert!(matches!(
                    checker.force_type(query, 0),
                    Completion::Complete(_)
                ));
                assert!(!checker.force_queries.contains_key(&query));
            }

            let number = checker.store.builtins.number;
            let contextual = checker.store.object(vec![Property {
                name: "x".to_string(),
                ty: number,
                optional: false,
                readonly: false,
            }]);
            checker
                .parameter_type_overrides
                .insert(declaration, contextual);
            checker.value_queries.remove(&declaration);
            assert_eq!(checker.force_type(indexed, 0), Completion::Complete(number));
            assert!(!checker.force_queries.contains_key(&indexed));
        },
    );
}

#[test]
fn stable_binary_results_with_incomplete_dependencies_never_enter_value_caches() {
    // Delete the conditional-producer nonclaim when `Select<string>` evaluation
    // is claimed; until then its stable string product is usable but noncacheable.
    with_checker(
        concat!(
            "type Select<Value>=Value extends string?string:number;",
            "declare const deferred:Select<string>;",
            "const produced=deferred+'';",
        ),
        |checker| {
            let declaration = checker.program.files[0]
                .bindings
                .declarations
                .iter()
                .find(|declaration| declaration.name == "produced")
                .expect("produced declaration")
                .id;

            for _ in 0..2 {
                let result = checker.declaration_value_type(declaration);
                assert!(matches!(
                    result,
                    Completion::Complete(value)
                        if matches!(checker.store.kind(value), TypeKind::String)
                ));
                assert!(!matches!(
                    checker.value_queries.get(&declaration),
                    Some(super::super::declaration_value::ValueQueryState::Ready(_))
                ));
            }
        },
    );
}
