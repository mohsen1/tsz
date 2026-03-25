# Roadmap to 100% Conformance & Emit

*Generated 2026-03-14 from deep analysis of 1000 commits and all failure data*

## Current State

| Metric | Current | Remaining |
|---|---|---|
| Conformance | 85.8% (10,789/12,581) | 1,792 tests |
| Emit JS | 83.9% (11,349/13,526) | 2,177 tests |
| Emit DTS | 69.8% (1,159/1,660) | 501 tests |

**Trajectory**: ~0.6 tests/commit over 850 commits. At current rate: ~3,000 commits to 100%.

## Conformance: 1,792 Failures

### Failure Categories

| Category | Count | % |
|---|---|---|
| Fingerprint-only (correct codes, wrong pos/msg) | 620 | 35% |
| Both missing AND extra codes | 948 | 53% |
| False positives (emit errors tsc doesn't) | 188 | 11% |
| All missing (emit nothing) | 280 | 16% |

### 12 Root-Cause Pillars

#### Pillar 1: Diagnostic Message & Position Accuracy (620 tests)
- **1A**: Flow-narrowed type not used in error messages (~30-40 tests). `assignability.rs` uses declared type, not CFA-narrowed type.
- **1B**: Uninstantiated type params in display (~15-20 tests). `format.rs` shows `A<T>` not `A<number>`.
- **1C**: Multiple-instance error emission (~20-30 tests). TS2420/TS2322 de-duplicated too aggressively.
- **1D**: Wrong anchor node for call argument errors (~10 tests).

#### Pillar 2: Generic Type Inference Engine (~100+ tests)
Two parallel inference engines (`infer_from_types` and `constrain_types`) with divergent capabilities:
- **2A**: `infer_from_types` never expands `Lazy` types — zero candidates for interfaces/classes
- **2B**: No tuple rest element alignment in `infer_tuples`
- **2C**: Callable inference requires exact arity match
- **2D**: Mapped type inference only works on `Object`, not `Lazy`/`ObjectWithIndex`
- **2E**: No `Conditional` match arm in `infer_from_types`
- **2F**: Variance defaults to covariant for all `TypeApplication` args
- **2G**: Reverse-mapped inference aborts on conditional templates

#### Pillar 3: Contextual Typing Pipeline (348 tests)
- **3A**: Array literal contextual narrowing — `Record | Array` union gives `string | number`
- **3B**: Async `PromiseLike<T> | T` branch context
- **3C**: `T & {}` anti-inference markers not propagated
- **3D**: Generator union return type — non-generator members not filtered
- **3E**: `satisfies` operator missing contextual propagation

#### Pillar 4: Parser Recovery (114 tests)
- **4A**: `ERROR_SUPPRESSION_DISTANCE = 3` vs tsc's exact-position dedup. Experimentally verified: reducing to 0 gives +22/-16 (net +6) — needs per-site handling, not a global change.
- **4B**: Missing TS17019/TS17020 for `?type` / `type?` patterns
- **4C**: TS1127 cascade containment

#### Pillar 5: JSX Type Checking (128 tests)
- **5A**: `JSX.LibraryManagedAttributes` entirely absent (~8-10 tests)
- **5B**: Generic JSX inference intentionally skipped (~10-15 tests)
- **5C**: `JSX.ElementType` not implemented (7 false positives)

#### Pillar 6: JSDoc & Salsa (159 tests)
- **6A**: `function(this: T, ...)` → `this_type` not set
- **6B**: `@template const/in/out/private` modifiers dropped
- **6C**: `Ns.Func.prototype = {...}` only handles `SimpleIdent.prototype`
- **6D**: `module.exports = primitive` + augmentation not validated
- **6E**: `@typedef` duplicate name not detected (TS2300)
- **6F**: Plain JS binder errors (TS1101/TS8009/TS8012) unimplemented

#### Pillar 7: Module Augmentation & Multi-File (80+ tests)
- **7A**: Augmentation key mismatch (file paths vs specifiers)
- **7B**: `global {}` inside `declare module` not processed
- **7C**: Cross-arena augmentation members return `TypeId::ANY`
- **7D**: TS2686 (UMD global access) entirely unimplemented
- **7E**: Package identity deduplication absent

#### Pillar 8: Control-Flow Narrowing (430 tests impacted)
- **8A**: Evolving array type TS7005/TS7034 not emitted
- **8B**: `in` operator TS2638 validation missing
- **8C**: Assertion function validation (TS2775/TS2776) missing
- **8D**: Correlated union narrowing fails for generic type params
- **8E**: Narrowing to `never` from chained ||/ternary incomplete

#### Pillar 9: TS2322 False Positives (60 tests)
- **9A**: Module package deduplication (8 tests)
- **9B**: Reverse-mapped/variance probing (8 tests)
- **9C**: Generic call return-type inference (9 tests)
- **9D**: Generator contextual return against union (4 tests)

#### Pillar 10: Declaration Emit (501 DTS tests)
- **10A**: TypePrinter falls back to `any`/`{}` for complex inferred types (~50-100 tests)
- **10B**: `strictNullChecks` not plumbed to DTS emitter (~30-50 tests)
- **10C**: `import("path").Type` synthesis missing (~20-40 tests)

#### Pillar 11: JS Emit (2,177 tests)
- Comment preservation: 811 tests
- Async/generator helpers: 167 tests
- Decorator metadata: 164 tests
- System module format: 113 tests
- Import/export helpers: 62 tests

#### Pillar 12: Missing Diagnostic Codes (~50 codes never emitted)
TS2686, TS2883, TS8030, TS2498, TS2775, TS2776, TS7005, TS7034, etc.

## Strategy

### Phase 1: Diagnostic Accuracy (target: +200 tests)
Fix flow-narrowed types in messages, instantiated type params, multiple-instance emission.

### Phase 2: Inference & Contextual (target: +200 tests)
Unify inference engines, add tuple rest, fix callable arity, expand Lazy types.

### Phase 3: JSDoc + JSX + Salsa semantic integration (target: +250 tests)
LibraryManagedAttributes, generic JSX, parser recovery, typedef cross-module resolution,
property/write semantics, and JS open-world module behavior.

### Phase 4: Declaration Emit (target: +300 tests)
TypePrinter accuracy, strictNullChecks plumbing, import type synthesis.

### Phase 5: Long Tail (target: remaining ~700 tests)
Individual diagnostic codes, narrowing edge cases, JS emit transforms.

## Key Architectural Insight

The **single highest-leverage change** is unifying the two inference engines. The **single highest-volume fix** is diagnostic fingerprint accuracy (620 tests = 35% of failures, no new type system features needed).
