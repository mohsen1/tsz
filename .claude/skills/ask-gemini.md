# Ask Gemini Skill

Consult Gemini AI with full codebase context for complex questions about the tsz TypeScript compiler.

## When to Use

Use this skill when you need deep understanding of the codebase architecture, implementation details, or complex reasoning that benefits from AI assistance with full repository context.

**Trigger keywords:**
- "ask gemini", "consult gemini", "use gemini"
- "complex analysis", "deep dive", "architecture question"
- "how does", "why does", "explain the architecture"
- "need more context", "full codebase analysis"

## Available Presets

The `ask-gemini.mjs` script supports focused presets for different areas:

| Preset | Focus Area | Example Questions |
|--------|-----------|-------------------|
| `--solver` | Type solver, inference, compatibility | "How does type inference work for conditional types?" |
| `--checker` | Type checker, diagnostics, AST traversal | "How are control flow analysis errors reported?" |
| `--binder` | Symbol binding, scopes, CFG | "How does the binder construct the control flow graph?" |
| `--parser` | Parser, scanner, AST nodes | "How does automatic semicolon insertion work?" |
| `--emitter` | Code emission, transforms, source maps | "How are ES5 transforms applied?" |
| `--lsp` | Language server protocol | "How does go-to-definition resolve symbols?" |
| `--types` | Type system overview | "How do generics and type parameters interact?" |
| `--modules` | Module resolution, imports, exports | "How are CommonJS modules resolved?" |

## Usage

```bash
# With preset (recommended for best results)
./scripts/ask-gemini.mjs --solver "How does generic instantiation work?"
./scripts/ask-gemini.mjs --checker "How are diagnostics reported?"
./scripts/ask-gemini.mjs --types "Explain the type inference algorithm"

# With custom paths
./scripts/ask-gemini.mjs --include="src/solver" "Custom question about solver"

# Dry run to see what files would be included
./scripts/ask-gemini.mjs --solver --dry

# List all available presets
./scripts/ask-gemini.mjs --list
```

## How It Works

The script:
1. **Builds a complete file tree** so Gemini knows all files in the repository
2. **Gathers relevant file contents** using `yek` with token limits
3. **Filters out submodule files** (TypeScript reference implementation)
4. **Sends to Gemini** with a system prompt tailored to the preset area
5. **Returns the response** with file references and line numbers

Each preset includes:
- **Focused paths** - Most relevant directories and files
- **Core files** - Key files marked with ★ in output
- **Optimized token limits** - Balances context depth and performance
- **Specialized system prompt** - Domain-specific guidance for Gemini

## System Prompts by Preset

Each preset includes specialized context:

### `--solver`
```
You are focused on the TYPE SOLVER component. Key concepts:
- Solver handles WHAT (pure type operations and relations)
- Checker handles WHERE (AST traversal, diagnostics)
- Use visitor pattern from src/solver/visitor.rs for type operations
- Key files: state.rs (main state), infer.rs (inference), compat.rs (assignability)
```

### `--checker`
```
You are focused on the TYPE CHECKER component. Key concepts:
- Checker is a thin wrapper that delegates type logic to solver
- Checker extracts AST data, calls solver, reports errors with source locations
- Control flow analysis lives in checker
- Key files: state.rs (main state), control_flow.rs, error_reporter.rs
```

### Other presets
See `scripts/ask-gemini.mjs` lines 354-405 for complete system prompt definitions.

## Environment Setup

Required environment variable:
```bash
export GEMINI_API_KEY="your-api-key-here"
```

Get an API key at: https://aistudio.google.com/apikey

## Best Practices

1. **Choose the right preset** - Match your question to the most relevant component
2. **Be specific** - Detailed questions get better answers
3. **Use dry run first** - Check what files are included before sending to API
4. **Reference AGENTS.md** - The script includes this file in context for architecture rules
5. **Follow up on file requests** - If Gemini lists files it needs, run again with `--include`

## Example Workflows

### Understanding Type Inference
```bash
./scripts/ask-gemini.mjs --solver "How does the solver infer the return type of arrow functions with contextual typing?"
```

### Debugging a Diagnostic Issue
```bash
./scripts/ask-gemini.mjs --checker "When should TS2322 be reported for object literal excess property checks?"
```

### Learning Module Resolution
```bash
./scripts/ask-gemini.mjs --modules "How does node16 module resolution differ from nodenext?"
```

### Custom Analysis
```bash
# Focus on specific files
./scripts/ask-gemini.mjs --include="src/solver/state.rs src/solver/infer.rs" "How are inference variables unified?"
```

## Integration with Development Workflow

This tool complements other documentation:
- **AGENTS.md** - Architecture rules and principles
- **docs/solver-type-computation-analysis.md** - Solver architecture deep dive
- **docs/TYPE_VISITOR_PATTERN_GUIDE.md** - Type operation patterns
- **docs/walkthrough/** - Component-specific guides

Use `ask-gemini.mjs` when you need:
- Answers that span multiple files
- Historical context or design rationale
- Complex "how does this work" questions
- Architecture impact analysis for changes
- Understanding edge cases and interactions
