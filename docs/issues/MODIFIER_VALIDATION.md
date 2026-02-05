# Missing Modifier Validation Errors

**Status**: NEEDS FIX (requires Gemini consultation)
**Discovered**: 2026-02-05
**Component**: Checker/Grammar validation
**Conformance Impact**: ~10 tests

## Problem

tsz emits incorrect or missing errors for invalid modifier usage on class members. TSC performs specific validation checks that tsz is missing or reporting generically.

## Missing Errors

### TS1089: 'static' modifier cannot appear on a constructor declaration

```typescript
// parserConstructorDeclaration2.ts
class C {
  static constructor() { }  // TSC: TS1089, tsz: no error!
}
```

### TS1031: 'export' modifier cannot appear on class elements of this kind

```typescript
// parserConstructorDeclaration3.ts
class C {
  export constructor() { }  // TSC: TS1031, tsz: TS1012 (generic)
}
```

### TS1012 vs Specific Errors

tsz emits the generic TS1012 "Unexpected modifier" instead of more specific errors like TS1031, TS1089, etc.

## Root Cause

The grammar validation in tsz's checker doesn't have all the specific modifier validation rules that TSC has. TSC checks:
- Which modifiers are valid on which kinds of declarations
- Emits specific error codes for each invalid combination

## Affected Tests

- `parserConstructorDeclaration2.ts` - TS1089 missing
- `parserConstructorDeclaration3.ts` - TS1031 missing (gets TS1012)
- Likely other modifier-related tests

## Severity

- **User Impact**: Low - user still gets an error, just less specific
- **Conformance Impact**: Medium - affects several tests
- **Fix Complexity**: Medium - needs checker changes

## Fix Approach

1. Identify where modifier validation occurs in checker
2. Add specific checks for constructor modifiers
3. Add specific checks for class member modifiers
4. Emit the correct specific error codes

## Related Files

- `src/checker/` - Grammar validation
- TSC: `src/compiler/checker.ts` - `checkGrammarModifiers`

## Testing

Run constructor declaration tests:
```bash
cd conformance-rust && cargo run --release --bin tsz-conformance -- \
  --filter ConstructorDeclaration --all \
  --test-dir ../TypeScript/tests/cases \
  --tsz-binary ../.target/release/tsz \
  --cache-file ../tsc-cache-full.json
```
