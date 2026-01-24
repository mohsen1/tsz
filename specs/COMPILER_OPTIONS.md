# Compiler Options Support

This document describes the compiler options supported by the WASM API.

## Overview

The WASM API provides a `setCompilerOptions` method on the `Parser` class that accepts compiler options in JSON format. These options control type checking behavior.

## Supported Options

### Strict Mode Options

#### `strict` (boolean, default: false)
Enable all strict type checking options. When set to `true`, it enables:
- `noImplicitAny`
- `strictNullChecks`
- `strictFunctionTypes`
- `strictPropertyInitialization`
- `noImplicitThis`

Individual options can override the `strict` setting.

#### `noImplicitAny` (boolean, default: false)
Raise error on expressions and declarations with an implied 'any' type. Implied by `strict: true`.

**Example:**
```typescript
// Error with noImplicitAny: true
function test(x) {  // Parameter 'x' implicitly has an 'any' type
    return x + 1;
}
```

#### `strictNullChecks` (boolean, default: false)
Enable strict null checks. When enabled, `null` and `undefined` are not assignable to other types. Implied by `strict: true`.

**Example:**
```typescript
// Error with strictNullChecks: true
let x: string | null = null;
let y: string = x;  // Type 'null' is not assignable to type 'string'
```

#### `strictFunctionTypes` (boolean, default: false)
Enable strict checking of function types (contravariant parameter checking). Implied by `strict: true`.

**Example:**
```typescript
// Error with strictFunctionTypes: true
type Handler = (x: string | number) => void;
const handler: Handler = (x: string) => {};  // Types of parameters are incompatible
```

#### `strictPropertyInitialization` (boolean, default: false)
Ensure class properties are initialized in the constructor or have a definite assignment. Implied by `strict: true`.

**Example:**
```typescript
// Error with strictPropertyInitialization: true
class C {
    x: number;  // Property 'x' has no initializer and is not definitely assigned in the constructor
}
```

#### `noImplicitThis` (boolean, default: false)
Raise error on 'this' expressions with an implied 'any' type. Implied by `strict: true`.

**Example:**
```typescript
// Error with noImplicitThis: true
function test() {
    this.x = 1;  // 'this' implicitly has type 'any'
}
```

#### `useUnknownInCatchVariables` (boolean, default: false)
Default catch clause variables to `unknown` instead of `any`. Implied by `strict: true`.

#### `strictBindCallApply` (boolean, default: false)
Enable strict checking of `bind`, `call`, and `apply` methods on functions. Implied by `strict: true`.


### Other Options

#### `noImplicitReturns` (boolean, default: false)
Report error when not all code paths in a function return a value.

**Example:**
```typescript
// Error with noImplicitReturns: true
function test(x: number): number {
    if (x > 0) {
        return x;
    }
    // Not all code paths return a value
}
```

#### `target` (string, default: "es3")
Specify ECMAScript target version. Supported values:
- `"es3"`
- `"es5"`
- `"es6"` or `"es2015"`
- `"es2016"`
- `"es2017"`
- `"es2018"`
- `"es2019"`
- `"es2020"`
- `"es2021"`
- `"es2022"`
- `"esnext"`

#### `module` (string, default: "none")
Specify module code generation. Supported values:
- `"none"`
- `"commonjs"`
- `"amd"`
- `"umd"`
- `"system"`
- `"es6"` or `"es2015"`
- `"es2020"`
- `"es2022"`
- `"esnext"`
- `"node16"`
- `"nodenext"`

#### `noLib` (boolean, default: false)
Do not include the default library file (`lib.d.ts`). When enabled, the compiler will not automatically load standard types like `Array`, `String`, etc., unless provided explicitly.

#### `exactOptionalPropertyTypes` (boolean, default: false)
Enable stricter checking of optional properties. When enabled, optional properties must either be present with the specified type or be missing entirely; they cannot be explicitly assigned `undefined` unless `undefined` is part of the property's union type.

#### `isolatedModules` (boolean, default: false)
Ensure that each file can be safely transpiled without relying on other imports.

#### `noUncheckedIndexedAccess` (boolean, default: false)
Include `undefined` in the type of any property accessed via an index signature.

## Usage

### JavaScript/TypeScript

```javascript
import { Parser } from 'tsc-clone';

const parser = new Parser('file.ts', sourceCode);

// Set compiler options
parser.setCompilerOptions(JSON.stringify({
    strict: true,
    noImplicitReturns: true,
    target: "es5",
    module: "commonjs"
}));

// Parse and check
parser.parseSourceFile();
const result = JSON.parse(parser.checkSourceFile());

console.log('Diagnostics:', result.diagnostics);
```

### Setting Individual Options

```javascript
// Enable only specific strict checks
parser.setCompilerOptions(JSON.stringify({
    noImplicitAny: true,
    strictNullChecks: true,
    strictFunctionTypes: false  // Override strict default
}));
```

### Using with Default Options

If `setCompilerOptions` is not called, all options default to `false` (non-strict mode).

## Implementation Details

### Option Resolution

Options are resolved in the following priority:
1. Individual option value (if set)
2. `strict` mode value (for strict-related options)
3. Default value (false)

For example:
```javascript
{
    strict: true,
    noImplicitAny: false  // Explicitly disabled, overrides strict
}
// Result: noImplicitAny = false, other strict options = true
```

### CheckerContext Integration

The compiler options are passed to `CheckerContext` through the following flow:

1. `Parser.setCompilerOptions()` parses JSON and stores `CompilerOptions` struct
2. When type checking, options are extracted using getter methods
3. Options are passed to `CheckerState::with_options()`
4. `CheckerContext` stores the individual option flags
5. Type checker uses these flags during analysis

### Cache Invalidation

Setting compiler options invalidates the type cache to ensure fresh type checking with new settings.

## Testing

To test compiler options:

```javascript
const source = `
function test(x) {
    return x + 1;
}
`;

// Test with strict mode off
const parser1 = new Parser('test.ts', source);
parser1.setCompilerOptions(JSON.stringify({ strict: false }));
parser1.parseSourceFile();
const result1 = JSON.parse(parser1.checkSourceFile());
// Should have 0 diagnostics

// Test with strict mode on
const parser2 = new Parser('test.ts', source);
parser2.setCompilerOptions(JSON.stringify({ strict: true }));
parser2.parseSourceFile();
const result2 = JSON.parse(parser2.checkSourceFile());
// Should have 1+ diagnostics (noImplicitAny error)
```

## Future Enhancements

Potential future compiler options to support:
- `noUnusedLocals`
- `noUnusedParameters`
- `noFallthroughCasesInSwitch`
- `alwaysStrict`
