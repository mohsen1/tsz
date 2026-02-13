# Debugging Session: Type Resolution Bug

**Date**: 2026-02-13
**Status**: Phase 1 Complete - Root Cause Identified
**Estimated Fix Time**: 3-5 hours

## Problem

6 tests fail with false positive type errors because imported types resolve to wrong global types.

## Evidence Gathered

### Reproduction
```typescript
// tmp/wrapClass.ts
export type Constructor<T = {}> = new (...args: any[]) => T;
export function Timestamped<TBase extends Constructor>(Base: TBase) {
    return class extends Base { timestamp = Date.now(); };
}

// tmp/index.ts
import { Timestamped } from "./wrapClass";
export class User { name = ''; }
export class TimestampedUser extends Timestamped(User) {}
```

**Error**: Type '{ new (): User }' is not assignable to 'AbortController<{}>'
**Expected**: 0 errors

### Root Cause

Symbol resolution trace shows:
- Resolver searches for "AbortController" instead of "Constructor"
- Import name is replaced before resolution
- Falls back to global type instead of imported type alias

**Location**: Type parameter constraint resolution incorrectly resolves imported type names.

## Next Steps (3-5 hours)

1. Find where constraint types are resolved
2. Compare with working type reference resolution
3. Fix scope/lookup mechanism
4. Create test, verify fix

## Impact

Would fix 6 tests: 90% → 96% pass rate
