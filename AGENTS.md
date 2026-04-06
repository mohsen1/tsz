# TSZ — TypeScript Compiler in Rust

You are working on **tsz**, a TypeScript compiler written in Rust. The absolute target is to match `tsc` behavior exactly.

## Key References

- **Architecture**: `.claude/CLAUDE.md` — full spec, pipeline, responsibility split, hard rules
- **Campaign protocol**: `scripts/session/AGENT_PROTOCOL.md` — discipline cycle, KPIs, tiers
- **Campaign definitions**: `scripts/session/campaigns.yaml` — missions, entry files, research commands
- **Campaign agent playbook**: `.opencode/agents/conformance-campaign.md` — full workflow

## Workflow Summary

1. **Research** — `python3 scripts/conformance/query-conformance.py --dashboard`
2. **Plan** — Write down the invariant before coding. Verify against source files.
3. **Implement** — Solver owns WHAT, Checker owns WHERE. Multi-crate changes are normal.
4. **Verify** — `scripts/session/verify-all.sh` (unit + conformance + emit + LSP). Zero regressions.
5. **Push** — Only after verify-all.sh passes: `git push origin main`
