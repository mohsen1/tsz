stuff like this is not good. we should not care about file name 

```
.and_then(|s| s.strip_prefix("lib."))
                .and_then(|s| s.strip_suffix(".generated"))

```


---

file name extension checks should be unified. there are so many crazy extensions like mjs etc

---

A future stateless `SubtypeChecker` rewrite? 
To benefit from Salsa's cycle detection (automatic coinductive cycle recovery)

---

## TS2318 Investigation (Feb 2026)

**Issue**: tsz missing ~700 TS2318 "Cannot find global type" errors.

**Root cause** (deeper than initially thought):

When `--lib es6` is explicitly specified:
- **TSC**: Sets `compilerOptions.lib = ['lib.es2015.d.ts']` and checks for core types ONLY in these specified libs (ignores `/// <reference>` chain for this check)
- **tsz**: Follows `/// <reference lib="es5" />` in es2015.d.ts and loads es5.d.ts too

So tsz finds Array/Boolean/etc in es5.d.ts (loaded via reference) and doesn't emit TS2318.
But TSC considers only the explicitly specified libs for the core type existence check.

**The real fix** requires:
1. Track which libs were EXPLICITLY specified via `--lib` vs loaded via `/// <reference>`
2. For TS2318 core type checking, only consider explicitly specified libs
3. This is a semantic difference in lib handling between tsz and TSC

**Complexity**: Medium-high. Requires changes to:
- `CheckOptions` to carry explicit lib list
- `CheckerContext` to track explicit vs referenced libs  
- `check_missing_global_types` to respect this distinction

**Impact**: ~700 missing TS2318 errors → could improve conformance by ~5%
