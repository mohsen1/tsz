# Conformance Test Runner Issue

## Problem
All 12,381 conformance tests crash with module resolution error:

```
Cannot find module '../../services/_namespaces/ts.js'
```

## Root Cause
The TypeScript submodule (at commit `7f6a84673`) has internal namespace files with broken relative imports. When `harness/_namespaces/ts.js` is loaded, it tries to require files like:
- `../corePublic.js`  
- `../core.js`
- `../services/_namespaces/ts.js` (missing!)

## Investigation
1. TypeScript build is at `TypeScript/built/local/`
2. Files exist: `compiler/corePublic.js`, `compiler/_namespaces/ts.js`
3. Missing: `compiler/services/_namespaces/ts.js`
4. The `ts.js` file has relative imports that assume a different build structure

## Possible Solutions
1. **Update TypeScript submodule** - Pull latest changes that may have fixed this
2. **Rebuild TypeScript** - Run full build with `npx hereby tests --no-bundle`  
3. **Use stable entry point** - Load from `typescript.js` instead of internal namespaces
4. **Fix module paths** - Update ts-harness-loader.ts to handle the new structure

## Status
- Conformance tests: **CRASHING** (0% pass rate)
- Unit tests: **PASSING** (7831/7831)
- Issue: Test harness infrastructure, not compiler code

## Files to Check
- `scripts/conformance/src/ts-harness-loader.ts`
- `TypeScript/built/local/compiler/_namespaces/ts.js`
- TypeScript submodule build process
