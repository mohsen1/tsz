# Fingerprint Type Printer Fixes Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix 6 confirmed type-printer bugs causing ~624 fingerprint-only conformance failures where error codes match tsc but diagnostic messages/positions differ.

**Architecture:** All type formatting lives in the solver's `TypeFormatter` (`crates/tsz-solver/src/diagnostics/format.rs`). The checker's error reporter calls `format_type_diagnostic()` which delegates to `TypeFormatter::format()`. Fixes must stay in the solver (WHAT) layer per the North Star architecture — the checker (WHERE) only orchestrates. The exception is `normalize_assignability_display_type` in the checker's error_reporter which pre-processes types before formatting.

**Tech Stack:** Rust, tsz-solver crate, conformance test runner

---

## File Structure

| File | Responsibility |
|---|---|
| `crates/tsz-solver/src/diagnostics/format.rs` | Central type-to-string formatter (Bug 4: mapped semicolons) |
| `crates/tsz-checker/src/error_reporter/core.rs` | Type display normalization for assignability messages |
| `crates/tsz-checker/src/error_reporter/assignability.rs` | TS2322/TS2345 diagnostic assembly |

---

## Chunk 1: Mapped Type Semicolons (Bug 4)

### Task 1: Add trailing semicolon to mapped type format

The simplest, most isolated fix. `format_mapped` on line 1256 of `format.rs` omits the trailing semicolon that tsc includes.

**Files:**
- Modify: `crates/tsz-solver/src/diagnostics/format.rs:1255-1259` (mapped type format string)
- Modify: `crates/tsz-solver/src/diagnostics/format.rs:2553` (existing test assertion)

- [ ] **Step 1: Update the existing test to expect the semicolon**

In `format.rs`, test `format_mapped_preserves_key_dependent_template` (line 2553):
```rust
// Change from:
assert_eq!(fmt.format(mapped), "{ [P in string]: P }");
// To:
assert_eq!(fmt.format(mapped), "{ [P in string]: P; }");
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p tsz-solver format_mapped_preserves_key_dependent_template -- --nocapture`
Expected: FAIL — actual output is `{ [P in string]: P }` (no semicolon)

- [ ] **Step 3: Fix the format string in `format_mapped`**

In `format.rs` line 1255-1259, change:
```rust
// From:
format!(
    "{{ {readonly_prefix}[{param_name} in {}]{optional_suffix}: {} }}",
    self.format(mapped.constraint),
    self.format(mapped.template)
)
// To:
format!(
    "{{ {readonly_prefix}[{param_name} in {}]{optional_suffix}: {}; }}",
    self.format(mapped.constraint),
    self.format(mapped.template)
)
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p tsz-solver format_mapped -- --nocapture`
Expected: ALL mapped type tests pass. Also verify `format_mapped_type_with_remove_optional` and `format_mapped_type_with_remove_readonly` still pass (they use `contains()` assertions).

- [ ] **Step 5: Run targeted conformance test**

Run: `./scripts/conformance/conformance.sh run --filter "mappedTypeErrors" --verbose`
Expected: `mappedTypeErrors.ts` and `mappedTypeErrors2.ts` should show fewer fingerprint mismatches for the semicolon issue.

- [ ] **Step 6: Commit**

```bash
git add crates/tsz-solver/src/diagnostics/format.rs
git commit -m "fix(solver): add trailing semicolon to mapped type display to match tsc"
```

---

## Chunk 2: Literal Type Widening in Object Literals (Bug 1 — partial)

The verbose tests showed:
- tsc: `Type '{ fooProp: "frizzlebizzle"; } & Bar'`
- tsz: `Type '{ fooProp: string; } & Bar'`

This happens because the ObjectShape stores property types as widened types (e.g., `string` instead of `"frizzlebizzle"`). When creating object literal types during checking, the property types may get widened before being interned. This is a semantic issue in how object literal types are constructed, not a formatting issue.

Investigation required before fixing: Trace where the object literal type is constructed for `errorMessagesIntersectionTypes02.ts` to determine if the widening happens at:
1. Type creation time (binder/checker constructing the object type)
2. Type evaluation time (`normalize_assignability_display_type` calling `evaluate_type`)
3. Type interning (dedup matching a wider type)

### Task 2: Investigate literal widening root cause

**Files:**
- Read: `crates/tsz-checker/src/error_reporter/core.rs:196-206` (normalize_assignability_display_type)

- [ ] **Step 1: Run tracing on the specific test**

Run: `./scripts/conformance/conformance.sh run --filter "errorMessagesIntersectionTypes02" --verbose`
Examine the output to see the actual vs expected fingerprint.

- [ ] **Step 2: Add targeted tracing to `format_type_for_assignability_message`**

In `core.rs` around line 1618, temporarily add:
```rust
let display_ty = self.normalize_assignability_display_type(ty);
// Temporary trace for debugging literal widening
tracing::debug!(
    original = %self.format_type_diagnostic(ty),
    normalized = %self.format_type_diagnostic(display_ty),
    "normalize_assignability_display_type"
);
```

- [ ] **Step 3: Run the test with tracing enabled to identify the widening point**

Run with `RUST_LOG=debug`: determine if `ty` already has `string` or if normalization widens it.

- [ ] **Step 4: Based on findings, implement the fix**

If the issue is in `evaluate_type` widening literals — the fix may need to skip evaluation for object literal types that contain literal properties. Or the fix may need to be in object type construction to preserve literal types in the ObjectShape.

- [ ] **Step 5: Remove tracing and commit**

---

## Chunk 3: Boolean Not Narrowed to False in Type Guards (Bug 2)

The verbose tests showed in `typeGuardOfFormIsType.ts`:
- tsc: `Type 'string | false' is not assignable to type 'string'`
- tsz: `Type 'string | boolean' is not assignable to type 'string'`

This is a narrowing issue in the checker/solver. When a type guard checks `if (x is Foo)`, the else branch should narrow `boolean` to `false` (removing `true` from the union). tsz is not doing this narrowing.

This is a deeper solver fix — the narrowing logic needs to understand that `boolean = true | false` and removing the `true` branch leaves `false`.

### Task 3: Investigate boolean narrowing in type guard else branches

**Files:**
- Read: `crates/tsz-checker/src/flow/` (flow analysis)
- Read: `crates/tsz-solver/src/operations/` (narrowing operations)

- [ ] **Step 1: Run the specific test**

Run: `./scripts/conformance/conformance.sh run --filter "typeGuardOfFormIsType" --verbose`
Confirm the mismatch pattern.

- [ ] **Step 2: Find where type guard narrowing happens**

Search for `narrow` in the solver and checker flow analysis to understand the narrowing path for type predicates (`x is Foo`). The issue is specifically in the **else** branch — the negation of the type predicate.

- [ ] **Step 3: Identify the specific narrowing gap**

When narrowing `string | boolean` by removing `string` (type guard says `x is string`), tsc narrows:
- True branch: `string`
- False branch: `false` (not `boolean`)

tsz likely produces `boolean` in the false branch because it doesn't decompose `boolean` into `true | false` before subtracting `string`.

- [ ] **Step 4: Implement fix in solver narrowing**

The solver's type subtraction/narrowing should recognize that `boolean` is `true | false` when computing the else branch of a type guard. This is a solver-level fix.

- [ ] **Step 5: Run tests and commit**

---

## Chunk 4: Array Display Shorthand (Bug 3 — verify needed)

The verbose tests showed `Array<Base>` vs `Base[]`. However, looking at the formatter code, `format.rs` line 361-376 already uses `T[]` syntax. This suggests the issue may be in how the type is represented — possibly as `Application(Lazy(Array_DefId), [Base])` rather than `TypeData::Array(Base)`.

### Task 4: Verify and fix array display

- [ ] **Step 1: Run the specific test**

Run: `./scripts/conformance/conformance.sh run --filter "assignmentCompatWithCallSignatures3" --verbose`
Confirm whether `Array<T>` appears in the output.

- [ ] **Step 2: If confirmed, trace the type representation**

The issue would be in `TypeData::Application` formatting (line 394-438). When the base is `Lazy(Array_DefId)`, the formatter produces `Array<Base>` because it resolves the DefId name "Array" and appends `<args>`. The fix: check if the Application's base resolves to the built-in Array type and format as `T[]` instead.

- [ ] **Step 3: Implement fix**

In `format_key`, in the `TypeData::Application` arm (around line 406-438), add a special case:
```rust
// If this is Array<T>, format as T[] instead of Array<T>
if app.args.len() == 1 {
    if let Some(TypeData::Lazy(def_id)) = base_key {
        if let Some(def_store) = self.def_store {
            if let Some(def) = def_store.get(def_id) {
                let name = self.format_def_name(&def);
                if name == "Array" {
                    let elem = self.format(app.args[0]);
                    // Parenthesize complex element types
                    let needs_parens = matches!(
                        self.interner.lookup(app.args[0]),
                        Some(TypeData::Union(_) | TypeData::Intersection(_) | TypeData::Function(_) | TypeData::Callable(_))
                    );
                    return if needs_parens {
                        format!("({elem})[]").into()
                    } else {
                        format!("{elem}[]").into()
                    };
                }
            }
        }
    }
}
```

- [ ] **Step 4: Add test and verify**

- [ ] **Step 5: Run conformance and commit**

---

## Chunk 5: Excess Property Check Alias vs Constituent (Bug 5)

The verbose test showed in `excessPropertyCheckWithUnions.ts`:
- tsc: `...does not exist in type '{ tag: "A"; a1: string; }'`
- tsz: `...does not exist in type 'ADT'`

This is an error reporter issue — when reporting excess properties against a discriminated union, tsc reports against the **specific matching constituent** while tsz reports against the **whole union alias**.

### Task 5: Investigate excess property error target type

- [ ] **Step 1: Find excess property error reporting**

Search in checker error reporter for TS2353 (excess property) diagnostic creation to understand what type gets passed as the target.

- [ ] **Step 2: Determine if the target type needs to be narrowed**

When a discriminated union excess property check fails, the checker should pass the specific union member that matched (not the whole union) as the target type for the error message.

- [ ] **Step 3: Implement fix in checker excess property error path**

- [ ] **Step 4: Run conformance and commit**

---

## Chunk 6: Conditional Type Alias Resolution (Bug 6)

The verbose tests showed multiple cases where tsz resolves conditional/mapped type aliases to wrong depths. This is the most complex issue and likely requires changes to the `normalize_assignability_display_type` logic in `core.rs`.

Examples:
- `Type 'NonFunctionProperties<T>'` → tsz shows `Type 'T'` (over-evaluates)
- `Type 'DeepReadonlyArray<Part>'` → tsz shows `Type 'DeepReadonlyArray'` (drops generic params)
- `Type 'T[keyof T] | undefined'` → tsz shows `Type 'Partial<T>[keyof T]'` (wrong alias resolution)

### Task 6: Investigate conditional type display

- [ ] **Step 1: Run the specific test**

Run: `./scripts/conformance/conformance.sh run --filter "conditionalTypes1" --verbose`

- [ ] **Step 2: Analyze the evaluation chain**

The `normalize_assignability_display_type` function (line 196-250 in core.rs) calls `evaluate_type`. For generic conditional types, evaluation may resolve the type too aggressively. The fix may need to preserve the unevaluated form when the type contains unresolved type parameters.

- [ ] **Step 3: Implement fix**

Likely: in `normalize_assignability_display_type`, skip evaluation when the type is a named Application with type parameters that haven't been instantiated.

- [ ] **Step 4: Run conformance and commit**

---

## Validation

After all fixes are applied:

- [ ] **Run full conformance suite to measure improvement**

```bash
scripts/safe-run.sh ./scripts/conformance/conformance.sh snapshot
```

Compare the `fingerprint_only` count against the baseline (624).

- [ ] **Push to main**

```bash
git push origin HEAD:main
```
