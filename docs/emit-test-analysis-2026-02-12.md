# Emit Test Analysis - February 12, 2026

## Current Status
- **Pass Rate**: 46.2% (5489/11879 tests)
- **Target**: 80%+ (9503+ tests)
- **Gap**: ~4000 tests need fixes

## Major Blocking Patterns

### 1. Missing Downlevel Helpers (~3000-4000 tests, 50% of failures)
TypeScript emits runtime helpers for ES5 transpilation. Examples:

```typescript
// Generator functions need __generator helper
function* gen() { yield 1; }

// Async functions need __awaiter helper  
async function foo() { await x; }

// Template literals need __makeTemplateObject
tag`template ${x}`;
```

**Required helpers** (15+ total):
- `__generator` - Generator state machines
- `__awaiter` - Async/await transpilation
- `__makeTemplateObject` - Tagged templates
- `__spreadArray` - Array spread in ES5
- `__assign` - Object spread
- `__rest` - Rest parameters
- `__read` - Destructuring iteration
- And more...

**Effort**: 3-5 days to implement full system

### 2. Export Assignment Statement Ordering (783 tests)
`export = X` in CommonJS should emit `module.exports = X` at END of file:

```typescript
// Source
export = B;
export class C {}

// Expected
exports.C = void 0;
class C {}
module.exports = B;  // ← At end

// Actual (wrong)
exports.C = void 0;
module.exports = B;  // ← Too early!
class C {}
```

**Effort**: 1-2 days (requires module emit refactoring)

### 3. Temp Variable Naming (641 tests)
For-of loops and destructuring use temp variables. TypeScript renames on collision:

```javascript
// Expected
for (var _i = 0, _a = []; _i < _a.length; _i++) {
    var v = _a[_i];
    for (var _b = 0, _c = []; _b < _c.length; _b++) {
        var v_1 = _c[_b];  // ← Renamed to avoid collision
    }
}

// Actual (wrong)
// Uses 'v' in both loops, causes shadowing
```

**Effort**: 1-2 days (implement collision detection)

### 4. Comment Preservation (1000+ tests)
Comments in parenthesized expressions are moved:

```javascript
// Expected
yield (
    // comment
    a);

// Actual (wrong)
yield a;
// comment
```

**Effort**: 2-3 days (enhance AST comment tracking)

### 5. Extra Blank Lines (321 tests)
Minor formatting issue with nested namespaces - extra line before closing.

**Effort**: Few hours (formatting tweaks)

## Fixes Completed (5 commits)
1. ✅ Class/enum/function + namespace var declaration merging
2. ✅ Arrow function parenthesis preservation
3. ✅ Declaration-only constructors  
4. ✅ Emit test runner alwaysstrict variant parsing
5. ✅ Nested exported namespace IIFE parameters

## Path to 80%

**Option A: Full Implementation** (~1-2 weeks)
- Implement downlevel helper system
- Fix export ordering
- Fix temp variable naming
- Result: 80-90% pass rate

**Option B: Incremental** (ongoing)
- Continue fixing smaller patterns
- Leave helpers for later
- Ceiling: ~55-60% pass rate

**Option C: Hybrid** (~3-5 days)
- Implement 2-3 most common helpers (__generator, __awaiter)
- Fix export ordering
- Result: ~65-75% pass rate
