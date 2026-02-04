# Session tsz-2: Generic Type Inference & Application Expansion

**Started**: 2026-02-05
**Status**: Active
**Goal**: Fix Application type expansion to enable complex generic libraries (Redux, etc.)

## Problem Statement

From Gemini's consultation:

> "The comments in `src/solver/evaluate.rs` (lines 185-215) explicitly identify a major bottleneck: `Application(Lazy(def_id), args)` types (like `Reducer<S, A>`) are often not expanding to their structural forms. This leads to 'Any poisoning' or `Ref(N)<error>` diagnostics."

**Impact**: 
- Blocks complex libraries like Redux
- Causes hundreds of `TS2339` (Property does not exist) and `TS2322` (Type not assignable) errors
- Prevents the checker from seeing underlying function signatures of generic type aliases

## Technical Details

**Files**: 
- `src/solver/infer.rs` - Type inference logic
- `src/solver/evaluate.rs` - Type evaluation with Application bottleneck (lines 185-215)
- `src/solver/application.rs` - Application type handling

**Root Cause**: 
`Application(Lazy(def_id), args)` types pass through unchanged in many contexts, preventing the Solver from expanding generic type aliases to their structural forms.

**Example**:
```typescript
type Reducer<S, A> = (state: S, action: A) => S;
const myReducer: Reducer<State, Action> = (state, action) => state;

// TypeScript can see myReducer is a function with (state, action) => state
// tsz sees Application(Lazy(Reducer), [State, Action]) without expanding
```

## Implementation Strategy

### Phase 1: Investigation (Pre-Implementation)
1. Read `src/solver/evaluate.rs` lines 185-215 to understand current Application handling
2. Read `src/solver/application.rs` to understand Application type structure
3. Ask Gemini: "What's the correct approach to expand Application types? Where should expansion happen?"

### Phase 2: Implementation
1. Implement Application type expansion in evaluation loop
2. Ensure recursive expansion for nested generics
3. Add conformance tests to verify improvement

### Phase 3: Validation
1. Run conformance tests to measure improvement
2. Test with real-world generic libraries (Redux, RxJS, etc.)
3. Ask Gemini Pro to review implementation for correctness

## Success Criteria

- [ ] Generic type aliases expand to structural forms
- [ ] `TS2339` and `TS2322` errors reduced significantly
- [ ] Complex libraries like Redux work correctly
- [ ] No "Any poisoning" from opaque Application types
- [ ] Conformance test pass rate increases measurably

## Session History

*Created 2026-02-05 following Gemini consultation after tsz-1 conclusion.*
