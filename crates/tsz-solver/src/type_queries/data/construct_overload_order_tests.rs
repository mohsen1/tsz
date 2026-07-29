use super::{
    deduplicate_construct_signatures_keep_last, mixin_instance_returns_with_base_last,
    reorder_construct_overload_candidates,
};
use crate::construction::TypeInterner;
use crate::def::DefId;
use crate::types::{
    CallSignature, CallableShape, ConstructSignatureOrigin, ParamInfo, TypeId, TypeParamInfo,
    TypeParamOrigin,
};
use tsz_common::Atom;

fn signature(
    return_type: TypeId,
    has_literal_types: bool,
    owner: u32,
    declaration_group: u32,
) -> CallSignature {
    let mut signature = CallSignature::new(Vec::new(), return_type);
    signature.has_literal_types = has_literal_types;
    signature.construct_origin = Some(ConstructSignatureOrigin {
        owner: Some(DefId(owner)),
        declaration_file: Atom(1),
        declaration_pos: declaration_group,
        declaration_end: declaration_group + 1,
    });
    signature
}

#[test]
fn construct_candidates_keep_specialized_source_order_and_reverse_regular_groups() {
    let stored = vec![
        signature(TypeId(101), true, 1, 0),
        signature(TypeId(102), false, 1, 0),
        signature(TypeId(103), true, 1, 1),
        signature(TypeId(104), false, 1, 1),
    ];

    let ordered = reorder_construct_overload_candidates(&stored);
    let returns: Vec<_> = ordered
        .iter()
        .map(|signature| signature.return_type)
        .collect();
    assert_eq!(
        returns,
        vec![TypeId(101), TypeId(103), TypeId(104), TypeId(102)]
    );
}

#[test]
fn construct_candidates_do_not_reverse_groups_across_distinct_owners() {
    let stored = vec![
        signature(TypeId(201), false, 1, 0),
        signature(TypeId(202), false, 2, 1),
    ];

    let ordered = reorder_construct_overload_candidates(&stored);
    let returns: Vec<_> = ordered
        .iter()
        .map(|signature| signature.return_type)
        .collect();
    assert_eq!(returns, vec![TypeId(201), TypeId(202)]);
}

#[test]
fn construct_candidates_keep_untracked_signature_as_group_boundary() {
    let untracked = CallSignature::new(Vec::new(), TypeId(302));
    let stored = vec![
        signature(TypeId(301), false, 1, 0),
        untracked,
        signature(TypeId(303), false, 1, 1),
    ];

    let ordered = reorder_construct_overload_candidates(&stored);
    let returns: Vec<_> = ordered
        .iter()
        .map(|signature| signature.return_type)
        .collect();
    assert_eq!(returns, vec![TypeId(301), TypeId(302), TypeId(303)]);
}

#[test]
fn construct_candidates_move_specialized_signature_across_distinct_owners() {
    let stored = vec![
        signature(TypeId(401), false, 1, 0),
        signature(TypeId(402), true, 2, 1),
    ];

    let ordered = reorder_construct_overload_candidates(&stored);
    let returns: Vec<_> = ordered
        .iter()
        .map(|signature| signature.return_type)
        .collect();
    assert_eq!(returns, vec![TypeId(402), TypeId(401)]);
}

#[test]
fn construct_candidates_move_specialized_signature_across_untracked_boundary() {
    let untracked = CallSignature::new(Vec::new(), TypeId(502));
    let stored = vec![
        signature(TypeId(501), false, 1, 0),
        untracked,
        signature(TypeId(503), true, 1, 1),
    ];

    let ordered = reorder_construct_overload_candidates(&stored);
    let returns: Vec<_> = ordered
        .iter()
        .map(|signature| signature.return_type)
        .collect();
    assert_eq!(returns, vec![TypeId(503), TypeId(501), TypeId(502)]);
}

#[test]
fn construct_candidate_metadata_is_callable_identity() {
    let interner = TypeInterner::new();
    let callable = |signature| {
        interner.callable(CallableShape {
            call_signatures: Vec::new(),
            construct_signatures: vec![signature],
            properties: Vec::new(),
            string_index: None,
            number_index: None,
            symbol: None,
            is_abstract: false,
        })
    };

    let regular = signature(TypeId(601), false, 1, 0);
    let mut literal = regular.clone();
    literal.has_literal_types = true;
    assert_ne!(
        callable(regular.clone()),
        callable(literal),
        "literal syntax changes observable candidate priority and must survive interning"
    );

    let distinct_owner = signature(TypeId(601), false, 2, 0);
    assert_ne!(
        callable(regular),
        callable(distinct_owner),
        "source ownership changes declaration-group reversal and must survive interning"
    );
}

#[test]
fn construct_merge_dedup_preserves_optional_and_required_signatures() {
    let interner = TypeInterner::new();
    let mut optional = CallSignature::new(
        vec![ParamInfo {
            name: None,
            type_id: TypeId::STRING,
            optional: true,
            rest: false,
        }],
        TypeId::BOOLEAN,
    );
    optional.construct_origin = signature(TypeId::BOOLEAN, false, 1, 0).construct_origin;
    let mut required = optional.clone();
    required.params[0].optional = false;
    required.construct_origin = signature(TypeId::BOOLEAN, false, 1, 1).construct_origin;

    let mut signatures = vec![optional, required];
    deduplicate_construct_signatures_keep_last(&interner, &mut signatures, true);

    assert_eq!(
        signatures.len(),
        2,
        "effective minimum arity is part of construct signature identity"
    );
}

#[test]
fn construct_merge_dedup_alpha_normalizes_type_parameters_and_keeps_last() {
    let interner = TypeInterner::new();
    let t = interner.intern_string("T");
    let u = interner.intern_string("U");
    let type_parameter = |name| TypeParamInfo {
        name,
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
        origin: TypeParamOrigin::User,
    };
    let t_info = type_parameter(t);
    let u_info = type_parameter(u);
    let t_type = interner.type_param(t_info);
    let u_type = interner.type_param(u_info);
    let generic = |name, type_id, owner| {
        let mut candidate = CallSignature::new(
            vec![ParamInfo {
                name: None,
                type_id,
                optional: false,
                rest: false,
            }],
            type_id,
        );
        candidate.type_params = vec![type_parameter(name)];
        candidate.construct_origin = signature(type_id, false, owner, 0).construct_origin;
        candidate
    };
    let first = generic(t, t_type, 1);
    let second = generic(u, u_type, 2);
    let expected_origin = second.construct_origin;
    let mut signatures = vec![first, second];

    deduplicate_construct_signatures_keep_last(&interner, &mut signatures, true);

    assert_eq!(signatures.len(), 1);
    assert_eq!(
        signatures[0].construct_origin, expected_origin,
        "alpha-equivalent signatures keep their last diamond occurrence"
    );
}

#[test]
fn projected_mixin_return_keeps_instantiated_member_before_base() {
    let interner = TypeInterner::new();
    let base = interner.lazy(DefId(701));
    let instantiated_return = interner.lazy(DefId(702));
    let projected = interner.intersection2(base, instantiated_return);

    let reordered = mixin_instance_returns_with_base_last(&interner, vec![projected], base);
    let members = crate::type_queries::get_intersection_members(&interner, reordered)
        .expect("mixin result should stay an intersection");

    assert_eq!(
        members.as_ref(),
        &[instantiated_return, base],
        "source returned-class order must win without reconstructing and losing explicit generic substitution"
    );
}

#[test]
fn projected_mixin_return_moves_nested_base_members_to_tail() {
    let interner = TypeInterner::new();
    let base_left = interner.lazy(DefId(711));
    let base_right = interner.lazy(DefId(712));
    let returned = interner.lazy(DefId(713));
    let base = interner.intersection2(base_left, base_right);
    let projected = interner.intersection(vec![base_left, base_right, returned]);

    let reordered = mixin_instance_returns_with_base_last(&interner, vec![projected], base);
    let members = crate::type_queries::get_intersection_members(&interner, reordered)
        .expect("nested mixin result should stay an intersection");

    assert_eq!(
        members.as_ref(),
        &[returned, base_left, base_right],
        "every exact constituent of an intersected base moves behind the returned class"
    );
}
