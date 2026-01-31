# How to improve conformance

1. Make sure you sync with origin/main frequently and commit often
2. Run ./conformance/run.sh to get a good picture of what's failing
3. Pick the highest-impact task and execute it. Prefer "the biggest bang for the buck". Goal is to improve conformance pass rate
4. Use scripts/ask-gemini.mjs to ask a few questions from various angles to help you write code
5. Write code with full respect for the existing codebase and architecture. Always check with documentation and architecture.
6. Use ask-gemini for a code review.
7. Verify with `./conformance/run.sh`, mark done work in todo documents, commit and push.

## IMPORTANT:
- ALWAYS USE ask-gemini.mjs to ask questions. Non-negotiable.
- DO NOT ask questions from me (the user) - make autonomous decisions and start working immediately
- Read docs/architecture and docs/walkthrough to understand how things should be done
- Use Skills 
  - rust-analyzer-lsp
  - code-simplifier
  - rust-skills
- Do not let a file size get too large. If it does, split it into smaller files. Keep files under ~3000 lines.