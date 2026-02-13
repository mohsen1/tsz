# Conformance Tests 100-199: Next Steps

## Current Status
**90/100 tests passing (90%)**

## Key Findings

### TS2488 False Positive Investigation

**Test**: `argumentsObjectIterator02_ES6.ts`
- **Expected**: No errors
- **Actual**: TS2488 on line 7

**Root Cause**:
```typescript
let blah = arguments[Symbol.iterator];  // Type should be () => Iterator<any>
for (let arg of blah()) {  // TS2488: AbstractRange<any> must have Symbol.iterator
```

**Issue**: We're inferring the wrong type for the result of `blah()`. 
- Should be: `Iterator<any>` (which is itself iterable)
- We get: `AbstractRange<any>` (which lacks Symbol.iterator)

**Where to fix**:
- Type of `arguments` object in regular functions
- Element access with Symbol.iterator as key
- The IArguments interface type in lib files

### Pattern Analysis

**Declaration Emit Mode Tests** (3 failures):
- `amdDeclarationEmitNoExtraDeclare.ts`
- `amdLikeInputDeclarationEmit.ts`
- `anonClassDeclarationEmitIsAnon.ts`

Hypothesis: We may be over-checking types when `@emitDeclarationOnly: true` is set. TSC might skip certain type checks in declaration-only mode.

**Ambient Class Tests** (1 failure):
- `ambientClassDeclarationWithExtends.ts`

Hypothesis: Namespace merging with ambient classes may cause incorrect type assignment checks.

**Const Enum Tests** (1 failure):
- `amdModuleConstEnumUsage.ts`

Hypothesis: Const enum members should be inlined/accessible but we're emitting TS2339.

## Recommended Next Steps

### Priority 1: Quick Wins (Low Hanging Fruit)

1. **TS1210 - Arguments Shadowing**
   - Implement check for `const arguments = ...` in class constructors
   - Only 1 function needed, well-defined error condition
   - Estimated impact: +1 test

2. **Declaration Emit Mode Investigation**
   - Check if `ctx.compiler_options.emit_declaration_only` should skip type checking
   - May fix 3 tests at once
   - Estimated impact: +3 tests

### Priority 2: Type System Fixes (Medium Complexity)

3. **Arguments Type with Symbol.iterator**
   - Fix type inference for `arguments[Symbol.iterator]`
   - Check IArguments interface in lib files
   - Estimated impact: +1 test

4. **Const Enum Member Access**
   - Ensure const enum members are accessible post-import
   - Check module resolution + const enum handling
   - Estimated impact: +1 test

### Priority 3: Parser Issues (Higher Complexity)

5. **TS2304 vs TS1434 in Generic Context**
   - Parser treats `<<T>` as left-shift operator
   - Should emit TS2304 "Cannot find name 'T'" not TS1434
   - Estimated impact: +1 test

## Code Locations

### For TS2488 (Arguments Iterator):
- `crates/tsz-checker/src/iterable_checker.rs:438` - Where TS2488 is emitted
- `crates/tsz-checker/src/type_computation.rs` - Type of property access
- Built-in `IArguments` interface definition

### For Declaration Emit Mode:
- `crates/tsz-checker/src/checker.rs` - Main checking logic
- Check for `ctx.compiler_options.emit_declaration_only` flag usage

### For TS1210 (Arguments Shadowing):
- Add check in `crates/tsz-checker/src/state_checking.rs`
- Detect `const arguments = ` in constructor scope
- Use `ctx.compiler_options.target` to check if strict mode applies

## Testing Strategy

After each fix:
1. Run specific conformance test: `./scripts/conformance.sh run --max=1 --offset=N --verbose`
2. Run full test slice: `./scripts/conformance.sh run --max=100 --offset=100`
3. Run unit tests: `cargo nextest run --no-fail-fast`
4. Commit with clear message describing the fix

## Goal

Target: **95/100 (95%)** by end of session
- Requires fixing 5 more tests
- Focus on declaration emit mode (potential +3) and TS1210 (+1)
