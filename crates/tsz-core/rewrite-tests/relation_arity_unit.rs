use std::{collections::HashMap, sync::Arc};

use crate::diagnostics::DiagnosticCategory;
use crate::source::{DeclId, FileId};
use crate::{CompileExitStatus, Compiler, CompilerOptions, SemanticCompletion, SourceInput};

use super::*;
use crate::semantics::types::{ObjectShape, ParameterType};

struct ArityContext {
    kinds: Vec<TypeKind>,
    constructor_signatures: HashMap<DeclId, Signature>,
}

impl RelationContext for ArityContext {
    fn force_type(&mut self, ty: TypeId, _depth: usize) -> Completion<TypeId> {
        Completion::Complete(ty)
    }

    fn type_kind(&self, ty: TypeId) -> TypeKind {
        self.kinds[ty.0 as usize].clone()
    }

    fn generative_reference_supported(&self, _declaration: DeclId, _arguments: &[TypeId]) -> bool {
        false
    }

    fn generative_relation_frame_supported(
        &self,
        _declaration: DeclId,
        _arguments: &[TypeId],
    ) -> bool {
        false
    }

    fn library_reference_arguments_are_covariant(&self, _declaration: DeclId) -> bool {
        false
    }

    fn class_constructor_signature(&mut self, declaration: DeclId) -> Completion<Signature> {
        self.constructor_signatures
            .get(&declaration)
            .cloned()
            .map_or(Completion::Deferred, Completion::Complete)
    }

    fn strict_null_checks(&self) -> bool {
        true
    }

    fn canonical_union(&mut self, members: &[TypeId]) -> TypeId {
        assert_eq!(members.len(), 1);
        members[0]
    }
}

fn declaration(local: u32) -> DeclId {
    DeclId {
        file: FileId(0),
        local,
    }
}

fn signature(required: usize, parameter_count: usize) -> Signature {
    Signature {
        generic_declaration: None,
        untyped_javascript: false,
        parameters: (0..parameter_count)
            .map(|index| ParameterType {
                name: None,
                ty: TypeId(0),
                optional: index >= required,
                rest: false,
            })
            .collect(),
        return_type: TypeId(0),
    }
}

#[test]
fn constructor_and_member_arity_failures_are_structured_relation_reasons() {
    let source_declaration = declaration(1);
    let target_declaration = declaration(2);
    let mut constructors = ArityContext {
        kinds: vec![
            TypeKind::ClassConstructor {
                declaration: source_declaration,
                name: "Derived".to_string(),
            },
            TypeKind::ClassConstructor {
                declaration: target_declaration,
                name: "Base".to_string(),
            },
        ],
        constructor_signatures: HashMap::from([
            (source_declaration, signature(1, 1)),
            (target_declaration, signature(0, 0)),
        ]),
    };
    let failure = relate_types(
        &mut constructors,
        TypeId(0),
        TypeId(1),
        RelationMode::Assignment,
    )
    .unwrap_err();
    assert_eq!(failure.kind, RelationFailureKind::Incompatible);
    assert_eq!(
        failure.child.expect("arity reason").kind,
        RelationFailureKind::SignatureArityMismatch {
            source_minimum: 1,
            target_parameter_count: 0,
        }
    );

    let source_signature = Signature {
        generic_declaration: None,
        untyped_javascript: false,
        parameters: vec![ParameterType {
            name: Some("seed".to_string()),
            ty: TypeId(2),
            optional: false,
            rest: false,
        }],
        return_type: TypeId(3),
    };
    let target_signature = Signature {
        generic_declaration: None,
        untyped_javascript: false,
        parameters: Vec::new(),
        return_type: TypeId(3),
    };
    let mut members = ArityContext {
        kinds: vec![
            TypeKind::Function(source_signature),
            TypeKind::Function(target_signature),
            TypeKind::Number,
            TypeKind::Void,
            TypeKind::Object(ObjectShape {
                properties: vec![Property {
                    name: "build".to_string(),
                    ty: TypeId(0),
                    optional: false,
                    readonly: false,
                }],
                ..ObjectShape::default()
            }),
            TypeKind::Object(ObjectShape {
                properties: vec![Property {
                    name: "build".to_string(),
                    ty: TypeId(1),
                    optional: false,
                    readonly: false,
                }],
                ..ObjectShape::default()
            }),
        ],
        constructor_signatures: HashMap::new(),
    };
    let failure =
        relate_types(&mut members, TypeId(4), TypeId(5), RelationMode::Assignment).unwrap_err();
    assert_eq!(failure.kind, RelationFailureKind::Object);
    let property = failure.child.expect("property reason");
    assert_eq!(
        property.kind,
        RelationFailureKind::Property("build".to_string())
    );
    let signature = property.child.expect("signature reason");
    assert_eq!(signature.kind, RelationFailureKind::Incompatible);
    assert_eq!(
        signature.child.expect("arity reason").kind,
        RelationFailureKind::SignatureArityMismatch {
            source_minimum: 1,
            target_parameter_count: 0,
        }
    );
}

fn compile(source: &str) -> crate::CompileOutput {
    Compiler::new().compile(
        vec![SourceInput::new(
            "relation-constructor-arity.ts",
            Arc::<str>::from(source),
        )],
        &CompilerOptions {
            no_emit: true,
            strict: true,
            target: "es2015".to_string(),
            ..CompilerOptions::default()
        },
    )
}

#[test]
fn assignment_compatibility_45_reports_the_pinned_ts7_arity_identity() {
    let source = concat!(
        "abstract class A {}\n",
        "class B extends A { constructor(x: number) { super(); } }\n",
        "const b: typeof A = B;\n",
    );
    let output = compile(source);
    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
    assert_eq!(
        output.exit_status,
        CompileExitStatus::DiagnosticsPresentOutputsSkipped
    );
    let [diagnostic] = output.diagnostics.as_slice() else {
        panic!("expected one diagnostic: {:?}", output.diagnostics);
    };
    assert_eq!(diagnostic.code, 2322);
    assert_eq!(diagnostic.category, DiagnosticCategory::Error);
    assert_eq!(diagnostic.start, source.find("b:").unwrap() as u32);
    assert_eq!(diagnostic.length, 1);
    assert_eq!(
        diagnostic.message_text,
        "Type 'typeof B' is not assignable to type 'typeof A'."
    );
    assert_eq!(diagnostic.related_information.len(), 1);
    let leaf = &diagnostic.related_information[0];
    assert_eq!(leaf.code, 2322);
    assert_eq!(leaf.depth, 1);
    assert_eq!(
        leaf.message_text,
        "Target signature provides too few arguments. Expected 1 or more, but got 0."
    );
}

#[test]
fn renamed_wrapped_and_generic_constructor_arity_matches_the_oracle_matrix() {
    let source = include_str!("fixtures/relation_constructor_arity.ts");
    let output = compile(source);
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>(),
        [2322, 2322, 2322],
        "{:?}",
        output.diagnostics
    );
    for (diagnostic, (binding, actual, expected)) in output.diagnostics.iter().zip([
        ("concrete", "Derived", "Base"),
        ("wrapped", "Derived", "Base"),
        ("generic", "GenericDerived", "GenericBase"),
    ]) {
        assert_eq!(diagnostic.start, source.find(binding).unwrap() as u32);
        assert_eq!(diagnostic.length, binding.len() as u32);
        assert_eq!(
            diagnostic.message_text,
            format!("Type 'typeof {actual}' is not assignable to type 'typeof {expected}'.")
        );
        assert_eq!(
            diagnostic
                .related_information
                .iter()
                .map(|related| (related.depth, related.message_text.as_str()))
                .collect::<Vec<_>>(),
            [(
                1,
                "Target signature provides too few arguments. Expected 1 or more, but got 0."
            )]
        );
    }
    // Arity compatibility is not full static-side compatibility. Until that
    // surface owns static members, abstractness, and constructed returns, the
    // zero/optional rows remain an explicit nonclaim rather than a false TS2322.
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
    assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
}
