# Conformance Tests 100-199: Pass Rate Investigation

## Current Status
- **Pass Rate**: 90/100 (90%)
- **Previous**: 89/100 
- **Improvement**: +1 test

## Failure Analysis

### False Positives (6 tests) - We emit errors TSC doesn't
1. **ambientClassDeclarationWithExtends.ts** - EXTRA: TS2322
   - Ambient classes with extends and namespace merging
   - We incorrectly emit type assignability error

2. **amdDeclarationEmitNoExtraDeclare.ts** - EXTRA: TS2322, TS2345
   - AMD module with class mixin pattern
   - Declaration emit mode

3. **amdModuleConstEnumUsage.ts** - EXTRA: TS2339
   - Const enum access after import
   - Module resolution with baseUrl

4. **amdLikeInputDeclarationEmit.ts** - EXTRA: TS2339
   - JS with JSDoc, AMD pattern
   - checkJs + allowJs + declaration emit

5. **anonClassDeclarationEmitIsAnon.ts** - EXTRA: TS2345
   - Class expressions in return types
   - Mixin pattern with Timestamped

6. **argumentsObjectIterator02_ES6.ts** - EXTRA: TS2488
   - arguments[Symbol.iterator] access
   - Should be allowed in ES6

### All-Missing (2 tests) - We don't emit errors TSC does
1. **argumentsReferenceInConstructor4_Js.ts** - MISSING: TS1210
   - `const arguments = this.arguments` in constructor
   - TS1210: "Code contained in a class is evaluated in JavaScript's strict mode"

2. **argumentsReferenceInFunction1_Js.ts** - MISSING: TS2345, TS7006
   - Argument spread with `format.apply(null, arguments)`
   - Missing parameter type errors

### Wrong-Code (2 tests) - Both have errors, codes differ
1. **ambiguousGenericAssertion1.ts**
   - Expected: [TS1005, TS1109, TS2304]
   - Actual: [TS1005, TS1109, TS1434]
   - Difference: We emit TS1434 instead of TS2304
   - Issue: Parser treats `<<T>` as left shift, should detect undefined `T`

2. **argumentsObjectIterator02_ES5.ts**
   - Expected: [TS2585]
   - Actual: [TS2495, TS2551]
   - ES5 doesn't support Symbol.iterator on arguments

## Top Error Code Impact

**False Positives (fix = immediate wins):**
- TS2322: 2 tests (assignability)
- TS2345: 2 tests (argument type)
- TS2339: 2 tests (property doesn't exist)
- TS2488: 1 test (iterator)

**Not Implemented (implement = new passes):**
- TS1210: 1 test (strict mode arguments)
- TS2304: 1 test (cannot find name)
- TS2585: 1 test (iterator downlevel)
- TS7006: 1 test (implicit any)

## Strategic Recommendations

### Highest ROI: Fix False Positive Patterns

**Pattern 1: Declaration Emit Mode**
- Tests with `@declaration: true` or `@emitDeclarationOnly: true`
- We may be over-checking in declaration emit mode
- Check if we should skip certain type checks when only emitting declarations

**Pattern 2: Ambient Classes**
- `declare class` with extends and namespace merging
- May incorrectly check instantiation of ambient classes

**Pattern 3: Const Enum Access**
- Imported const enums should inline values
- We may be missing the const enum optimization

### Medium ROI: Implement Missing Error Codes

**TS1210 - Strict Mode Arguments**
- Detect `const arguments = ...` in constructors
- Only in class context (strict mode)

**TS2304 - Cannot Find Name**
- Generic type parameter not resolved correctly
- May be a parser issue with `<<T>` ambiguity

## Next Steps

1. **Investigate declaration emit mode** - Check if type checking should be suppressed
2. **Test ambient class pattern** - Create minimal reproduction
3. **Trace TS2322 emission** - Find where assignability check fires incorrectly
4. **Implement TS1210** - Add strict mode arguments check
