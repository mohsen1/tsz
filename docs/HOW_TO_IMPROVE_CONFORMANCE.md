# How to improve Conformance

## Finding Low-Hanging Fruits from Conformance

### Step 1: Look at "Extra" Errors (False Positives)
These block valid code from compiling - highest user impact:

```bash
# Current top error mismatches
./scripts/conformance.sh run 2>&1 | grep -A15 "Top Error Code"
```

From our last run:
```
TS2339: missing=357, extra=621   # 621 false positives!
TS1005: missing=472, extra=283   # Was 972 before ASI fix
TS2322: missing=284, extra=426   # Type assignability false positives
TS2345: missing=84, extra=334    # Argument type false positives
```

### Step 2: Investigate High-Volume "Extra" Codes
```bash
# Pick the top "extra" error code and filter tests
./scripts/conformance.sh run --error-code 2339 --verbose 2>&1 | head -100
```

### Step 3: Find Patterns
Look for **repeated failures with same root cause**:
```bash
# Get a specific failing test and compare
./.target/release/tsz <test_file> --noEmit 2>&1
npx tsc <test_file> --noEmit 2>&1
```

### Step 4: Prioritize by Fix Complexity

| Error Range | Type | Typical Complexity |
|-------------|------|-------------------|
| **TS1xxx** | Parser | Often simple (1-line fixes like ASI) |
| **TS2304** | Symbol resolution | Medium |
| **TS2339** | Property access | Medium-Hard |
| **TS2322/2345** | Type compatibility | Hard (Solver/Lawyer) |

### Current Best Targets

Based on the last conformance run, here are the next low-hanging fruits:

```bash
# TS2339 has 621 extra errors - property access issues
./scripts/conformance.sh run --error-code 2339 --verbose --max 50 2>&1

# TS2345 has 334 extra errors - argument type issues  
./scripts/conformance.sh run --error-code 2345 --verbose --max 50 2>&1
```

