# Type Inference Gaps Analysis - 2026-02-13

## Critical Missing Features

### 1. Higher-Order Generic Function Inference

**Status**: Missing
**Impact**: ~50-100 conformance tests
**Priority**: High

#### Problem
When a generic function is passed as an argument to another generic function, TSZ fails to instantiate the inner function's type parameters.

#### Example
```typescript
declare function pipe<A extends any[], B, C>(
  ab: (...args: A) => B,
  bc: (b: B) => C
): (...args: A) => C;

declare function list<T>(a: T): T[];
declare function box<V>(x: V): { value: V };

const f01 = pipe(list, box);  // FAILS in TSZ, works in TSC
```

**TSZ behavior**: Infers B as `unknown`, produces type errors
**TSC behavior**: Infers B as `T[]`, composes correctly

#### Root Cause
Location: `crates/tsz-solver/src/infer.rs`, lines ~1063-1090

The inference engine only handles `Function <: Function` inference. Missing cases:
- `Callable <: Function` - generic function to expected function type
- `Function <: Callable` - function to callable's signatures
- `Callable <: Callable` - matching callable signatures

#### Solution Approach
Add three new methods to `InferenceContext`:

1. `infer_callable_to_function`:
   - Detect if callable has type parameters
   - Create fresh inference variables for each type parameter
   - Build TypeSubstitution mapping param names to fresh variables
   - Instantiate params and return type with substitution
   - Convert to FunctionShape and perform regular inference

2. `infer_function_to_callable`:
   - Extract first call signature from callable
   - Convert to FunctionShape
   - Perform regular function inference

3. `infer_callable_to_callable`:
   - Match call signatures bidirectionally
   - Handle overloads properly

#### Related Tests
- `TypeScript/tests/cases/compiler/genericFunctionInference1.ts`
- Many tests involving `pipe`, `compose`, `map`, currying patterns

---

### 2. Mapped Type Inference

**Status**: Missing
**Impact**: Unknown (possibly 50+ tests)
**Priority**: High

#### Problem
When an object type is passed where a mapped type with a type parameter is expected, TSZ fails to infer the type parameter.

#### Example
```typescript
type Simple<T> = { [K in keyof T]: T[K] };

declare function identity<T>(x: Simple<T>): T;

interface A { a: string }
declare let a: A;

const result = identity(a);  // FAILS: infers T = unknown
result.a;  // Error: Property 'a' does not exist on type 'unknown'
```

**TSZ behavior**: Returns `unknown`, no inference
**TSC behavior**: Correctly infers T = A

#### Root Cause
Location: `crates/tsz-solver/src/infer.rs`

No handling for mapped type inference at all. When encountering:
- Source: Object type `{ a: string }`
- Target: Mapped type `{ [K in keyof T]: T[K] }`

The system needs to "invert" the mapped type to solve for T.

#### Complexity
The recursive case is even harder:

```typescript
type Deep<T> = { [K in keyof T]: Deep<T[K]> }
interface A { a: A }
declare function foo<T>(deep: Deep<T>): T;
const out = foo(a);  // Should infer T = A
```

This requires:
- Detecting the recursive structure
- Understanding coinductive semantics
- "Unwrapping" the recursion to find T

#### Solution Approach
1. Detect when target contains a mapped type with type parameter
2. For identity mappings `{ [K in keyof T]: T[K] }`:
   - Directly infer T from the source object structure
3. For transforming mappings (readonly, optional, etc.):
   - Apply inverse transformation
4. For recursive mappings:
   - Implement coinductive unwrapping
   - May need depth limits

#### Related Tests
- `TypeScript/tests/cases/compiler/mappedTypeRecursiveInference.ts`
- Many tests involving mapped type utilities

---

## Impact Assessment

### Test Failure Patterns

**Generic Function Inference Failures**:
- Pattern: Generic functions passed as arguments
- Error: Type parameter inferred as `unknown`
- Diagnostic: TS2345, TS2322, TS2769

**Mapped Type Inference Failures**:
- Pattern: Objects passed where mapped types expected
- Error: Return type is `unknown`
- Diagnostic: TS2339 (property does not exist on unknown)

### Recommended Fix Order

1. **Higher-Order Generic Function Inference** (easier, well-defined)
   - Clear algorithm
   - Similar to existing function inference
   - High test coverage impact

2. **Mapped Type Inference (Identity)** (moderate difficulty)
   - Handle simple identity mappings first
   - Builds foundation for recursive case

3. **Mapped Type Inference (Recursive)** (complex)
   - Requires careful coinductive reasoning
   - Needs depth limits and cycle detection
   - Consider TSC's actual algorithm

---

## Implementation Notes

### For Higher-Order Inference

Key insight: TypeScript represents generic functions as `CallableShape` with `CallSignature` objects that have `type_params`. When these appear in inference contexts, we need to:

1. Create fresh `InferenceVar` for each type parameter
2. Use `InferenceContext::fresh_type_param(name, is_const)`
3. Build `TypeSubstitution` with mappings
4. Use `instantiate_type()` from `crates/tsz-solver/src/instantiate.rs`

### For Mapped Type Inference

Key challenge: "Inverting" the mapped type to solve for T.

For `{ [K in keyof T]: T[K] }` (identity):
- If source is `{ a: string, b: number }`
- Then T must be `{ a: string, b: number }`

For `{ [K in keyof T]: Deep<T[K]> }` (recursive):
- Need to recursively solve
- May need to use evaluation/simplification first

---

## Testing Strategy

1. Create minimal reproduction cases in `tmp/`
2. Run with tracing: `TSZ_LOG=debug TSZ_LOG_FORMAT=tree`
3. Verify fix doesn't break existing tests: `cargo nextest run`
4. Run conformance subset: `./scripts/conformance.sh run --max=100`
5. Commit and sync after each working fix

---

## References

- **Architecture**: `docs/architecture/NORTH_STAR.md`
- **Coding conventions**: `docs/HOW_TO_CODE.md`
- **Key files**:
  - `crates/tsz-solver/src/infer.rs` - Inference engine
  - `crates/tsz-solver/src/instantiate.rs` - Type instantiation
  - `crates/tsz-solver/src/types.rs` - Type definitions
  - `crates/tsz-solver/src/subtype.rs` - Subtyping with callable handling
