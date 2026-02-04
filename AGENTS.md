# GOAL

**Goal**: Match `tsc` behavior exactly. Every error, inference, and edge case must be identical to TypeScript's compiler.

Important document: docs/architecture/NORTH_STAR.md

---

## CRITICAL: MANDATORY GEMINI CONSULTATION WORKFLOW

**🚨 THIS IS NON-NEGOTIABLE 🚨**

For ANY changes to `src/solver/` or `src/checker/`, you MUST follow this workflow:

### The Two-Question Rule

**Before writing ANY implementation code, ask Gemini TWO questions:**

#### Question 1: Approach Validation (PRE-implementation)
```bash
./scripts/ask-gemini.mjs --include=src/solver "I need to implement [FEATURE/BUGFIX].
Here's my understanding of the problem: [PROBLEM DESCRIPTION].
Here's my planned approach: [YOUR PLAN].

Is this approach correct? What function should I modify? Are there edge cases I'm missing?
Please provide: 1) File paths, 2) Function names, 3) Edge cases, 4) Potential pitfalls."
```

#### Question 2: Implementation Review (POST-implementation)
```bash
./scripts/ask-gemini.mjs --include=src/solver "I implemented [FEATURE/BUGFIX] in [FILE].
Here's what I changed: [PASTE CODE OR DESCRIBE CHANGES].

Please review: 1) Is this correct? 2) Does it match TypeScript behavior? 3) Are there bugs?
Be brutal - if it's wrong, tell me specifically what's wrong."
```

### What Happens Without Gemini Consultation

**Evidence from Investigation (2026-02-04):**
- Latest commit `f2d4ae5d5` (discriminant narrowing) had **3 CRITICAL BUGS** that Gemini immediately found:
  1. **Reversed subtype check** - asked `is_subtype_of(property_type, literal)` instead of `is_subtype_of(literal, property_type)`
  2. **Missing type resolution** - didn't handle `Lazy`/`Ref`/`Intersection` types
  3. **Broken for optional properties** - failed on `{ prop?: "a" }` cases

**This happens EVERY TIME Claude Code implements without asking Gemini first.**

### Examples of Good vs Bad Patterns

#### ❌ BAD PATTERN (Do NOT do this):
```
1. Read the code
2. Implement the fix
3. Write tests
4. Commit with "feat: implemented X"
5. Update session file

Result: BROKEN CODE that needs to be reverted
```

#### ✅ GOOD PATTERN (Do this EVERY TIME):
```
1. Read session file and understand the problem
2. Ask Gemini Question 1: "What's the right approach?"
3. Implement based on Gemini's guidance
4. Ask Gemini Question 2: "Review my implementation"
5. Fix any issues Gemini finds
6. Test and commit
7. Update session file

Result: WORKING CODE that matches TypeScript
```

### When You MUST Ask Gemini

**MANDATORY for:**
- Any changes to `src/solver/*.rs`
- Any changes to `src/checker/*.rs`
- Type system logic (subtyping, variance, inference)
- Control flow analysis
- Narrowing operations
- Assignability checks

**Optional for:**
- Documentation updates
- Test infrastructure
- CLI changes
- Build system changes

---

## CRITICAL: Check Session Coordination

Before starting work, check [docs/sessions/](docs/sessions/) to understand what other sessions are working on. Your session is determined by your directory name (tsz-1, tsz-2, tsz-3, tsz-4).

1. Make sure you have the latest session files from the repo's origin remote
2. Read all session files to avoid duplicate/conflicting work
3. When starting work, update your session file immediately with the current task, commit and push so others see
4. When finishing, move to history and note outcome

---

## CRITICAL: Use Skills

It's very important to use the available skills frequently to maximize productivity and code quality.

### tsz-gemini skill
Use for:
- codebase questions
- architecture understanding
- code reviews
- implementation strategies
- fixing bugs and failing tests

**This skill wraps ask-gemini.mjs - use it frequently!**

### tsz-tracing skill
Use for debugging:
- conformance test failures
- type inference issues
- narrowing and control flow analysis
- assignability check problems
- Example: `TSZ_LOG=debug TSZ_LOG_FORMAT=tree cargo run -- file.ts`

---

## CRITICAL: How to Ask Gemini Effectively

**Vague questions get vague answers. Be SPECIFIC and CONCRETE.**

### Good Question Pattern:
```bash
./scripts/ask-gemini.mjs --include=src/solver "When checking this code:
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
./scripts/ask-gemini.mjs --include=src/solver "How does type inference work?"

# NO CONTEXT - Gemini can't help without knowing what you're looking at
./scripts/ask-gemini.mjs --include=src/solver "Why is the wrong type being used?"
```

### Template for Pre-Implementation:
```bash
# Use Flash (default) for approach validation - fast and sufficient
./scripts/ask-gemini.mjs --include=src/solver "I need to implement [FEATURE].
Problem: [PROBLEM STATEMENT]
My planned approach: [YOUR APPROACH]

Before I implement: 1) Is this the right approach? 2) What functions should I modify?
3) What edge cases do I need to handle? 4) Are there TypeScript behaviors I need to match?"
```

### Template for Post-Implementation Review:
```bash
# Use Pro (--pro flag) for implementation reviews - better at catching architectural bugs
./scripts/ask-gemini.mjs --pro --include=src/solver "I implemented [FEATURE] in [FILE]:[FUNCTION].
Changes: [PASTE CODE OR DIFF]

Please review: 1) Is this logic correct for TypeScript? 2) Did I miss any edge cases?
3) Are there type system bugs? Be specific if it's wrong - tell me exactly what to fix."
```

### Template for Debugging:
```bash
./scripts/ask-gemini.mjs --include=src/checker "[PROBLEM]: I'm seeing [ACTUAL] but expected [EXPECTED].
Code: [PASTE TYPESCRIPT]
I traced to [FILE/FUNCTION] but need to find where [SPECIFIC DECISION] is made.
What exact function handles this? I need to add logging."
```

---

## CRITICAL: When to Ask Gemini

**ASK GEMINI IMMEDIATELY if:**
- You're about to modify `src/solver/` or `src/checker/` (MANDATORY)
- You're unsure about the approach (MANDATORY)
- You've been stuck on a problem for more than 5 minutes (MANDATORY)
- Architecture or design questions arise
- You need implementation strategies
- You need help with debugging strategies
- Your solution fails tests and you can't find the issue

**If you don't ask Gemini for solver/checker changes, the code WILL BE WRONG.**

This is not an exaggeration - it's a pattern proven by investigation.

### 🚨 RATE LIMIT HANDLING - MANDATORY WAIT 🚨

**If ask-gemini.mjs returns HTTP 429 (rate limit):**

1. **STOP ALL IMPLEMENTATION WORK**
2. **WAIT until rate limit resets** (typically 1-2 minutes, sometimes up to 60 minutes)
3. **DO NOT proceed without Gemini consultation**

**Under NO circumstances should you:**
- Implement the fix anyway and "ask Gemini later"
- Skip the review because "it's probably fine"
- Commit code that hasn't been validated by Gemini

**Why this is critical:**
- Investigation of 500 commits proved that 100% of unreviewed solver/checker changes had critical bugs
- The latest commit (f2d4ae5d5) had 3 critical type system bugs that Gemini immediately found
- Waiting 60 minutes for rate limit reset is faster than spending days debugging broken code

**If you hit rate limits:**
- Take a break, document findings, or work on non-critical tasks (docs, tests, CLI)
- Come back and ask Gemini once rate limit resets
- Only proceed with implementation AFTER getting Gemini's guidance

---

## CRITICAL: Flash vs Pro Model Selection

**ask-gemini.mjs now defaults to Flash model for faster responses.**

### When to use Flash (default):
- **Code lookup**: "What function handles X?"
- **Simple questions**: "How does feature X work?"
- **Approach validation**: "Is this the right direction?"
- **Most day-to-day questions**

**Flash is faster and cheaper - use it for 80% of questions.**

### When to use Pro (`--pro` flag):
- **Implementation reviews**: "Review this code for bugs"
- **Complex architectural decisions**: "How should I redesign this?"
- **Multi-file changes**: "I'm touching 5 files, is this right?"
- **Type system logic**: "Is this subtype check correct?"

**Pro is more thorough for complex questions - use it for the remaining 20%.**

### Examples:
```bash
# Flash (default) - simple lookup
./scripts/ask-gemini.mjs --include=src/solver "Where is discriminant narrowing implemented?"

# Flash (default) - approach validation
./scripts/ask-gemini.mjs --include=src/solver "Should I use visitor pattern or direct TypeKey match?"

# Pro - implementation review (CRITICAL)
./scripts/ask-gemini.mjs --pro --include=src/solver "Review my discriminant narrowing implementation:
[PASTE CODE]
Does this match TypeScript behavior?"

# Pro - architectural question
./scripts/ask-gemini.mjs --pro --include=src/solver --include=src/checker "I need to add support for conditional type inference.
What's the right approach? Which files need to change?"
```

**Key Rule**: If you're asking "is this code correct?" or "did I implement this right?", use `--pro`.

---

## Git Workflow
- Commit frequently with clear messages
- Push branches to remote regularly and rebase from main before and after each commit
- Only add files you touched, do not `git add -A`
- Make semantic and short commit headers
- Important: When syncing, also push to remote

---

## Hook Configuration (Recommended)

To enforce this workflow, consider adding a UserPromptSubmit hook in `~/.claude/settings.json`:

```json
{
  "hooks": {
    "UserPromptSubmit": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "~/.claude/hooks/mandatory-gemini-check.sh"
          }
        ]
      }
    ]
  }
}
```

Example hook script (`~/.claude/hooks/mandatory-gemini-check.sh`):
```bash
#!/bin/bash
cat <<'EOF'
⚠️  REMINDER: MANDATORY GEMINI CONSULTATION ⚠️

Before implementing ANY solver/checker changes:
1. Ask Gemini: "What's the correct approach for [FEATURE]?"
2. Implement based on guidance
3. Ask Gemini: "Review my implementation for [FEATURE]"
4. Fix any issues found

Evidence: Investigation of 500 commits showed EVERY implementation without
Gemini consultation had critical type system bugs.

If modifying src/solver/ or src/checker/: YOU MUST ASK GEMINI FIRST.
EOF
```

See [Mandatory Skill Activation Hook for Claude Code](https://gist.github.com/umputun/570c77f8d5f3ab621498e1449d2b98b6) for details.

---

## Summary

1. **Two-Question Rule**: Ask Gemini BEFORE and AFTER implementing solver/checker changes
2. **No Exceptions**: Even "simple" changes have hidden complexities
3. **Evidence-Based**: Investigation proves skipping Gemini = broken code
4. **Skills Available**: Use `tsz-gemini` skill which wraps `ask-gemini.mjs`

**When in doubt: ASK GEMINI. It's faster than fixing broken code later.**
