# investigation: chained `this & T` loses earlier intersection augmentations

- **Date**: 2026-05-01
- **Branch**: TBD
- **PR**: TBD
- **Status**: investigation (not claimed)
- **Workstream**: 1 (Conformance — `intersectionThisTypes.ts` extra TS2339, diff=1)

## Symptom

`conformance/types/intersection/intersectionThisTypes.ts`:

```ts
interface Component { extend<T>(props: T): this & T; }
interface Label extends Component { title: string; }
function test(label: Label) {
    const extended = label.extend({ id: 67 }).extend({ tag: "hello" });
    extended.id;  // tsc: ok. tsz: TS2339 "Property 'id' does not exist on type 'Label & { tag: \"hello\"; }'"
    extended.tag;
}
```

Chained `extend({ id }).extend({ tag })` loses the `{ id: number }` augmentation. Expected `Label & { id: number } & { tag: "hello" }`; we produce `Label & { tag: "hello" }`.

## Repro

```bash
cat > /tmp/this_chain.ts <<'EOF'
interface Component { extend<T>(props: T): this & T; }
interface Label extends Component { title: string; }
function test(label: Label) {
    const inner = label.extend({ id: 67 });
    const _: never = inner;                         // shows: '{ id: number; } & Label'
    const outer = inner.extend({ tag: "hello" });
    const __: never = outer;                        // shows: 'Label & { tag: string; }'  ← BUG
}
EOF
./.target/dist-fast/tsz /tmp/this_chain.ts
```

## What I traced (don't redo this)

Added `eprintln!` at:
- `state.rs::apply_this_substitution_to_call_return` (where receiver-based `this` substitution happens for call return types).
- `property_access_type/resolve.rs:~1787` around `this_substitution_target` assignment plus the `contains_this` branch and the `raw_this` recovery branch.

For the second `inner.extend(...)`:
- `original_object_type = "{ id: number; } & Label"` ✓ (correct receiver)
- `this_substitution_target = "{ id: number; } & Label"` ✓
- `prop_type` (looked up `extend` on the intersection) **already** displays as `<T>(props: T) => Label & T`, with `contains_this_type` returning **false**.
- The `raw_this` recovery (`resolve_property_access_raw_this`) returns the **same** pre-substituted shape — `contains_this` still false.
- Result: substitution at the property-access site is a no-op; `state.rs::apply_this_substitution_to_call_return` then sees a return type with no `this`, so its substitution is also a no-op; the final result keeps `Label`, dropping `{ id: number }`.

So `this` is being substituted to `Label` (the *interface that owns the method*, not the actual receiver) somewhere **upstream** of `PropertyAccessEvaluator` — which means `set_skip_this_binding(true)` (set by the intersection-member loop in `tsz-solver/src/operations/property.rs:472-489`) does not actually skip whatever path is doing the early bind.

## Suspect paths (haven't pinned the exact one)

1. **Lazy resolution of `Label`.** When the intersection member `Label` is resolved from its lazy form, the heritage chain walk inherits `extend` from `Component`. The inherited method may be re-stored with `this -> Label` baked in at definition time (rather than left polymorphic). If that's the case, the `extend` we look up on the intersection has already lost its `this`.
2. **Object-shape caching.** The structural shape of `Label` may include a "resolved" `extend` whose return type is `Label & T`. Once cached, `skip_this_binding` at the visitor level can't recover it.
3. **Heritage walking in property visitor.** There may be a path where, when looking up an inherited member, we substitute `this` with the interface that declares it (`Component`) or the inheriting interface (`Label`) before returning to the visitor.

The skip-this-binding handling at `property.rs:232-242` (`bind_object_receiver_this`) is what the intersection loop **expected** to no-op out, and it does. The leak is somewhere earlier.

## Where to look first

- `crates/tsz-solver/src/operations/property_visitor.rs` — where `Object` shapes are visited; check whether `bind_object_receiver_this` is the only `this` substitution path (likely not).
- Any `substitute_this_type` calls inside `tsz-checker/src/state/type_environment/lazy.rs` or `state/type_environment/type_node_resolution.rs` that fire during interface body resolution.
- `crates/tsz-binder/src/...` heritage merge — does the binder fold inherited members into the inheriting interface's shape with `this` already substituted?

## Test target

- `TypeScript/tests/cases/conformance/types/intersection/intersectionThisTypes.ts` — currently FAIL with diff=1 (extra TS2339).
- Expected fix: net +1 conformance, no regressions.

## Why this isn't yet claimed

The substitution leak point is upstream of the property visitor's `skip_this_binding` and is shared with many other paths (any property access through a Lazy interface). A naive fix could regress other tests. Needs the investigator to find the exact substitution site, then verify the broader test suite.

## Follow-up trace (2026-05-01)

Added `eprintln!` at `tsz-solver/src/operations/property_visitor.rs::visit_object_impl` to dump the raw property type read out of the object shape (before `bind_object_receiver_this`). The trace shows two **distinct** `obj_type` ids visited for the same conceptual `Label`:

```
DEBUG visit_object_impl: prop="extend" raw_type=TypeId(1202) contains_this=true  obj_type=TypeId(1009)   ← first call (label.extend)
DEBUG visit_object_impl: prop="extend" raw_type=TypeId(1363) contains_this=false obj_type=TypeId(1111)   ← second call (inner.extend)
```

So the issue is **not** that we re-substitute `this` at lookup time. The issue is that `inner`'s type — `{ id: number } & Label` — is **flattened** (during call-return computation or contextual-type evaluation) into a single new `Object` (TypeId 1111) whose `extend` property already has `this` substituted to a stale receiver. By the time the second property access runs, there is no `this` left to skip-bind.

The flattening probably runs `substitute_this_type(this & T, flattened_object)` once, eagerly. The intersection visitor's `skip_this_binding` (and any `Lazy` hook that respects it) cannot recover.

### Tried and reverted

Adding a `skip_this_binding` check to the **`Lazy` arm** of `resolve_property_access_inner` (`property.rs:944-949`) so it preserves raw `this` when the intersection visitor sets the skip flag:

```rust
let resolved = if !self.skip_this_binding.get()
    && crate::contains_this_type(self.interner(), resolved)
{
    crate::substitute_this_type(self.interner(), resolved, obj_type)
} else {
    resolved
};
```

- Builds, all 5806 checker tests + 5567 solver tests pass.
- Does **not** fix `intersectionThisTypes.ts` (the `Lazy` arm isn't the path being taken — the cached flattened `Object` is).
- Conformance: net **−1** (`typeFromParamTagForFunction.ts` PASS → FAIL). Reverted; not committed.

### Where the actual fix has to land

- The intersection-flattening path (look in `crates/tsz-solver/src/evaluation/`, especially intersection normalization that converts `A & B` of object members into a single flattened `Object`) needs to **either** keep `A & B` as a structural `Intersection` of two members so the visitor can still use `skip_this_binding`, **or** preserve raw `this` on the flattened result so each call-site substitution can rebind to the actual receiver.
- The Lazy-arm fix above is still the right behavior for the case it covers, but it must land bundled with the flattening fix to net non-negative on conformance. Not safe to land alone.

## Second follow-up trace (2026-05-01, iter 26)

Pinpointed the flattening site. The intersection-to-Object merge happens in
`crates/tsz-solver/src/intern/intersection.rs::extract_and_merge_objects`
(called from `normalize_intersection`). Two object members of an
intersection get merged via `try_merge_objects_in_intersection` which
clones each property's `type_id` verbatim into the merged shape.

So the property's `type_id` itself is preserved unchanged; the merge does
NOT substitute `this`. Yet the trace below shows `Label`'s `extend`
property has `contains_this=true` (TypeId 1202), but the merged
`{id:number} & Label` Object's `extend` has `contains_this=false`
(TypeId 1363). Different TypeIds — same conceptual member.

```
DBG-VOI prop=extend obj=TypeId(1009) raw=TypeId(1202) contains_this=true   ← Label.extend
DBG-VOI prop=extend obj=TypeId(1111) raw=TypeId(1363) contains_this=false  ← inner.extend (merged)
```

So the substitution runs **between** Label's stored shape and the merged
shape. Most likely culprit: the call-return path
(`apply_this_substitution_to_call_return`) calls `substitute_this_type`
on the call's return type `this & T`. After that runs, the return type
is `Label & {id: number}`. Then this Intersection gets normalized by
`extract_and_merge_objects`, which clones each member's properties.
For the `Label` member, its body is read via `object_shape(...)` —
**but** `instantiate_type` may have been applied to it during the prior
substitute_this_type pass, which walks Object types via line 781-786 in
`instantiation/instantiate.rs` and re-interns each property after
calling `instantiate_properties`. That re-internment substitutes `this`
in every method body of Label.

So the eager bake site is `TypeData::Object` arm of `instantiate`
(`instantiation/instantiate.rs:781-786`):

```rust
TypeData::Object(shape_id) => {
    let shape = self.interner.object_shape(*shape_id);
    let instantiated = self.instantiate_properties(&shape.properties);
    self.interner
        .object_with_flags_and_symbol(instantiated, shape.flags, shape.symbol)
}
```

`instantiate_properties` runs `instantiate(prop.type_id)` for each prop,
which substitutes `this -> instantiator.this_type`. When the call-return
path calls `substitute_this_type(this & T, Label)`, the instantiator's
`this_type = Label`, and the WHOLE `this & T` shape walks through this
Object arm — meaning Label's stored body is re-instantiated with
`this -> Label` baked into every method.

### Tried and reverted (round 2)

1. Removed the `this` substitution from `evaluate.rs::visit_lazy` —
   no effect (Label is already in Object form before reaching this path).
2. Removed the `this` substitution from
   `property.rs::resolve_property_access_inner` Lazy arm — no effect
   (same reason).

### Real fix sketch

Either:
1. In the `TypeData::Object` arm of `instantiate`: when instantiating
   Object properties via `instantiate_properties`, **skip** properties
   whose type contains raw `this` if `instantiator.this_type` is set to
   the very Object being instantiated (would cause `this -> selfId` bake
   that defeats subsequent intersection rebinding). But "self vs the
   intersection that wraps self" is tricky to detect here.
2. Defer the call-return `this` substitution until after intersection
   normalization, and apply it only on the final post-normalization
   shape — so the merged Object can store raw `this` and the
   substitution sees the merged Object as the new receiver.
3. In `extract_and_merge_objects`: when one member's properties contain
   `this` baked to that member's own TypeId, *un-substitute* that
   `this -> member_id` back to raw `this` in the merged result. This
   restores the polymorphic shape so the next call-site substitution
   can bind `this` to the full intersection.

Option 3 is the most localized but requires a `unsubstitute_this_type`
pass, which doesn't exist yet. Option 2 is the most semantically
correct but reorders a substitution that's depended on by many other
sites. Option 1 is unsafe.

This stays unclaimed pending more investigation of which option breaks
the fewest existing assumptions.

## Iter 28 attempt (2026-05-01) — shallow_this_only flag

Added `shallow_this_only: bool` to `TypeInstantiator` and set it in
`substitute_this_type_cached`. When true, the Object/ObjectWithIndex/
Function/Callable arms `return self.interner.intern(*key)` instead of
re-instantiating their internals.

Result on `intersectionThisTypes.ts`: **FIXED** — `outer.id` and
`outer.tag` no longer error. The chained `extend({id}).extend({tag})`
correctly produces `Label & {id} & {tag}`.

Result on full conformance: net **−5** (12305 → 12300). 5 improvements,
10 regressions. Of the 10 regressions, 5 are flaky (also fail on main)
but 5 are genuine breakages caused by the filter:

- `subclassThisTypeAssignable02.ts` — uses `Vnode<A, this>` (`this`
  in type-argument position) and `view(this: State, ...)`
  (this-parameter annotation).
- `contextualThisType.ts`, `looseThisTypeInFunctions.ts`,
  `unionThisTypeInFunctions.ts` — function `this:` annotations need
  the substitution to walk into Function bodies.
- `superCallsInConstructor.ts`, `arrowFunctionContexts.ts` — class /
  arrow function `this` flow.

These tests fail because the filter blocks substitution into
Function/Callable internals **even when that substitution is the
intended behavior** (e.g., when computing the type of a method bound
to a specific class instance).

Variants tried (all reverted):
- Object-with-symbol filter only: the repro stays broken — recursion
  path that causes the bake goes through Function/Callable methods,
  not directly through Object.
- Object-with-symbol + Function + Callable filter: fixes the repro
  but breaks the 5 real regressions above.

### Why a static filter doesn't work

The substitution needs a **context-aware** distinction:

1. `apply_this_substitution_to_call_return` substituting `this` in
   `this & T` should only replace structural `ThisType` references at
   the return-type level. Walking into stored Object/Function bodies
   is harmful — they carry their own polymorphic `this`.
2. `instantiate_type_with_this` for class inheritance ("how does
   Subclass see Superclass's methods when `this` is the subclass")
   genuinely means "specialize this method body".

The two cases use the same `substitute_this_type` entry. The flag must
be set differently per call site, not as a blanket policy.

### Real fix sketch (round 3)

Split into two functions:
- `substitute_this_type_at_return_position(...)`: shallow. Used by
  `apply_this_substitution_to_call_return`.
- `substitute_this_type_for_class_specialization(...)`: deep.
  Used by class inheritance / heritage merging.

Then audit all current `substitute_this_type` call sites, classify
each as "call return" or "class specialization", and route to the
correct variant.

### Action items for next investigator

1. Audit all call sites of `substitute_this_type` /
   `substitute_this_type_cached`. Classify each.
2. Split into two functions; extend cache key with the variant.
3. Verify the 5 real-regression tests above stay green.
4. Verify the fix on `intersectionThisTypes.ts` still applies.

This is the third attempt; each iteration narrows the fix surface.
Unclaimed pending the call-site audit.
