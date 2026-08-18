//! Overload candidate reordering (tsc `reorderCandidates` port) — see
//! `type_queries/data/accessors.rs`.

use crate::construction::TypeInterner;
use crate::type_queries::data::reorder_overload_candidates;
use crate::types::TypeId;

/// Port fidelity for tsc's `reorderCandidates` group handling: signatures
/// from a LATER `declaration_group` are tried first (each group keeps its
/// internal order), and specialized (literal-param) signatures are hoisted
/// to the front regardless of group. Traces verified against tsc's splice
/// loop and the live oracle (#17646).
#[test]
fn test_reorder_overload_candidates_declaration_groups() {
    use crate::types::{CallSignature, ParamInfo};
    let interner = TypeInterner::new();

    let plain = |group: u32, ret: TypeId| CallSignature {
        type_params: vec![],
        params: vec![ParamInfo {
            name: None,
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: ret,
        type_predicate: None,
        is_method: true,
        declaration_group: group,
    };
    let specialized = |group: u32, lit: &str, ret: TypeId| CallSignature {
        params: vec![ParamInfo {
            name: None,
            type_id: interner.literal_string(lit),
            optional: false,
            rest: false,
        }],
        ..plain(group, ret)
    };

    // Two groups of two: later group first, internal order preserved.
    let sigs = vec![
        plain(0, TypeId::STRING),
        plain(0, TypeId::NUMBER),
        plain(1, TypeId::BOOLEAN),
        plain(1, TypeId::VOID),
    ];
    let ordered: Vec<TypeId> = reorder_overload_candidates(&interner, &sigs)
        .iter()
        .map(|s| s.return_type)
        .collect();
    assert_eq!(
        ordered,
        vec![
            TypeId::BOOLEAN,
            TypeId::VOID,
            TypeId::STRING,
            TypeId::NUMBER
        ]
    );

    // Single group: order untouched (no reordering for plain overloads).
    let sigs = vec![plain(0, TypeId::STRING), plain(0, TypeId::NUMBER)];
    let ordered: Vec<TypeId> = reorder_overload_candidates(&interner, &sigs)
        .iter()
        .map(|s| s.return_type)
        .collect();
    assert_eq!(ordered, vec![TypeId::STRING, TypeId::NUMBER]);

    // Mixed specialized/non-specialized across groups (the oracle-verified
    // `order2` matrix): [g0 plain, g0 lit, g1 plain, g1 lit] resolves as
    // [g0 lit, g1 lit, g1 plain, g0 plain].
    let sigs = vec![
        plain(0, TypeId::STRING),
        specialized(0, "lit", TypeId::NUMBER),
        plain(1, TypeId::BOOLEAN),
        specialized(1, "lit2", TypeId::VOID),
    ];
    let ordered: Vec<TypeId> = reorder_overload_candidates(&interner, &sigs)
        .iter()
        .map(|s| s.return_type)
        .collect();
    assert_eq!(
        ordered,
        vec![
            TypeId::NUMBER,
            TypeId::VOID,
            TypeId::BOOLEAN,
            TypeId::STRING
        ]
    );
}
