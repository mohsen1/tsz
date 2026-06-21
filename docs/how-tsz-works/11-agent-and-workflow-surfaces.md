# Agent And Workflow Surfaces

This repository is built in a crowded multi-agent environment, so workflow files
are part of how `tsz` works. They encode the coordination rules that keep
compiler changes from fighting each other.

## Repo Contract

`AGENTS.md` is the root contract. It covers:

- roadmap-first work selection;
- architecture boundaries;
- compatibility model;
- test commands and local-suite limits;
- worktree and disk hygiene;
- GitHub and PR body requirements;
- anti-hardcoding rules;
- bug/fix discipline;
- LLM context hygiene.

This guide does not replace `AGENTS.md`; it explains where the files live and how
they fit together.

## Repo-Local Skills

Skills under `.agents/skills/` capture repeatable workflows:

- `.agents/skills/tsz-worktree-intake/SKILL.md`
- `.agents/skills/tsz-disk-cache-hygiene/SKILL.md`
- `.agents/skills/tsz-pr-coordination/SKILL.md`
- `.agents/skills/tsz-ci-pr/SKILL.md`
- `.agents/skills/tsz-architecture/SKILL.md`
- `.agents/skills/tsz-conformance/SKILL.md`
- `.agents/skills/tsz-tracing/SKILL.md`
- `.agents/skills/tsz-performance-engineering/SKILL.md`
- `.agents/skills/tsz-project-bench/SKILL.md`
- `.agents/skills/tsz-iteration-audit/SKILL.md`
- `.agents/skills/rust-hygiene-audit/SKILL.md`

The generated inventory also mentions each skill reference and helper script.

## Assistant Configuration

`.claude/` and `.codex/` contain assistant-specific configuration, hooks, and
skill copies. They are not compiler layers, but they affect how changes are
made. After editing `.codex/`, `.claude/`, `AGENTS.md`, or startup hooks, run:

```bash
scripts/agents/llm-context-audit.py
```

## Worktree And Disk Hygiene

Before new worktrees, use:

```bash
scripts/setup/disk-worktree-guard.sh
git worktree list
```

For routine intake, `scripts/agents/disk-preflight.sh` reports the active branch,
dirty state, disk headroom, TypeScript submodule state, and reusable worktree
signals.

## GitHub Workflow

GitHub is the coordination surface. PR bodies must include:

- `Goal: <green|fast|grow|hold>`
- a `## Verification` section;
- a `## Provenance` block with machine, assistant, model, and effort.

The PR author lands their own PR through the native merge queue after exact-head
CI passes. Do not merge WIP or draft PRs.

## Why This Matters To Compiler Work

Compiler parity work is easy to break with process shortcuts. The workflow
surface protects:

- no hidden ownership lanes;
- no stale draft PRs;
- no broad local suites that exhaust machines;
- no context-heavy startup hooks;
- no file-name or test-name semantic patches;
- no branch overlap that silently reverts another fix.
