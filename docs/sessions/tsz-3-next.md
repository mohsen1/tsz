# Session tsz-3: Method Parameter Bivariance

**Started**: 2026-02-06
**Status**: Active - Planning Phase
**Predecessor**: tsz-3-investigations (void/string/keyof all already implemented)

## Completed Sessions

1. **In operator narrowing** - Filtering NEVER types from unions in control flow narrowing
2. **TS2339 string literal property access** - Implementing visitor pattern for primitive types
3. **Conditional type inference with `infer` keywords** - Fixed `collect_infer_type_parameters_inner` to recursively check nested types
4. **Anti-Pattern 8.1 refactoring** - Eliminated TypeKey matching from Checker

## Current Task: Method Parameter Bivariance

### Task Definition (from Gemini Consultation)

**Implement Method Parameter Bivariance** to fix extra TS2322 errors where tsz is too strict.

TypeScript allows method parameters to be bivariant (both A <: B AND B <: A are acceptable), even when `strictFunctionTypes` is on. This is a "Lawyer" override per NORTH_STAR.md Section 3.3.

### The Failing Test Case

```typescript
interface Animal { _isAnimal: any }
interface Dog extends Animal { _isDog: any }

interface Handler {
    // This is a METHOD, so it should be bivariant
    handle(a: Animal): void;
}

const h: Handler = {
    handle(d: Dog) {} // tsz: Error TS2322 (extra), tsc: OK
};
```

### Files to Investigate

Per Gemini's recommendation:
1. **`src/solver/types.rs`** - Add `is_method: bool` flag to `FunctionShapeId` or `TypeKey::Function`
2. **`src/solver/lower.rs`** - Set the flag when converting method declarations/signatures
3. **`src/solver/subtype.rs`** - Modify `check_function_subtyping` to use bivariance for methods

### Implementation Approach (Pending Gemini Question 1)

**Planned Steps**:
1. Add `is_method` flag to distinguish methods from functions
2. Update lowering to set the flag for method declarations
3. Modify subtype checking to allow bivariance for method parameters
4. Follow MANDATORY Gemini workflow (Two-Question Rule)

### MANDATORY Gemini Workflow

Per AGENTS.md, must ask **Question 1 (Approach Validation)** before implementing:

```bash
./scripts/ask-gemini.mjs --include=src/solver "I need to implement Method Parameter Bivariance to fix extra TS2322 errors.

My plan:
1. Add an 'is_method' flag to TypeKey::Function or FunctionShapeId
2. Modify check_function_subtyping in src/solver/subtype.rs
3. For parameters: if either source or target is a method, allow bivariance
   (source <: target) OR (target <: source)

Is this the correct approach? Should the flag be on the TypeKey or passed as a context flag?"
```
