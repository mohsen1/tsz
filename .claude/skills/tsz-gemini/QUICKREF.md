# TSZ Gemini Quick Reference

Quick commands for asking Gemini about the tsz codebase.

## KEY RULE: Be Specific!

**Vague questions → vague answers. Specific questions → actionable answers.**

```bash
# BAD - will get generic overview
./scripts/ask-gemini.mjs --solver "How does type inference work?"

# GOOD - asks for specific location
./scripts/ask-gemini.mjs --solver "What file and function handles return type inference 
for generic function calls like foo<T>(x: T): T?"

# BEST - includes concrete example and what you need
./scripts/ask-gemini.mjs --solver "When checking \`const x = identity('hello')\` where
identity is \`<T>(x: T): T\`, what code path determines that x should be string?
I need the exact function name so I can add a debug trace."
```

## Question Templates That Work

### Finding Code Location
```bash
./scripts/ask-gemini.mjs --solver "What exact file and function handles [OPERATION]?
I need to add logging to trace [VALUE]."
```

### Debugging Wrong Type
```bash
./scripts/ask-gemini.mjs --solver "This code produces [WRONG TYPE] but should produce [RIGHT TYPE]:
\`\`\`typescript
[PASTE CODE]
\`\`\`
What code path determines this? I traced to [FUNCTION] in [FILE] but need to go deeper."
```

### Understanding Call Sequence
```bash
./scripts/ask-gemini.mjs --checker "List the exact call sequence when tsz processes:
\`\`\`typescript
[CODE]
\`\`\`
From [START FUNCTION] to [END CONDITION]."
```

## Presets

```bash
./scripts/ask-gemini.mjs --solver "question"     # Type solver, inference, compatibility
./scripts/ask-gemini.mjs --checker "question"    # Type checker, diagnostics, AST
./scripts/ask-gemini.mjs --binder "question"     # Symbols, scopes, CFG
./scripts/ask-gemini.mjs --parser "question"     # Parser, scanner, AST nodes
./scripts/ask-gemini.mjs --emitter "question"    # Code emission, transforms
./scripts/ask-gemini.mjs --lsp "question"        # Language server protocol
./scripts/ask-gemini.mjs --types "question"      # Type system overview
./scripts/ask-gemini.mjs --modules "question"    # Module resolution
```

## Custom Paths

```bash
# Single file
./scripts/ask-gemini.mjs --include=src/solver/infer.rs "question"

# Multiple paths
./scripts/ask-gemini.mjs --include="src/solver/* src/checker/*" "question"

# Include tests
./scripts/ask-gemini.mjs --include="src/solver src/solver/tests" "question"
```

## Useful Options

```bash
--dry       # Show files that would be included (no API call)
--query     # Print full query payload (debugging)
--no-skeleton # Disable code skeleton extraction
--no-use-vertex  # Use direct Gemini API instead of Vertex AI
--list      # List all presets
```

## Common Questions

### Understanding Type System
```bash
./scripts/ask-gemini.mjs --solver "How does type inference work?"
./scripts/ask-gemini.mjs --types "How are generics handled?"
./scripts/ask-gemini.mjs --solver "How does assignability checking work?"
```

### Debugging Errors
```bash
./scripts/ask-gemini.mjs --checker "Where is TS2322 reported?"
./scripts/ask-gemini.mjs --include=src/checker/error_reporter.rs "How are errors formatted?"
```

### Implementation Guidance
```bash
./scripts/ask-gemini.mjs --solver "What files should I modify to add feature X?"
./scripts/ask-gemini.mjs --include=src/solver/compat.rs "How does this work?"
```

## Follow-up Pattern

1. Ask initial question with preset
2. If Gemini needs more context, re-ask with `--include` for specific files
3. Add test files if needed: `--include="src/solver/tests/*"`
4. Iterate until you have the answer

Example:
```bash
# Initial
./scripts/ask-gemini.mjs --solver "How does conditional type inference work?"

# Deep dive into specific file
./scripts/ask-gemini.mjs --include=src/solver/conditional.rs "Explain in more detail"

# Look at test examples
./scripts/ask-gemini.mjs --include="src/solver/tests/*" "Show me test cases"
```
