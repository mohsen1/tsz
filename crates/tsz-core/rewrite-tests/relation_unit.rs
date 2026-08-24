use std::collections::HashMap;

use crate::source::{DeclId, FileId};

use super::*;
use crate::semantics::types::{DeferredType, InvalidType, LiteralProvenance};

struct TestContext {
    kinds: Vec<TypeKind>,
    completions: HashMap<TypeId, Completion<TypeId>>,
}

impl RelationContext for TestContext {
    fn force_type(&mut self, ty: TypeId, _depth: usize) -> Completion<TypeId> {
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
    };

    let failure = relate(&mut context, TypeId(0), TypeId(1), RelationMode::Assignment).unwrap_err();
    assert_eq!(failure.kind, RelationFailureKind::InvalidProjection);
}
