# JS Emit Conformance Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Improve JS emit-related conformance pass rate by fixing diagnostic issues in emit-adjacent code paths (destructuring, classes, generators, async) AND build emit baseline comparison infrastructure.

**Architecture:** The conformance suite currently only tests diagnostics (`--noEmit`). We add a parallel emit-comparison mode that diffs our JS output against tsc's 13,805 `.js` baselines. For diagnostic fixes, we target the 15 failing destructuring tests and 3 failing class tests since these have the highest density of fixable issues.

**Tech Stack:** Rust (conformance runner, checker, emitter), Python (analysis scripts), TypeScript baselines

**Key Finding:** The conformance suite ALWAYS passes `--noEmit` to tsz. The 93.5% pass rate measures diagnostic parity only. There is NO automated testing of JS output correctness.

---

### Task 1: Fix TS1312 missing in shorthandPropertyAssignmentsInDestructuring_ES6

This test expects TS1312 ("Did you mean to use a ':'?") but we emit TS18004 instead.

**Files:**
- Modify: `crates/tsz-checker/src/checkers/` (find TS18004 emission for shorthand property)
- Test: `./scripts/conformance/conformance.sh run --filter "shorthandPropertyAssignmentsInDestructuring_ES6" --verbose`

- [ ] **Step 1: Run the failing test verbose to see exact mismatch**
```bash
./scripts/conformance/conformance.sh run --filter "shorthandPropertyAssignmentsInDestructuring_ES6" --verbose
```

- [ ] **Step 2: Check tsc expectation**
```bash
python3 -c "
import json
with open('scripts/conformance/tsc-cache-full.json') as f:
    cache = json.load(f)
for key in cache:
    if 'shorthandPropertyAssignmentsInDestructuring_ES6' in key:
        for fp in cache[key].get('diagnostic_fingerprints', []):
            print(f'  TS{fp[\"code\"]} L{fp[\"line\"]}:{fp[\"column\"]} {fp[\"message_key\"][:80]}')
"
```

- [ ] **Step 3: Find where TS18004 is emitted and understand why TS1312 should be emitted instead**
```bash
grep -rn "18004\|TS18004" crates/tsz-checker/src/ | head -10
grep -rn "1312\|TS1312" crates/tsz-checker/src/ | head -10
```

- [ ] **Step 4: Implement the fix — emit TS1312 when shorthand property assignment uses `=` in destructuring context**

- [ ] **Step 5: Verify fix passes and run regression check**
```bash
./scripts/conformance/conformance.sh run --filter "shorthandPropertyAssignmentsInDestructuring_ES6" --verbose
./scripts/conformance/conformance.sh run --max 300
```

- [ ] **Step 6: Commit**
```bash
git add -A && git commit -m "fix(checker): emit TS1312 for shorthand property = in destructuring"
```

---

### Task 2: Fix TS2488 missing in destructuringAssignabilityCheck

We emit extra TS2322 where tsc expects TS2488 ("Type must have a '[Symbol.iterator]()' method").

**Files:**
- Modify: `crates/tsz-checker/src/assignability/` or `crates/tsz-checker/src/checkers/`
- Test: `./scripts/conformance/conformance.sh run --filter "destructuringAssignabilityCheck" --verbose`

- [ ] **Step 1: Run the failing test verbose**
- [ ] **Step 2: Check tsc expectation and compare**
- [ ] **Step 3: Find where we emit TS2322 instead of TS2488 for non-iterable destructuring**
- [ ] **Step 4: Implement fix**
- [ ] **Step 5: Verify and regression check**
- [ ] **Step 6: Commit**

---

### Task 3: Fix missing parser errors in destructuringParameterDeclaration6

Missing TS1109/TS1128/TS1181 — parser recovery issue.

**Files:**
- Modify: `crates/tsz-parser/src/`
- Test: `./scripts/conformance/conformance.sh run --filter "destructuringParameterDeclaration6" --verbose`

- [ ] **Step 1-6: Same pattern as above**

---

### Task 4: Build emit baseline comparison script

Create a Python script that compares our JS output against tsc's 13,805 `.js` baselines.

**Files:**
- Create: `scripts/conformance/emit-compare.py`
- Create: `scripts/conformance/emit-compare.sh` (wrapper)

- [ ] **Step 1: Create emit comparison script**

```python
#!/usr/bin/env python3
"""Compare tsz JS emit output against tsc baselines.

Usage:
  python3 scripts/conformance/emit-compare.py [--max N] [--filter PATTERN]
"""
import subprocess, os, sys, json, re, tempfile, argparse
from pathlib import Path

TSZ_BIN = ".target/dist-fast/tsz"
BASELINE_DIR = Path("TypeScript/tests/baselines/reference")
TEST_DIR = Path("TypeScript/tests/cases")

def parse_baseline(path):
    """Extract expected JS from a .js baseline file."""
    content = path.read_text()
    # Find //// [filename.js] section
    js_sections = {}
    current_file = None
    for line in content.split('\n'):
        if line.startswith('//// [') and line.endswith(']'):
            fname = line[6:-1]
            if fname.endswith('.js'):
                current_file = fname
                js_sections[current_file] = []
            else:
                current_file = None
        elif current_file is not None:
            js_sections[current_file].append(line)
    return {k: '\n'.join(v).strip() for k, v in js_sections.items()}

def run_tsz_emit(test_path, target="es2015"):
    """Run tsz on a test file and capture JS output."""
    with tempfile.TemporaryDirectory() as tmpdir:
        result = subprocess.run(
            [TSZ_BIN, str(test_path), "--outDir", tmpdir, "--target", target],
            capture_output=True, text=True, timeout=30
        )
        js_files = {}
        for root, dirs, files in os.walk(tmpdir):
            for f in files:
                if f.endswith('.js'):
                    full = os.path.join(root, f)
                    js_files[f] = open(full).read().strip()
        return js_files

# Main comparison logic would go here
```

- [ ] **Step 2: Run on a sample of 100 tests and measure match rate**
- [ ] **Step 3: Report results — what percentage of JS baselines match?**
- [ ] **Step 4: Commit**

---

### Task 5: Run full verification

- [ ] **Step 1: Run all unit tests**
```bash
scripts/safe-run.sh cargo nextest run 2>&1 | tail -5
```

- [ ] **Step 2: Run full conformance**
```bash
scripts/safe-run.sh ./scripts/conformance/conformance.sh run 2>&1 | grep "FINAL"
```

- [ ] **Step 3: Update snapshot if improved**
```bash
scripts/safe-run.sh ./scripts/conformance/conformance.sh snapshot
```

- [ ] **Step 4: Push the branch and open a pull request**
```bash
# Push your working branch (NOT main) and open a PR targeting main.
git push -u origin "$(git rev-parse --abbrev-ref HEAD)"
```
Do not push directly to `main`; the integrator validates and merges the PR.
