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
