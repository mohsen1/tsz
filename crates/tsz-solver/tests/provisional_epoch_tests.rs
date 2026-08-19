//! Tests pinning the provisional-value-epoch contract (issue #16055).
//!
//! A resolver serving a value derived from a mid-resolution class partial
//! bumps [`TypeResolver::provisional_value_epoch`]. An evaluation during
//! which the epoch moved is a function of that resolution window, not of its
//! input `TypeId`s, so the evaluator must fold the movement into
//! `unresolved_def_seen` — the flag every `TypeId`-keyed cache-write gate
//! already consults — and a later evaluation must recompute against the
//! completed body instead of reading a poisoned cache entry.

use super::*;
use crate::construction::TypeInterner;
use std::cell::Cell;

/// Resolver standing in for a checker mid-way through building a class:
/// every `resolve_lazy` answer is derived from a provisional partial, so the
/// epoch moves on each serve.
struct ProvisionalServingResolver {
    epoch: Cell<u64>,
    body: TypeId,
}

impl TypeResolver for ProvisionalServingResolver {
    fn resolve_ref(&self, _symbol: SymbolRef, _interner: &dyn TypeDatabase) -> Option<TypeId> {
        None
    }

    fn resolve_lazy(&self, _def_id: DefId, _interner: &dyn TypeDatabase) -> Option<TypeId> {
        self.epoch.set(self.epoch.get() + 1);
        Some(self.body)
    }

    fn provisional_value_epoch(&self) -> u64 {
        self.epoch.get()
    }
}

/// Resolver whose answers are final: the epoch never moves.
struct SettledResolver {
    body: TypeId,
}

impl TypeResolver for SettledResolver {
    fn resolve_ref(&self, _symbol: SymbolRef, _interner: &dyn TypeDatabase) -> Option<TypeId> {
        None
    }

    fn resolve_lazy(&self, _def_id: DefId, _interner: &dyn TypeDatabase) -> Option<TypeId> {
        Some(self.body)
    }
}

#[test]
fn provisional_epoch_movement_during_evaluation_marks_unresolved_def_seen() {
    let interner = TypeInterner::new();
    let value_name = interner.intern_string("value");
    let body = interner.object(vec![PropertyInfo::new(value_name, TypeId::STRING)]);
    let resolver = ProvisionalServingResolver {
        epoch: Cell::new(0),
        body,
    };

    // A tuple with a `Lazy` element forces a resolver excursion during the
    // tuple's own evaluation — the shape of the #16055 witness, where the
    // `[ZodTypeAny, ...ZodTypeAny[]]` alias body materialized its fixed slot
    // against a mid-resolution class partial.
    let lazy = interner.lazy(DefId(901));
    let tuple = interner.tuple(vec![crate::types::TupleElement {
        type_id: lazy,
        name: None,
        optional: false,
        rest: false,
    }]);

    let mut evaluator = TypeEvaluator::with_resolver(&interner, &resolver);
    let _ = evaluator.evaluate(tuple);

    assert!(
        evaluator.is_unresolved_def_seen(),
        "an evaluation during which the resolver served a provisional class \
         partial must report unresolved_def_seen so TypeId-keyed caches skip \
         the write (issue #16055)"
    );
}

#[test]
fn settled_resolver_evaluation_stays_cacheable() {
    let interner = TypeInterner::new();
    let value_name = interner.intern_string("value");
    let body = interner.object(vec![PropertyInfo::new(value_name, TypeId::STRING)]);
    let resolver = SettledResolver { body };

    let lazy = interner.lazy(DefId(902));
    let tuple = interner.tuple(vec![crate::types::TupleElement {
        type_id: lazy,
        name: None,
        optional: false,
        rest: false,
    }]);

    let mut evaluator = TypeEvaluator::with_resolver(&interner, &resolver);
    let _ = evaluator.evaluate(tuple);

    assert!(
        !evaluator.is_unresolved_def_seen(),
        "a resolver whose answers are final must not taint the evaluation — \
         the provisional gate is scoped to mid-resolution serves only"
    );
}

#[test]
fn epoch_movement_between_evaluations_taints_only_the_overlapping_run() {
    let interner = TypeInterner::new();
    let value_name = interner.intern_string("value");
    let body = interner.object(vec![PropertyInfo::new(value_name, TypeId::STRING)]);
    let resolver = ProvisionalServingResolver {
        epoch: Cell::new(0),
        body,
    };

    // First run: no `Lazy` node, no resolver excursion, epoch never moves —
    // the run must stay clean even on a resolver that WOULD serve
    // provisionally if asked.
    let plain = interner.object(vec![PropertyInfo::new(value_name, TypeId::NUMBER)]);
    let mut evaluator = TypeEvaluator::with_resolver(&interner, &resolver);
    let _ = evaluator.evaluate(plain);
    assert!(
        !evaluator.is_unresolved_def_seen(),
        "a run with no provisional serve must stay clean"
    );

    // Second run on the same evaluator forces the excursion; only now does
    // the taint appear.
    let lazy = interner.lazy(DefId(903));
    let tuple = interner.tuple(vec![crate::types::TupleElement {
        type_id: lazy,
        name: None,
        optional: false,
        rest: false,
    }]);
    let _ = evaluator.evaluate(tuple);
    assert!(
        evaluator.is_unresolved_def_seen(),
        "the run overlapping the provisional serve must be tainted"
    );
}
