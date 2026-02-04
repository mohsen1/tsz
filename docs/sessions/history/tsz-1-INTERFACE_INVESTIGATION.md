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

## Status: INVESTIGATION IN PROGRESS

### Test Analysis
The test has an EMPTY class implementing an interface:
```typescript
interface Printable { print(): void; }
class Doc implements Printable { }  // Empty class!
let doc: Doc;
doc.print();
```

**tsc Behavior Investigation**:
- Command line: Emits TS2420 (missing property) AND TS2339 (property doesn't exist)
- But test expects NO TS2339

**Hypothesis**: The test might be checking a specific scenario where TS2339 should not be emitted, possibly related to:
- Lib context setup differences
- Structural typing behavior
- How property access resolution works for interfaces

**Complexity**: Medium-High
- Requires understanding tsc's structural typing rules
- Property access resolution for implemented interfaces
- Possibly related to how `Doc` type is constructed and merged with `Printable`

**Recommendation**: This requires deeper investigation into:
1. How tsc handles property access on empty classes implementing interfaces
2. Whether there's a specific scenario where TS2339 is suppressed
3. Property resolution in type checker vs class type creation

**Time Spent**: ~45 minutes investigating
**Conclusion**: More complex than initially assessed. Requires dedicated debugging session with tsc behavior analysis.
