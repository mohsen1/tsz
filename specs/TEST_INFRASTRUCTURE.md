# Test Infrastructure

## CompilerOptions Flow

This document describes how compiler options flow through the system from test files to the type checker.

### Overview

The compiler is designed to be configuration-driven, not path-driven. Configuration flows explicitly through well-defined interfaces, ensuring that the checker's behavior is determined solely by the `CompilerOptions` object, not by inspecting file paths or other external state.

### Data Flow Diagram

```
Test File (*.ts)
    |
    | Contains directives in comments
    | Example: // @strict
    |
    v
Test Harness/Runner
    |
    | 1. Reads test file
    | 2. Parses comment directives
    |    - Scans for // @<directive>
    |    - Extracts configuration settings
    |
    v
Directive Parser
    |
    | Converts directives to structured config
    | Example: // @strict -> { strict: true }
    |
    v
CompilerOptions Object
    |
    | Structured configuration object
    | Contains all compiler settings
    |
    v
createProgram(files, options)
    |
    | Explicit API call with options
    | No file path inspection
    |
    v
Program Object
    |
    | Stores CompilerOptions internally
    | Provides getCompilerOptions() method
    |
    v
Checker (via program.getCompilerOptions())
    |
    | Receives configuration explicitly
    | Uses options to determine behavior
    | NO file path inspection
    | NO implicit configuration
    |
    v
Type Checking Behavior
```

### Step-by-Step Flow

#### 1. Test Infrastructure Reads Test Files

The test harness or runner reads TypeScript test files from the filesystem. These files contain both:
- TypeScript source code to be type-checked
- Configuration directives in comments

Example test file:
```typescript
// @strict
// @target ES2015
// @module commonjs

function greet(name: string) {
    console.log("Hello, " + name);
}
```

#### 2. Parse Directives from Comments

The test infrastructure scans the file for special comment directives that begin with `// @`. Each directive corresponds to a compiler option.

Common directives:
- `// @strict` - Enable strict mode
- `// @noImplicitAny` - Disallow implicit any types
- `// @target ES2015` - Set compilation target
- `// @module commonjs` - Set module system
- `// @lib ES2015,DOM` - Specify library files

The parser extracts these directives and their values:
```rust
// Pseudocode
let directives = parse_directives(file_content);
// Result: [
//   ("strict", true),
//   ("target", "ES2015"),
//   ("module", "commonjs")
// ]
```

#### 3. Convert Directives to CompilerOptions Object

The parsed directives are converted into a structured `CompilerOptions` object. This object contains all configuration settings in a typed, validated format.

```rust
// Pseudocode
let mut options = CompilerOptions::default();
for (directive, value) in directives {
    match directive {
        "strict" => options.strict = parse_bool(value),
        "target" => options.target = parse_target(value),
        "module" => options.module_kind = parse_module(value),
        // ... other options
    }
}
```

#### 4. Pass CompilerOptions Explicitly to Compiler API

The test infrastructure calls `createProgram()` or equivalent compiler API with the constructed `CompilerOptions` object. This is an explicit, type-safe API call.

```rust
// Pseudocode
let program = create_program(
    files: vec!["test.ts"],
    options: options,  // Explicitly passed
    host: host
);
```

Key principle: **Configuration is passed explicitly, never inferred from file paths or other implicit sources.**

#### 5. Checker Receives Options via program.getCompilerOptions()

When the type checker needs to determine its behavior, it queries the program object for the compiler options:

```rust
// Inside the checker
let options = self.program.get_compiler_options();

// Use options to determine behavior
if options.strict {
    // Enable strict type checking
}

if options.no_implicit_any {
    // Report errors for implicit any
}
```

The checker **never**:
- Inspects file paths to determine configuration
- Uses global configuration state
- Infers settings from the environment
- Makes assumptions based on file extensions or locations

#### 6. Checker Uses Options to Determine Behavior

The checker's behavior is entirely determined by the `CompilerOptions` object. Different options affect various aspects of type checking:

**Strictness Options:**
- `strict`: Enables all strict type checking options
- `noImplicitAny`: Forbids implicit any types
- `strictNullChecks`: Null and undefined are separate types
- `strictFunctionTypes`: Stricter function type checking

**Language Options:**
- `target`: ECMAScript version (affects available features)
- `lib`: Library type definitions to include

**Module Options:**
- `module`: Module code generation
- `moduleResolution`: How modules are resolved

**Example in the checker:**
```rust
fn check_function_call(&mut self, node: &CallExpression) -> Type {
    let options = self.program.get_compiler_options();

    // Behavior varies based on options
    if options.strict_function_types {
        // Use stricter contravariance rules
        self.check_call_strict(node)
    } else {
        // Use lenient bivariance rules
        self.check_call_lenient(node)
    }
}
```

### Anti-Patterns (What NOT to Do)

The following patterns are explicitly forbidden in our codebase:

#### DO NOT: Inspect File Paths
```rust
// WRONG - Never do this!
fn is_strict_mode(&self, file: &SourceFile) -> bool {
    file.path.contains("strict") || file.path.ends_with(".strict.ts")
}
```

#### DO NOT: Use Global Configuration
```rust
// WRONG - Never do this!
static mut GLOBAL_STRICT_MODE: bool = false;

fn check_with_global_config() {
    if unsafe { GLOBAL_STRICT_MODE } {
        // ...
    }
}
```

#### DO NOT: Infer Settings from Context
```rust
// WRONG - Never do this!
fn infer_module_kind(&self, file: &SourceFile) -> ModuleKind {
    if file.text.contains("import") {
        ModuleKind::ES6
    } else {
        ModuleKind::CommonJS
    }
}
```

### Correct Pattern

Always use the explicit CompilerOptions:

```rust
// CORRECT
fn get_module_kind(&self) -> ModuleKind {
    self.program.get_compiler_options().module_kind
}

fn is_strict_mode(&self) -> bool {
    self.program.get_compiler_options().strict
}

fn check_implicit_any(&self, node: &Node) -> bool {
    let options = self.program.get_compiler_options();
    options.no_implicit_any || options.strict
}
```

### Benefits of This Architecture

1. **Testability**: Each test can specify its own configuration without affecting others
2. **Predictability**: Behavior is explicit and traceable
3. **Maintainability**: Configuration logic is centralized
4. **Debuggability**: Configuration state is inspectable at any point
5. **Type Safety**: Options are validated at construction time
6. **Isolation**: Tests don't interfere with each other

### Implementation Notes

- The `CompilerOptions` object should be immutable after creation
- Default values should be explicitly specified, not assumed
- All option parsing should happen in the test infrastructure, not the checker
- The checker should treat the `CompilerOptions` as a read-only configuration source
- When adding new compiler options, update both the parser and the options struct

### Testing the Flow

When writing tests, you can verify the flow works correctly:

```typescript
// test-strict.ts
// @strict

let x;  // Should error: Variable 'x' implicitly has an 'any' type
```

```typescript
// test-lenient.ts
// No @strict directive

let x;  // Should NOT error in lenient mode
```

Both tests use the same checker code, but produce different results based on their explicit configuration.

### Summary

The key principle is: **Configuration flows explicitly from test files through the test infrastructure to the compiler API, and the checker receives it via well-defined interfaces. No file path inspection, no global state, no implicit configuration.**

This architecture ensures that our type checker is:
- Configuration-driven, not path-driven
- Testable in isolation
- Predictable in behavior
- Easy to reason about
