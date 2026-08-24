use crate::semantics::types::TypeKind;
use crate::source::{DeclId, FileId};

use super::*;

fn parameter(declaration: DeclId, index: u32) -> TypeKind {
    TypeKind::TypeParameter {
        declaration,
        index,
        name: "ignored".to_string(),
    }
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
