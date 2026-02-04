# GOAL

**Goal**: Match `tsc` behavior exactly. Every error, inference, and edge case must be identical to TypeScript's compiler.

Important document: docs/architecture/NORTH_STAR.md

## CRITICAL: Check Session Coordination

Before starting work, check [docs/sessions/](docs/sessions/) to understand what other sessions are working on. Your session is determined by your directory name (tsz-1, tsz-2, tsz-3, tsz-4).

1. Make sure you have the latest session files from the repo's origin remote
2. Read all session files to avoid duplicate/conflicting work
3. When starting work, update your session file immediately with the current task, commit and push so others see
4. When finishing, move to history and note outcome

## CRITICAL: Use Skills

It's very important to use the available skills frequently to maximize productivity and code quality.

- Use tsz-gemini skill for 
  - codebase questions
  - architecture understanding
  - code reviews
  - implementation strategies
  - fixing bugs and failing tests

- Use tsz-tracing skill for debugging:
  - conformance test failures
  - type inference issues
  - narrowing and control flow analysis
  - assignability check problems
  - Example: `TSZ_LOG=debug TSZ_LOG_FORMAT=tree cargo run -- file.ts`

## CRITICAL: How to Ask Gemini Effectively

**Vague questions get vague answers. Be SPECIFIC and CONCRETE.**

### Good Question Pattern:
```bash
./scripts/ask-gemini.mjs --solver "When checking this code:
\`\`\`typescript
function identity<T>(x: T): T { return x; }
const result = identity('hello'); // expected: string, got: unknown
\`\`\`
What exact function determines the return type? I need the file path and function name 
so I can add a debug log."
```

### Bad Question Pattern:
```bash
# TOO VAGUE - will get architectural overview, not actionable answer
./scripts/ask-gemini.mjs --solver "How does type inference work?"

# NO CONTEXT - Gemini can't help without knowing what you're looking at
./scripts/ask-gemini.mjs  "Why is the wrong type being used?"
```

### Template for Debugging:
```bash
./scripts/ask-gemini.mjs --checker "[PROBLEM]: I'm seeing [ACTUAL] but expected [EXPECTED].
Code: [PASTE TYPESCRIPT]
I traced to [FILE/FUNCTION] but need to find where [SPECIFIC DECISION] is made.
What exact function handles this? I need to add logging."
```

## CRITICAL: Always Ask Gemini
When to use Gemini:
- If unsure about next steps
- If stuck on a problem for more than 5 minutes
- If architecture or design questions arise
- If needing implementation strategies
- If needing help with debugging strategies
- When your solution fails tests and you can't find the issue


## Git Workflow
- Commit frequently with clear messages
- Push branches to remote regularly and rebase from main before and after each commit
- Only add files you touched, do not `git add -A`
- Make semantic and short commit headers
- Important: When syncing, also push to remote