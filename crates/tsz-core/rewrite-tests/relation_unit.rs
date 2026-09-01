use std::{collections::HashMap, sync::Arc};

use crate::source::{DeclId, FileId};
use crate::{CompileExitStatus, Compiler, CompilerOptions, SemanticCompletion, SourceInput};

use super::*;
use crate::semantics::types::{
    DeferredLogicalOperator, DeferredType, InvalidType, LiteralProvenance, ParameterType, Signature,
};

const COVARIANT_LIBRARY_REFERENCE: DeclId = DeclId {
    file: FileId(u32::MAX),
    local: 0,
};

/// Relate two types in one query-local session.
///
/// TypeScript's recursive structural comparison is coinductive: a repeated
/// active `(source, target, mode)` pair is provisionally related. Keeping that
/// identity before forcing deferred references lets recursive symbolic shapes
/// meet the same active pair instead of materializing without a bound.
fn relate<C: RelationContext>(
    context: &mut C,
    source: TypeId,
    target: TypeId,
    mode: RelationMode,
) -> Result<(), RelationFailure> {
    relate_types(context, source, target, mode)
}

struct TestContext {
    kinds: Vec<TypeKind>,
    completions: HashMap<TypeId, Completion<TypeId>>,
    evaluator_depth: usize,
}

impl RelationContext for TestContext {
    fn force_type(&mut self, ty: TypeId, depth: usize) -> Completion<TypeId> {
        assert_eq!(depth, self.evaluator_depth);
        self.completions
            .get(&ty)
            .cloned()
            .unwrap_or(Completion::Complete(ty))
    }

    fn type_kind(&self, ty: TypeId) -> TypeKind {
        self.kinds[ty.0 as usize].clone()
    }

    fn generative_reference_supported(&self, _declaration: DeclId, _arguments: &[TypeId]) -> bool {
        true
    }

    fn generative_relation_frame_supported(
        &self,
        _declaration: DeclId,
        _arguments: &[TypeId],
    ) -> bool {
        true
    }

    fn library_reference_arguments_are_covariant(&self, declaration: DeclId) -> bool {
        declaration == COVARIANT_LIBRARY_REFERENCE
    }

    fn strict_null_checks(&self) -> bool {
        true
    }

    fn canonical_union(&mut self, members: &[TypeId]) -> TypeId {
        if let [only] = members {
            return *only;
        }
        let id = TypeId(self.kinds.len() as u32);
        self.kinds.push(TypeKind::Union(members.to_vec()));
        id
    }
}

fn property(name: &str, ty: u32) -> Property {
    Property {
        name: name.to_string(),
        ty: TypeId(ty),
        optional: false,
        readonly: false,
    }
}

#[test]
fn equal_object_shapes_keep_authored_order_across_allocation_and_root_order() {
    let options = CompilerOptions {
        no_emit: true,
        strict: true,
        ..CompilerOptions::default()
    };
    let ab = Arc::<str>::from(concat!(
        "declare const ab:{alpha:string;beta:number};const abOk:{beta:number;alpha:string}=ab;",
        "const abBad:{alpha:\"a\";beta:1}=ab;declare function takesAB(value:{alpha:\"a\";beta:1}):void;takesAB(ab);",
    ));
    let ba = Arc::<str>::from(concat!(
        "declare const ba:{beta:number;alpha:string};const baOk:{alpha:string;beta:number}=ba;",
        "const baBad:{beta:1;alpha:\"a\"}=ba;declare function takesBA(value:{beta:1;alpha:\"a\"}):void;takesBA(ba);",
    ));
    let compiler = Compiler::new();
    let run = |inputs| compiler.compile(inputs, &options);
    let verify = |output: &crate::CompileOutput| {
        assert_eq!(
            output
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.code)
                .collect::<Vec<_>>(),
            [2322, 2345, 2322, 2345]
        );
        assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
        for (suffix, source, target, property, leaf) in [
            (
                "ab.ts",
                "{ alpha: string; beta: number; }",
                "{ alpha: \"a\"; beta: 1; }",
                "alpha",
                "Type 'string' is not assignable to type '\"a\"'.",
            ),
            (
                "ba.ts",
                "{ beta: number; alpha: string; }",
                "{ beta: 1; alpha: \"a\"; }",
                "beta",
                "Type 'number' is not assignable to type '1'.",
            ),
        ] {
            let diagnostics = output
                .diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.file.ends_with(suffix))
                .collect::<Vec<_>>();
            assert_eq!(diagnostics.len(), 2, "{suffix}: {:?}", output.diagnostics);
            assert_eq!(
                diagnostics[0].message_text,
                format!("Type '{source}' is not assignable to type '{target}'.")
            );
            assert_eq!(
                diagnostics[1].message_text,
                format!(
                    "Argument of type '{source}' is not assignable to parameter of type '{target}'."
                )
            );
            for diagnostic in diagnostics {
                assert_eq!(
                    diagnostic
                        .related_information
                        .iter()
                        .map(|information| information.message_text.as_str())
                        .collect::<Vec<_>>(),
                    [
                        format!("Types of property '{property}' are incompatible."),
                        leaf.to_string()
                    ]
                );
            }
        }
    };
    let ab_first = vec![
        SourceInput::new("a-ab.ts", ab.clone()),
        SourceInput::new("z-ba.ts", ba.clone()),
    ];
    let cold = run(ab_first.clone());
    verify(&cold);
    for inputs in [ab_first.clone(), ab_first.into_iter().rev().collect()] {
        let output = run(inputs);
        verify(&output);
        assert_eq!(
            serde_json::to_vec(&output.diagnostics).unwrap(),
            serde_json::to_vec(&cold.diagnostics).unwrap()
        );
    }
    let ba_first = vec![
        SourceInput::new("a-ba.ts", ba),
        SourceInput::new("z-ab.ts", ab),
    ];
    for inputs in [ba_first.clone(), ba_first.into_iter().rev().collect()] {
        verify(&run(inputs));
    }
}

#[test]
fn multiple_missing_properties_keep_authored_order_across_renamed_reversed_roots() {
    let options = CompilerOptions {
        no_emit: true,
        strict: true,
        ..CompilerOptions::default()
    };
    let zeta_first = Arc::<str>::from(
        "declare const zetaFirst:{present:number};const target:{zeta:string;alpha:string}=zetaFirst;",
    );
    let alpha_first = Arc::<str>::from(
        "declare const alphaFirst:{present:number};const target:{alpha:string;zeta:string}=alphaFirst;",
    );
    let compiler = Compiler::new();
    let inputs = vec![
        SourceInput::new("a-zeta-first.ts", zeta_first),
        SourceInput::new("z-alpha-first.ts", alpha_first),
    ];
    let cold = compiler.compile(inputs.clone(), &options);
    assert_eq!(
        cold.diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>(),
        [2322, 2322]
    );
    assert_eq!(cold.semantic_completion, SemanticCompletion::Complete);
    for (suffix, properties) in [
        ("a-zeta-first.ts", "zeta, alpha"),
        ("z-alpha-first.ts", "alpha, zeta"),
    ] {
        let diagnostic = cold
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.file.ends_with(suffix))
            .expect("one diagnostic per authored shape");
        assert!(
            diagnostic
                .related_information
                .last()
                .is_some_and(|related| related.message_text.contains(properties)),
            "{diagnostic:?}"
        );
    }
    let reversed = compiler.compile(inputs.into_iter().rev().collect(), &options);
    assert_eq!(reversed.semantic_completion, SemanticCompletion::Complete);
    assert_eq!(
        serde_json::to_vec(&reversed.diagnostics).unwrap(),
        serde_json::to_vec(&cold.diagnostics).unwrap()
    );
}

#[test]
fn assignment_any_observes_never_and_symbolic_keyof() {
    let mut context = TestContext {
        kinds: vec![
            TypeKind::Any,
            TypeKind::Never,
            TypeKind::Deferred(DeferredType::KeyOf(TypeId(3))),
            TypeKind::TypeParameter {
                declaration: DeclId {
                    file: FileId(0),
                    local: 1,
                },
                index: 0,
                name: "Key".to_string(),
            },
        ],
        completions: HashMap::new(),
        evaluator_depth: 0,
    };

    let failure = relate(&mut context, TypeId(0), TypeId(1), RelationMode::Assignment).unwrap_err();
    assert_eq!(failure.kind, RelationFailureKind::Incompatible);
    assert!(relate(&mut context, TypeId(0), TypeId(2), RelationMode::Assignment,).is_ok());
    assert!(relate(&mut context, TypeId(0), TypeId(2), RelationMode::Subtype,).is_err());
}

#[test]
fn logical_assignment_proof_propagates_incomplete_rhs() {
    let declaration = |local| DeclId {
        file: FileId(0),
        local,
    };
    let mut context = TestContext {
        kinds: vec![
            TypeKind::Deferred(DeferredType::Value(declaration(0))),
            TypeKind::Deferred(DeferredType::Logical {
                operator: DeferredLogicalOperator::Or,
                left: TypeId(0),
                right: TypeId(2),
            }),
            TypeKind::Deferred(DeferredType::Value(declaration(2))),
        ],
        completions: HashMap::from([(TypeId(2), Completion::Cycle)]),
        evaluator_depth: 0,
    };

    let failure = relate(&mut context, TypeId(1), TypeId(0), RelationMode::Assignment).unwrap_err();
    assert_eq!(failure.kind, RelationFailureKind::Cycle);

    let signature = Signature {
        generic_declaration: None,
        untyped_javascript: false,
        parameters: Vec::new(),
        return_type: TypeId(3),
    };
    let mut contextual = TestContext {
        kinds: vec![
            TypeKind::Function(signature),
            TypeKind::Deferred(DeferredType::Logical {
                operator: DeferredLogicalOperator::Or,
                left: TypeId(0),
                right: TypeId(2),
            }),
            TypeKind::Array(TypeId(3)),
            TypeKind::Void,
        ],
        completions: HashMap::from([(TypeId(1), Completion::Complete(TypeId(0)))]),
        evaluator_depth: 0,
    };
    assert!(
        relate(
            &mut contextual,
            TypeId(1),
            TypeId(0),
            RelationMode::Assignment,
        )
        .is_ok()
    );
}

#[test]
fn callable_variant_pairs_preserve_their_failure_policy() {
    let callable = |shape: bool, name: &str, parameter: TypeId| {
        let signature = Signature {
            generic_declaration: None,
            untyped_javascript: false,
            parameters: vec![ParameterType {
                name: (!shape).then(|| name.to_string()),
                ty: parameter,
                optional: false,
                rest: false,
            }],
            return_type: TypeId(0),
        };
        if shape {
            TypeKind::ShapeFunction(signature)
        } else {
            TypeKind::Function(signature)
        }
    };

    for (source_shape, target_shape) in [(false, false), (true, false), (false, true), (true, true)]
    {
        let kinds = vec![
            TypeKind::String,
            TypeKind::Number,
            callable(source_shape, "source", TypeId(0)),
            callable(target_shape, "target", TypeId(0)),
        ];
        assert_eq!(
            relate(
                &mut TestContext {
                    kinds,
                    completions: HashMap::new(),
                    evaluator_depth: 0,
                },
                TypeId(2),
                TypeId(3),
                RelationMode::Assignment,
            ),
            Ok(())
        );

        let kinds = vec![
            TypeKind::String,
            TypeKind::Number,
            callable(source_shape, "source", TypeId(0)),
            callable(target_shape, "target", TypeId(1)),
        ];
        let failure = relate(
            &mut TestContext {
                kinds,
                completions: HashMap::new(),
                evaluator_depth: 0,
            },
            TypeId(2),
            TypeId(3),
            RelationMode::Assignment,
        )
        .unwrap_err();
        if source_shape || target_shape {
            assert_eq!(failure.kind, RelationFailureKind::Deferred);
            assert!(failure.child.is_none());
        } else {
            assert_eq!(failure.kind, RelationFailureKind::Parameter(0));
            assert_eq!(
                failure.child.as_deref().map(|child| &child.kind),
                Some(&RelationFailureKind::Incompatible)
            );
        }
    }
}

#[test]
fn covariant_library_references_relate_arguments_and_preserve_incompletion() {
    let other_library = DeclId {
        file: FileId(u32::MAX),
        local: 1,
    };
    let mut context = TestContext {
        kinds: vec![
            TypeKind::Any,
            TypeKind::String,
            TypeKind::Number,
            TypeKind::LibraryReference {
                declaration: COVARIANT_LIBRARY_REFERENCE,
                name: "Canonical".to_string(),
                arguments: vec![TypeId(0), TypeId(0)],
            },
            TypeKind::LibraryReference {
                declaration: COVARIANT_LIBRARY_REFERENCE,
                name: "Canonical".to_string(),
                arguments: vec![TypeId(1), TypeId(2)],
            },
            TypeKind::LibraryReference {
                declaration: COVARIANT_LIBRARY_REFERENCE,
                name: "Canonical".to_string(),
                arguments: vec![TypeId(1), TypeId(1)],
            },
            TypeKind::Deferred(DeferredType::Value(DeclId {
                file: FileId(0),
                local: 0,
            })),
            TypeKind::LibraryReference {
                declaration: COVARIANT_LIBRARY_REFERENCE,
                name: "Canonical".to_string(),
                arguments: vec![TypeId(1), TypeId(6)],
            },
            TypeKind::LibraryReference {
                declaration: other_library,
                name: "Other".to_string(),
                arguments: vec![TypeId(0), TypeId(0)],
            },
        ],
        completions: HashMap::from([(TypeId(6), Completion::Deferred)]),
        evaluator_depth: 0,
    };

    assert_eq!(
        relate(&mut context, TypeId(3), TypeId(4), RelationMode::Assignment),
        Ok(())
    );

    let incompatible =
        relate(&mut context, TypeId(5), TypeId(4), RelationMode::Assignment).unwrap_err();
    assert_eq!(incompatible.kind, RelationFailureKind::TypeArgument(1));
    let leaf = incompatible.child.as_deref().unwrap();
    assert_eq!(
        (leaf.source, leaf.target, &leaf.kind),
        (TypeId(1), TypeId(2), &RelationFailureKind::Incompatible)
    );

    let incomplete =
        relate(&mut context, TypeId(7), TypeId(4), RelationMode::Assignment).unwrap_err();
    assert_eq!(incomplete.kind, RelationFailureKind::Deferred);
    assert!(incomplete.child.is_none());

    let other = relate(&mut context, TypeId(8), TypeId(4), RelationMode::Assignment).unwrap_err();
    assert_eq!(other.kind, RelationFailureKind::Deferred);
}

#[test]
fn recursive_deferred_shapes_stop_at_the_active_pair() {
    let declaration_a = DeclId {
        file: FileId(0),
        local: 0,
    };
    let declaration_b = DeclId {
        file: FileId(0),
        local: 1,
    };
    let mut context = TestContext {
        kinds: vec![
            TypeKind::Deferred(DeferredType::Reference {
                declaration: declaration_a,
                arguments: Vec::new(),
            }),
            TypeKind::Deferred(DeferredType::Reference {
                declaration: declaration_b,
                arguments: Vec::new(),
            }),
            TypeKind::Object(vec![property("next", 0)].into()),
            TypeKind::Object(vec![property("next", 1)].into()),
        ],
        completions: HashMap::from([
            (TypeId(0), Completion::Complete(TypeId(2))),
            (TypeId(1), Completion::Complete(TypeId(3))),
        ]),
        evaluator_depth: 0,
    };

    assert_eq!(
        relate(&mut context, TypeId(0), TypeId(1), RelationMode::Assignment),
        Ok(())
    );
}

#[test]
fn nested_array_failures_keep_each_structural_pair() {
    let mut context = TestContext {
        kinds: vec![
            TypeKind::Array(TypeId(2)),
            TypeKind::Array(TypeId(3)),
            TypeKind::Array(TypeId(4)),
            TypeKind::Array(TypeId(5)),
            TypeKind::LiteralString("other".to_string(), LiteralProvenance::Fresh),
            TypeKind::LiteralString("seed".to_string(), LiteralProvenance::Regular),
        ],
        completions: HashMap::new(),
        evaluator_depth: 0,
    };

    let failure = relate(&mut context, TypeId(0), TypeId(1), RelationMode::Assignment).unwrap_err();
    assert_eq!(
        (failure.source, failure.target, &failure.kind),
        (TypeId(0), TypeId(1), &RelationFailureKind::ArrayElement)
    );
    let child = failure.child.as_deref().unwrap();
    assert_eq!(
        (child.source, child.target, &child.kind),
        (TypeId(2), TypeId(3), &RelationFailureKind::ArrayElement)
    );
    let leaf = child.child.as_deref().unwrap();
    assert_eq!(
        (leaf.source, leaf.target, &leaf.kind),
        (TypeId(4), TypeId(5), &RelationFailureKind::Incompatible)
    );
    assert!(leaf.child.is_none());
}

#[test]
fn nested_relation_structure_preserves_the_active_evaluator_seed() {
    let mut context = TestContext {
        kinds: vec![
            TypeKind::Array(TypeId(2)),
            TypeKind::Array(TypeId(3)),
            TypeKind::Array(TypeId(4)),
            TypeKind::Array(TypeId(5)),
            TypeKind::Deferred(DeferredType::Value(DeclId {
                file: FileId(0),
                local: 4,
            })),
            TypeKind::Deferred(DeferredType::Value(DeclId {
                file: FileId(0),
                local: 5,
            })),
            TypeKind::Number,
        ],
        completions: HashMap::from([
            (TypeId(4), Completion::Complete(TypeId(6))),
            (TypeId(5), Completion::Complete(TypeId(6))),
        ]),
        evaluator_depth: 7,
    };

    assert_eq!(
        relate_types_at_evaluation_depth(
            &mut context,
            TypeId(0),
            TypeId(1),
            RelationMode::Assignment,
            EvaluationDepth::from_active_depth(7),
        ),
        Ok(())
    );
}

#[test]
fn target_union_failures_keep_the_outer_pair_and_first_member_cause() {
    let mut context = TestContext {
        kinds: vec![
            TypeKind::String,
            TypeKind::Array(TypeId(0)),
            TypeKind::LiteralString("a".to_string(), LiteralProvenance::Regular),
            TypeKind::Array(TypeId(2)),
            TypeKind::LiteralString("b".to_string(), LiteralProvenance::Regular),
            TypeKind::Array(TypeId(4)),
            TypeKind::Union(vec![TypeId(3), TypeId(5)]),
        ],
        completions: HashMap::new(),
        evaluator_depth: 0,
    };

    let failure = relate(&mut context, TypeId(1), TypeId(6), RelationMode::Assignment).unwrap_err();
    assert_eq!(
        (failure.source, failure.target, &failure.kind),
        (TypeId(1), TypeId(6), &RelationFailureKind::UnionMember)
    );
    let member = failure.child.as_deref().unwrap();
    assert_eq!(
        (member.source, member.target, &member.kind),
        (TypeId(1), TypeId(3), &RelationFailureKind::ArrayElement)
    );
    let leaf = member.child.as_deref().unwrap();
    assert_eq!(
        (leaf.source, leaf.target, &leaf.kind),
        (TypeId(0), TypeId(2), &RelationFailureKind::Incompatible)
    );
}

#[test]
fn object_property_failures_keep_object_property_and_leaf_pairs() {
    let mut context = TestContext {
        kinds: vec![
            TypeKind::Object(vec![property("kind", 2)].into()),
            TypeKind::Object(vec![property("kind", 3)].into()),
            TypeKind::LiteralString("b".to_string(), LiteralProvenance::Regular),
            TypeKind::LiteralString("a".to_string(), LiteralProvenance::Regular),
        ],
        completions: HashMap::new(),
        evaluator_depth: 0,
    };

    let failure = relate(&mut context, TypeId(0), TypeId(1), RelationMode::Assignment).unwrap_err();
    assert_eq!(
        (failure.source, failure.target, &failure.kind),
        (TypeId(0), TypeId(1), &RelationFailureKind::Object)
    );
    let property = failure.child.as_deref().unwrap();
    assert_eq!(
        (property.source, property.target, &property.kind),
        (
            TypeId(2),
            TypeId(3),
            &RelationFailureKind::Property("kind".to_string())
        )
    );
    let leaf = property.child.as_deref().unwrap();
    assert_eq!(
        (leaf.source, leaf.target, &leaf.kind),
        (TypeId(2), TypeId(3), &RelationFailureKind::Incompatible)
    );
}

#[test]
fn tuple_array_failures_keep_combined_element_and_length_causes() {
    let mut context = TestContext {
        kinds: vec![
            TypeKind::Tuple(vec![TypeId(2), TypeId(3)]),
            TypeKind::Array(TypeId(4)),
            TypeKind::LiteralString("first".to_string(), LiteralProvenance::Regular),
            TypeKind::LiteralString("second".to_string(), LiteralProvenance::Regular),
            TypeKind::LiteralString("seed".to_string(), LiteralProvenance::Regular),
        ],
        completions: HashMap::new(),
        evaluator_depth: 0,
    };
    let element_failure =
        relate(&mut context, TypeId(0), TypeId(1), RelationMode::Assignment).unwrap_err();
    assert_eq!(element_failure.kind, RelationFailureKind::ArrayElement);
    let combined = element_failure.child.as_deref().unwrap();
    assert!(matches!(
        context.kinds[combined.source.0 as usize],
        TypeKind::Union(_)
    ));

    let mut reverse_context = TestContext {
        kinds: vec![
            TypeKind::Array(TypeId(2)),
            TypeKind::Tuple(vec![TypeId(3)]),
            TypeKind::String,
            TypeKind::LiteralString("seed".to_string(), LiteralProvenance::Regular),
        ],
        completions: HashMap::new(),
        evaluator_depth: 0,
    };
    let length_failure = relate(
        &mut reverse_context,
        TypeId(0),
        TypeId(1),
        RelationMode::Assignment,
    )
    .unwrap_err();
    assert_eq!(
        length_failure.kind,
        RelationFailureKind::ArrayToTupleLength { required: 1 }
    );
}

#[test]
fn incomplete_nested_relations_are_not_rewritten_as_property_failures() {
    for (completion, expected) in [
        (Completion::Deferred, RelationFailureKind::Deferred),
        (Completion::Cycle, RelationFailureKind::Cycle),
        (Completion::Limit, RelationFailureKind::ComplexityLimit),
    ] {
        let mut context = TestContext {
            kinds: vec![
                TypeKind::Object(vec![property("item", 2)].into()),
                TypeKind::Object(vec![property("item", 3)].into()),
                TypeKind::Number,
                TypeKind::Deferred(DeferredType::Value(DeclId {
                    file: FileId(0),
                    local: 3,
                })),
            ],
            completions: HashMap::from([(TypeId(3), completion)]),
            evaluator_depth: 0,
        };

        let failure =
            relate(&mut context, TypeId(0), TypeId(1), RelationMode::Assignment).unwrap_err();
        assert_eq!(failure.kind, expected);
    }
}

#[test]
fn alternative_failures_use_semantic_completion_dominance() {
    let declaration = |local| DeclId {
        file: FileId(0),
        local,
    };
    let mut context = TestContext {
        kinds: vec![
            TypeKind::Number,
            TypeKind::Union(vec![TypeId(2), TypeId(3), TypeId(4), TypeId(5)]),
            TypeKind::Invalid(InvalidType::MissingProperty {
                object: TypeId(0),
                name: "missing".to_string(),
            }),
            TypeKind::Deferred(DeferredType::Value(declaration(3))),
            TypeKind::Deferred(DeferredType::Value(declaration(4))),
            TypeKind::Deferred(DeferredType::Value(declaration(5))),
        ],
        completions: HashMap::from([
            (TypeId(3), Completion::Deferred),
            (TypeId(4), Completion::Cycle),
            (TypeId(5), Completion::Limit),
        ]),
        evaluator_depth: 0,
    };

    let failure = relate(&mut context, TypeId(0), TypeId(1), RelationMode::Assignment).unwrap_err();
    assert_eq!(failure.kind, RelationFailureKind::ComplexityLimit);
}

#[test]
fn invalid_projection_is_a_relation_failure_not_a_success_type() {
    let mut context = TestContext {
        kinds: vec![
            TypeKind::Invalid(InvalidType::MissingProperty {
                object: TypeId(1),
                name: "missing".to_string(),
            }),
            TypeKind::Number,
        ],
        completions: HashMap::new(),
        evaluator_depth: 0,
    };

    let failure = relate(&mut context, TypeId(0), TypeId(1), RelationMode::Assignment).unwrap_err();
    assert_eq!(failure.kind, RelationFailureKind::InvalidProjection);
}

#[test]
fn diagnostic_display_truncation_does_not_become_semantic_exhaustion() {
    fn nested_object(mut leaf: String, depth: usize) -> String {
        for index in (0..depth).rev() {
            leaf = format!("{{ level{index}: {leaf} }}");
        }
        leaf
    }

    let compiler = Compiler::new();
    let options = CompilerOptions {
        no_emit: true,
        strict: true,
        ..CompilerOptions::default()
    };
    for depth in [24, 25, 26] {
        let source = nested_object("string".to_owned(), depth);
        let target = nested_object("number".to_owned(), depth);
        let text = format!("declare const cedar: {source}; const willow: {target} = cedar;");
        let output = compiler.compile(
            vec![SourceInput::new(
                format!("display-depth-{depth}.ts"),
                Arc::<str>::from(text),
            )],
            &options,
        );
        assert_eq!(
            output
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.code)
                .collect::<Vec<_>>(),
            [if depth <= 24 { 2322 } else { 2719 }],
            "depth {depth}: {:?}",
            output.diagnostics
        );
        if depth > 24 {
            assert!(
                output.diagnostics[0]
                    .message_text
                    .ends_with("Two different types with this name exist, but they are unrelated.")
            );
        }
        assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
        assert_eq!(
            output.exit_status,
            CompileExitStatus::DiagnosticsPresentOutputsSkipped
        );
        assert!(
            !output
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == 2589)
        );
    }

    let equal = nested_object("string".to_owned(), 26);
    let output = compiler.compile(
        vec![SourceInput::new(
            "display-depth-equal.ts",
            Arc::<str>::from(format!(
                "declare const cedar: {equal}; const willow: {equal} = cedar;"
            )),
        )],
        &options,
    );
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
}
