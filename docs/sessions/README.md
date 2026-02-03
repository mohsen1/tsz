# Session Coordination

This directory coordinates work across 4 parallel Claude Code sessions (`tsz-1` through `tsz-4`).

## Protocol

### Before Starting Work
1. Read all session files to understand what others are working on
2. Check for duplicate or conflicting work
3. If conflict exists, either coordinate or pick different work

### When Starting Work
1. Update your session file immediately with:
   - Current work item at the top
   - Move previous current work to history (keep max 20)
2. Be specific about what you're doing to help others avoid conflicts

### When Finishing Work
1. Move the item to history with outcome (completed/punted/fixed)
2. If punted, note why in the Punted Todos section

## Session Files

| Session | File | Current Work |
|---------|------|--------------|
| tsz-1 | [tsz-1.md](./tsz-1.md) | TBD |
| tsz-2 | [tsz-2.md](./tsz-2.md) | TBD |
| tsz-3 | [tsz-3.md](./tsz-3.md) | TBD |
| tsz-4 | [tsz-4.md](./tsz-4.md) | TBD |

## Work Discovery

Work items come from:
1. **AGENTS.md** - The main goals and priorities
2. **Test failures** - `cargo nextest run` output
3. **Conformance gaps** - TypeScript compatibility issues
4. **Code exploration** - Issues discovered during development

## Conflict Resolution

If two sessions are working on similar areas:
- Communicate via the session files
- Consider coordinating to divide the work
- One session may punt to the other with a note

---

*Sessions are identified by directory name (tsz-1, tsz-2, tsz-3, tsz-4). Each session updates only its own file.*
