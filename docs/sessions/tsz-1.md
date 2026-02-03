# Session tsz-1

## Current Work

**Investigating parser/binder/emitter conformance issues** - Analyzing top error mismatches from 500 test sample.

---

## Findings

Top error mismatches:
1. **TS2705** (missing=105): ES5 async functions require Promise - lib context handling
2. **TS1109/TS1055/TS1359** (missing=43): Parse errors - parser issues
3. **TS2304/TS2585** (missing=20): Cannot find name - binder symbol resolution
4. **TS2664** (missing=11): Module not found - module resolution
5. **TS2654** (extra=6): Multiple default exports - false positive
6. **TS2524/TS2515** (missing=13): Abstract class issues - checker

### Discovered Issues
1. **TS1040 false positive**: `async function f() {}` inside namespace flagged as "ambient context" error - parser/binder issue
2. **Multi-file test handling**: Ambient module merging works when files set up correctly, but conformance runner may have issues

---

## Next Steps
1. Investigate TS2705 - lib context handling for ES5 async/Promise
2. Fix parse errors (TS1109/TS1055/TS1359)
3. Fix symbol resolution issues (TS2304/TS2585)

---

## History (Last 20)

*No work history yet*

---

## Punted Todos

*No punted items*
