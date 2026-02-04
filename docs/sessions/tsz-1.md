# Session tsz-1: False Positive TS2339 on Interface Properties

**Started**: 2026-02-04 (Fifth iteration)
**Goal**: Fix false positive TS2339 for properties from implemented interfaces

## Problem Statement

Test `test_class_implements_interface_property_access` is failing with a false positive TS2339 error.

**Test Case**:
```typescript
interface Printable { print(): void; }
class Doc implements Printable { }
let doc: Doc;
doc.print();  // Should work - Doc implements Printable
```

**Expected**: No errors
**Actual**: TS2339 "Property 'print' does not exist on type 'Doc'"

## Investigation

The issue is that when a class implements an interface, the interface's properties should be accessible on instances of that class. Currently, tsz is not recognizing that `Doc` has the `print` method from the `Printable` interface it implements.

## Files to Investigate

1. Interface implementation checking in checker
2. Property access resolution for class instances
3. Type merging for implemented interfaces
4. Symbol lookup for class properties from interfaces

## Success Criteria

- Test `test_class_implements_interface_property_access` passes
- Properties from implemented interfaces are accessible
- No false positive TS2339 errors
- No regressions in other property access tests

## Status: READY TO BEGIN
Investigating property access resolution for implemented interfaces.
