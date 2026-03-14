# Cross-Campaign Note: generic-inference → subtype-relations

## Issue: Type parameter constraint not used in keyof assignability

When `S extends State` and `K extends keyof S`, calling a method
`set<K2 extends keyof S>(key: K2)` with `key: K` fails with:
`TS2345: Argument of type 'K' is not assignable to parameter of type 'keyof S'`

### Root cause
The assignability checker doesn't use K's declared constraint (`keyof S`)
when checking `K <: keyof S`. It tries to directly check the abstract
type parameter K against a concrete evaluation of `keyof S`, which fails.

### Minimal repro
```typescript
interface Store<S> { set<K extends keyof S>(key: K): void; }
type State = { x: number };
function test<S extends State, K extends keyof S>(store: Store<S>, key: K) {
    store.set(key); // TS2345: K not assignable to keyof S
}
```

### Expected fix location
Subtype checker: when checking `TypeParam(K) <: keyof S`, should use K's
constraint. If constraint IS `keyof S`, it's trivially assignable.

### Affected tests
- indexedAccessAndNullableNarrowing.ts
- Potentially many other tests with `K extends keyof T` patterns
