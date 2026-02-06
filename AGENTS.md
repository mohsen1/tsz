# GOAL

**Goal**: Match `tsc` behavior exactly. Every error, inference, and edge case must be identical to TypeScript's compiler.

Important document: docs/architecture/NORTH_STAR.md

---

## Debugging
- **NEVER use `eprintln!`** — use the `tsz-tracing` skill instead.

---

## Tesing 
- Write unit tests for any new functionality
- It is a good idea to write a failing test first before implementing a feature

## Profiling
- Do NOT bind to port 3000. Disable profiler web UIs (`samply --no-open`, etc).

## Git Workflow
- Commit frequently with clear messages
- Only add files you touched, do not `git add -A`
- Make semantic and short commit headers
- Important: When syncing, also push to remote



Now, make sure repo is setup properly. Run `scripts/setup.sh`
