---
name: tsz-gemini
description: Ask Gemini AI questions about the tsz TypeScript compiler codebase with full context. Use when working on tsz architecture, implementation, debugging, or understanding how components (Solver, Checker, Binder, etc.) work. Automatically gathers relevant files and code skeletons for accurate answers.
---

# TSZ Gemini Helper

This skill helps you ask Gemini AI questions about the tsz TypeScript compiler codebase with full project context. It automatically gathers relevant files, code skeletons, and documentation to provide accurate answers about architecture and implementation.

## When to Use This Skill

Use this skill when you need to:
- Understand how a component works (Solver, Checker, Binder, Parser, Emitter, LSP)
- Get architecture guidance before implementing features
- Debug type system issues or inference problems
- Understand existing patterns and conventions
- Find the right files for a specific task
- Get unstuck when blocked on implementation details

## How to Ask Gemini

The `ask-gemini.mjs` script is located at `./scripts/ask-gemini.mjs` in the tsz project root.

### Basic Usage

```bash
# Ask a general question
./scripts/ask-gemini.mjs "How does type inference work for generic functions?"

# Ask about a specific component (uses preset with relevant files)
./scripts/ask-gemini.mjs --solver "How does assignability checking work?"

# Ask with custom file paths
./scripts/ask-gemini.mjs --include=src/solver/infer.rs "How does this file handle type inference?"
```

### Presets for Different Components

Use presets to automatically include the most relevant files:

| Preset | Purpose | Example |
|--------|---------|---------|
| `--solver` | Type solver, inference, compatibility | `./scripts/ask-gemini.mjs --solver "How does apparent type work?"` |
| `--checker` | Type checker, diagnostics, AST traversal | `./scripts/ask-gemini.mjs --checker "How are errors reported?"` |
| `--binder` | Symbol binding, scopes, CFG | `./scripts/ask-gemini.mjs --binder "How are symbols declared?"` |
| `--parser` | Parser, scanner, AST nodes | `./scripts/ask-gemini.mjs --parser "How is ASI implemented?"` |
| `--emitter` | Code emission, transforms, source maps | `./scripts/ask-gemini.mjs --emitter "How are source maps generated?"` |
| `--lsp` | Language server protocol | `./scripts/ask-gemini.mjs --lsp "How does go-to-definition work?"` |
| `--types` | Type system overview | `./scripts/ask-gemini.mjs --types "How are generics handled?"` |
| `--modules` | Module resolution, imports, exports | `./scripts/ask-gemini.mjs --modules "How does module resolution work?"` |

### Including Test Files

By default, test files are filtered out. Include them when needed:

```bash
# Include test files in context
./scripts/ask-gemini.mjs --include="src/solver src/solver/tests" "How do these tests work?"
```

### Additional Options

```bash
# Show what files would be included without calling API
./scripts/ask-gemini.mjs --dry "My question"

# Print the full query payload (for debugging)
./scripts/ask-gemini.mjs --query "My question"

# Use direct Gemini API instead of Vertex AI (fallback)
./scripts/ask-gemini.mjs --no-use-vertex "My question"

# List all available presets
./scripts/ask-gemini.mjs --list
```

## Workflow for Getting Answers

### Step 1: Ask Initial Question

Start with a focused question using the appropriate preset:

```bash
# Example: Understanding type inference
./scripts/ask-gemini.mjs --solver "How does type inference work for conditional types?"
```

### Step 2: Review Gemini's Response

Read the response and note:
- Files and line numbers referenced
- Key concepts and patterns mentioned
- Any follow-up questions you have

### Step 3: Ask Follow-up with More Context

If Gemini needs more files or you want deeper understanding, re-ask with additional includes:

```bash
# Gemini mentioned specific files - include them for deeper dive
./scripts/ask-gemini.mjs --solver --include="src/solver/infer.rs src/solver/apparent.rs" \
  "Can you explain the apparent type handling in more detail?"
```

### Step 4: Iterate as Needed

Continue asking follow-ups, adding more files as needed:

```bash
# Add test files to see examples
./scripts/ask-gemini.mjs --include="src/solver/infer.rs src/solver/tests/*" \
  "Show me examples of how conditional type inference is tested"
```

## Best Practices

### 1. Always Use Presets When Available

Presets automatically include the most relevant files and documentation. Use them instead of manually specifying paths unless you need something specific.

### 2. Include Failing Tests in Questions

When debugging, include the failing test code:

```bash
# Embed the test in your question
./scripts/ask-gemini.mjs --checker "Why is this test failing?

```typescript
function test_conditional_type_inference() {
  // ... test code ...
}
```"
```

### 3. Be Specific in Your Questions

- Good: "How does the solver handle conditional type inference when the condition depends on a type parameter?"
- Bad: "How do types work?"

### 4. Reference Files by Path

When you know specific files are relevant, include them:

```bash
./scripts/ask-gemini.mjs --include="src/solver/compat.rs" \
  "How does the Lawyer compatibility layer work?"
```

### 5. Use --include for Multiple Paths

You can specify multiple paths or use wildcards:

```bash
./scripts/ask-gemini.mjs --include="src/solver/*" \
  "How do all the solver modules work together?"
```

## Environment Setup

The script requires API keys. Set up environment variables:

```bash
# For Vertex AI Express (default)
export GCP_VERTEX_EXPRESS_API_KEY="your-key-here"

# OR for direct Gemini API (fallback)
export GEMINI_API_KEY="your-key-here"
```

The script uses:
- `yek` for code context gathering (install with `cargo install yek`)
- `ast-grep` for code skeleton extraction (install from ast-grep.github.io)
- Node.js for running the script

## Common Questions

### Q: What's the difference between --solver and --types?

- `--solver`: Focuses on the Solver component only (pure type operations)
- `--types`: Includes both Solver and Checker for type system overview

### Q: Should I include test files?

Only include test files when:
- You need examples of how a feature is tested
- You're debugging a failing test
- You want to understand test coverage

### Q: What if Gemini's answer isn't detailed enough?

Re-ask with:
1. More specific files using `--include`
2. The relevant test files
3. Reference specific line numbers from Gemini's previous answer

### Q: Can I use this for understanding error messages?

Yes! Include error context and relevant files:

```bash
./scripts/ask-gemini.mjs --checker "What does this error mean?

Error: TS2322: Type 'string' is not assignable to type 'number'.

Where is this error reported and how can I fix it?"
```

## Example Workflows

### Understanding a New Feature

```bash
# 1. Start with high-level question
./scripts/ask-gemini.mjs --solver "How does the solver handle template literal types?"

# 2. Deep dive into specific files mentioned
./scripts/ask-gemini.mjs --include="src/solver/literal.rs" \
  "Explain the template literal type inference in detail"

# 3. Look at test examples
./scripts/ask-gemini.mjs --include="src/solver/tests/*" \
  "Show me test cases for template literal types"
```

### Debugging a Type Error

```bash
# 1. Understand the error reporting
./scripts/ask-gemini.mjs --checker "Where and how is TS2322 reported?"

# 2. Understand the type checking logic
./scripts/ask-gemini.mjs --solver "How does assignability checking determine if string is assignable to number?"

# 3. Find the specific check
./scripts/ask-gemini.mjs --include="src/solver/compat.rs" \
  "Show me the code that checks primitive type assignability"
```

### Implementing a New Feature

```bash
# 1. Ask for architecture guidance
./scripts/ask-gemini.mjs --solver "I need to add support for mapped type modifiers. What files should I modify?"

# 2. Understand existing patterns
./scripts/ask-gemini.mjs --include="src/solver/mapped.rs" \
  "How are mapped types currently handled?"

# 3. Ask for implementation approach
./scripts/ask-gemini.mjs --solver --include="src/solver/mapped.rs src/solver/infer.rs" \
  "What's the right approach to add readonly modifier support to mapped types?"
```

## Troubleshooting

### Script fails with "yek not found"

```bash
cargo install yek
```

### Script fails with API key errors

```bash
# Check which API you need
./scripts/ask-gemini.mjs --help

# Set the appropriate environment variable
export GCP_VERTEX_EXPRESS_API_KEY="..."  # For Vertex AI (default)
export GEMINI_API_KEY="..."               # For direct Gemini API
```

### Context seems too small

The script auto-sizes to use ~90% of Gemini's 1M token context window. If you need more control:

```bash
# Override with explicit token limit
./scripts/ask-gemini.mjs --tokens=5000k "My question"
```

### Skeleton extraction fails

Skeleton extraction requires `ast-grep`. Install it from https://ast-grep.github.io/ or disable with `--no-skeleton`.
