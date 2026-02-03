# Session tsz-1

## Current Work

**Debugging TS1040 false positive** - `export async function` inside regular namespace incorrectly flagged as ambient context.

---

## Bug Investigation

### Test Case
```typescript
namespace M {
    export async function f1() { }
}
```

**Expected**: No errors (TypeScript accepts this)
**Actual**: TS1040: 'async' modifier cannot be used in an ambient context

### Analysis

The parser sets `CONTEXT_FLAG_AMBIENT` only in `parse_module_block()` when `is_ambient=true`. For `namespace M` (no `declare` modifier), `is_ambient` should be `false`.

Code path:
1. `parse_module_declaration()` → `parse_module_declaration_with_modifiers(start_pos, None)`
2. `is_declare = modifiers.as_ref()...` → `None.as_ref()` → `None` → `false`
3. `parse_module_block(false)` called with `is_ambient = false`
4. Ambient flag should NOT be set
5. But error is still emitted

### Hypothesis
There may be:
- A bug in how `is_declare` is computed
- Another code path setting the ambient flag
- Context flags not being properly restored

---

## Findings (500 test sample)

Top error mismatches:
1. **TS2705** (missing=105): ES5 async functions require Promise
2. **TS1109/TS1055/TS1359** (missing=43): Parse errors
3. **TS2304/TS2585** (missing=20): Cannot find name
4. **TS2664** (missing=11): Module not found
5. **TS2654** (extra=6): Multiple default exports
6. **TS2524/TS2515** (missing=13): Abstract class issues

---

## Next Steps
1. Fix TS1040 false positive (debug context flag handling)
2. Investigate TS2705 - lib context for ES5 async/Promise
3. Fix parse errors (TS1109/TS1055/TS1359)

---

## History (Last 20)

*No work history yet*

---

## Punted Todos

*No punted items*
