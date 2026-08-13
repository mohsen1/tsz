//! Regression tests for #17332 — `evaluate_intersection` must not splice a
//! class/interface reference's expansion into the surrounding intersection.
//!
//! A multi-parent heritage interface (`interface Window extends EventTarget,
//! GlobalEventHandlers, ...`) resolves through `resolve_lazy` to an
//! intersection of its parents plus its own declared shape; a multi-declaration
//! merge resolves to an intersection of per-declaration shapes. When such a
//! reference sits inside a *written* intersection (`Window & typeof
//! globalThis`), evaluating the member and re-interning used to splice the
//! expansion's constituents directly into the outer member list. That erased
//! the member's `DefId` identity — which the #16089 nominal fast path and the
//! `TypeId`-keyed coinductive cycle guard both key on — so a relation as
//! trivial as `Window & typeof globalThis <: Window` re-walked the full DOM
//! structural graph and died on the relation budget (false TS2859 on
//! `var z1: Window = window;`, false TS2322 on
//! `typeArgumentInferenceConstructSignatures.ts`).
//!
//! The contract pinned here: a bare `Lazy` reference to a class or interface
//! whose evaluation expands to an intersection keeps its reference identity in
//! the evaluated member list. Type-alias references and interface references
//! with plain object bodies keep their existing expansion behavior.

use super::*;
use crate::construction::TypeInterner;
use crate::def::{DefId, DefKind};
use crate::relations::subtype::TypeResolver;

/// `iface` models a multi-parent lib interface (`Window`): its body resolves
/// to `parents & own`. `alias` models `type A = {...} & {...}`. `plain_iface`
/// models a single-declaration, no-heritage interface with an object body.
struct HeritageExpandingResolver {
    iface: DefId,
    alias: DefId,
    plain_iface: DefId,
    iface_expansion: TypeId,
    alias_body: TypeId,
    plain_body: TypeId,
}

impl TypeResolver for HeritageExpandingResolver {
    fn resolve_ref(&self, _symbol: SymbolRef, _interner: &dyn TypeDatabase) -> Option<TypeId> {
        None
    }

    fn resolve_lazy(&self, def_id: DefId, _interner: &dyn TypeDatabase) -> Option<TypeId> {
        if def_id == self.iface {
            Some(self.iface_expansion)
        } else if def_id == self.alias {
            Some(self.alias_body)
        } else if def_id == self.plain_iface {
            Some(self.plain_body)
        } else {
            None
        }
    }

    fn get_def_kind(&self, def_id: DefId) -> Option<DefKind> {
        if def_id == self.iface || def_id == self.plain_iface {
            Some(DefKind::Interface)
        } else if def_id == self.alias {
            Some(DefKind::TypeAlias)
        } else {
            None
        }
    }
}

fn named_object(interner: &TypeInterner, prop: &str, ty: TypeId) -> TypeId {
    interner.object(vec![PropertyInfo::new(interner.intern_string(prop), ty)])
}

fn build_fixture(interner: &TypeInterner) -> HeritageExpandingResolver {
    // Parents modeled as opaque `Lazy` references (as in a real heritage
    // expansion) so the expansion survives the interner's eager object-shape
    // merging and stays a genuine `Intersection`.
    let parent_a = interner.lazy(DefId(101));
    let own = named_object(interner, "document", TypeId::STRING);
    HeritageExpandingResolver {
        iface: DefId(11),
        alias: DefId(22),
        plain_iface: DefId(33),
        iface_expansion: interner.intersection2(parent_a, own),
        alias_body: interner.intersection2(
            interner.lazy(DefId(201)),
            named_object(interner, "b", TypeId::NUMBER),
        ),
        plain_body: named_object(interner, "solo", TypeId::BOOLEAN),
    }
}

fn intersection_members(interner: &TypeInterner, type_id: TypeId) -> Vec<TypeId> {
    match interner.lookup(type_id) {
        Some(TypeData::Intersection(list_id)) => interner.type_list(list_id).to_vec(),
        other => panic!("expected an intersection, got {other:?}"),
    }
}

/// `Window & { extra }`: the `Lazy(Window)` member must survive evaluation as
/// itself, not as its spliced heritage constituents.
#[test]
fn interface_reference_keeps_identity_when_body_expands_to_intersection() {
    let interner = TypeInterner::new();
    let resolver = build_fixture(&interner);

    let window_ref = interner.lazy(resolver.iface);
    let extra = named_object(&interner, "extra", TypeId::NUMBER);
    let source = interner.intersection2(window_ref, extra);

    let mut evaluator = TypeEvaluator::with_resolver(&interner, &resolver);
    let result = evaluator.evaluate(source);

    let members = intersection_members(&interner, result);
    assert!(
        members.contains(&window_ref),
        "the interface reference must keep its Lazy identity in the evaluated \
         intersection; got members {members:?}"
    );
    let expansion_members = intersection_members(&interner, resolver.iface_expansion);
    for spliced in &expansion_members {
        assert!(
            !members.contains(spliced),
            "heritage constituent {spliced:?} must not be spliced into the \
             outer intersection"
        );
    }
}

/// Negative control: a type-ALIAS member whose body is an intersection keeps
/// today's deferred-reduction expansion (`string & T[K]`-style reduction
/// depends on it) — identity preservation is for class/interface defs only.
#[test]
fn type_alias_intersection_body_still_expands() {
    let interner = TypeInterner::new();
    let resolver = build_fixture(&interner);

    let alias_ref = interner.lazy(resolver.alias);
    let extra = named_object(&interner, "extra", TypeId::NUMBER);
    let source = interner.intersection2(alias_ref, extra);

    let mut evaluator = TypeEvaluator::with_resolver(&interner, &resolver);
    let result = evaluator.evaluate(source);

    let members = intersection_members(&interner, result);
    assert!(
        !members.contains(&alias_ref),
        "a type-alias member must still expand; got members {members:?}"
    );
    assert!(
        members.contains(&interner.lazy(DefId(201))),
        "the alias body's constituents must be spliced in; got members {members:?}"
    );
}

/// Negative control: an interface whose body evaluates to a plain object
/// shape (single declaration, no heritage) still materializes — that shape
/// carries its own symbol provenance, so nothing is lost by expanding it.
#[test]
fn interface_reference_with_object_body_still_materializes() {
    let interner = TypeInterner::new();
    let resolver = build_fixture(&interner);

    let plain_ref = interner.lazy(resolver.plain_iface);
    let extra = named_object(&interner, "extra", TypeId::NUMBER);
    let source = interner.intersection2(plain_ref, extra);

    let mut evaluator = TypeEvaluator::with_resolver(&interner, &resolver);
    let result = evaluator.evaluate(source);

    // Both members are plain object shapes after expansion, so the interner's
    // normalization merges them into one shape — the pre-#17332 behavior this
    // fix must NOT disturb.
    assert!(
        matches!(interner.lookup(result), Some(TypeData::Object(_))),
        "an object-bodied interface member must still materialize and merge; got {:?}",
        interner.lookup(result)
    );
}
