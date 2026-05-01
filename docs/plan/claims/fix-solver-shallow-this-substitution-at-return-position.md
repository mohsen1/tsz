---
name: Fix solver shallow this substitution at return position
description: Split `substitute_this_type` into deep + shallow variants. Shallow (new) is used at call-return-position and bind_object_receiver_this paths to keep stored Object/Function/Callable internals' `this` references polymorphic for intersection rebinding.
type: project
branch: fix-solver-split-this-substitution-shallow-vs-deep
status: ready
scope: solver (this-type substitution / instantiation)

## Summary

Fix the chained `extend({a}).extend({b})` regression in
`intersectionThisTypes.ts` by introducing a shallow-only variant of
`substitute_this_type` and routing the call-return / property-access
binding paths through it. Stored Object/Function/Callable internals
keep raw `this` references so the next call-site rebinding sees the
full intersection receiver.

## Root Cause

`apply_this_substitution_to_call_return`, `bind_object_receiver_this`
and the property-access `this`-recovery path (`property_access_type/
resolve.rs:1805,1830`) all called `substitute_this_type`, which uses
`TypeInstantiator::instantiate` to walk the entire type tree. When the
substituted target was an Object whose stored method bodies referenced
`this`, the walk re-instantiated those properties — baking `this -> the
substituted target's id` (e.g. `Label_lazy`) into Label's `extend`
return type.

Once baked, the merged-intersection Object that resulted from
`label.extend({id})` carried `Label & T` for `extend`'s return, with no
raw `this` left. The second `inner.extend({tag})` call's `this`
substitution couldn't recover the full intersection receiver, dropping
`{id}` and producing `Label & {tag}` instead of
`Label & {id} & {tag}`.

## Fix

Two-call split:
1. **Deep walk** (existing `substitute_this_type` /
   `substitute_this_type_cached`): used by class-inheritance
   specialization and heritage merge paths where the substitution
   genuinely means "specialize this method body for this class".
2. **Shallow walk** (new
   `substitute_this_type_at_return_position`): used by
   `apply_this_substitution_to_call_return`, `bind_object_receiver_this`
   and the property-access `this`-recovery sites. Skips recursion into
   named (`shape.symbol.is_some()`) Object/ObjectWithIndex internals
   and into Function/Callable bodies when `shallow_this_only=true`.

A new field `shallow_this_only: bool` on `TypeInstantiator` carries the
flag through the visitor.

## Files Changed

- `crates/tsz-solver/src/instantiation/instantiate.rs` — new field +
  `substitute_this_type_at_return_position` function.
- `crates/tsz-solver/src/lib.rs` — re-export.
- `crates/tsz-solver/src/operations/property.rs` —
  `bind_object_receiver_this` routes to shallow.
- `crates/tsz-checker/src/query_boundaries/common.rs` — wrapper.
- `crates/tsz-checker/src/state/state.rs` —
  `apply_this_substitution_to_call_return` routes to shallow (both
  receiver-from-node and receiver-from-symbol branches).
- `crates/tsz-checker/src/types/property_access_type/resolve.rs` —
  the two `this`-recovery sites route to shallow.

## Verification

- `intersectionThisTypes.ts` flips FAIL → PASS.
- Unit tests: tsz-solver (5576) + tsz-checker (3097) + tsz-core (3038)
  all green, including `test_covariant_this_basic_subtyping` and
  `test_covariant_this_unsound_call` (which lock super-call `this`
  rebinding).
- Conformance: 1 improvement (the target), **0 regressions**.
  Net **+1** (12304 → 12305).

## Routing Policy (final)

After tracing which call sites need shallow vs deep:

- `apply_this_substitution_to_call_return` → **shallow**.
- `bind_object_receiver_this` → **shallow**.
- `property_access_type/resolve.rs:1805,1830` (this-recovery sites)
  → **deep** (super-call rebinding depends on this; the `raw_this`
  recovery already gets the un-baked form, so deep substitution
  here doesn't re-bake).
- All other call sites (heritage merge, class instantiation, etc.)
  → **deep** (default, unchanged).

## How the Function/Callable Carve-Out Works

The shallow mode in the Function/Callable arms doesn't simply skip
recursion — that broke `this:` parameter annotation tests
(`contextualThisType.ts`, `looseThisTypeInFunctions.ts`,
`unionThisTypeInFunctions.ts`).

Instead, the shallow arm performs **top-level-only ThisType
substitution** within the Function/Callable shape:

- `this:` parameter slot: substituted (always top-level).
- Each parameter's `type_id`: substituted only if it IS literally
  `TypeData::ThisType` (not nested in a composite).
- Return type: same — substituted only if literally `ThisType`.

This handles `(p: this) => this` (params and return are top-level
ThisType) but NOT `(props: T) => this & T` (return is an Intersection
that contains ThisType, which must stay raw for intersection
rebinding).

The distinction: **top-level `this`** is a polymorphic-receiver type
that the call site must specialize. **Nested `this` in a composite
return type** is a polymorphic body fragment that must stay raw
through method binding so the next call-site rebinding can place it
correctly when the receiver is itself an intersection.
