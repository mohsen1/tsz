# Maintaining This Guide

This guide has two kinds of content:

1. Narrative pages written by humans or agents under `docs/how-tsz-works/`.
2. Generated file inventory pages under `docs/how-tsz-works/file-inventory/`.

The narrative explains how `tsz` works. The inventory proves that every
repository file path is mentioned somewhere in this guide.

## Scripts

Two scripts maintain the generated reference:

- `scripts/docs/generate-how-tsz-inventory.mjs`
- `scripts/docs/check-how-tsz-docs-coverage.mjs`

Regenerate and check with:

```bash
node scripts/docs/generate-how-tsz-inventory.mjs
node scripts/docs/check-how-tsz-docs-coverage.mjs
```

The generator uses:

```bash
git ls-files --cached --others --exclude-standard
```

That means new untracked docs and scripts are included during development, not
only after staging.

## After Adding Files

When adding, moving, or deleting files:

1. Update the relevant narrative page if the architecture or workflow changed.
2. Regenerate the inventory.
3. Run the coverage checker.
4. Check line counts so no hand-authored, generated, source, test, or script file
   exceeds the repo's 2,000 physical line ceiling.

Useful command:

```bash
find docs/how-tsz-works scripts/docs -type f -print0 | xargs -0 wc -l
```

## When To Edit Existing Architecture Docs

Do not put routine status into durable docs. Edit architecture or roadmap docs
only when the contract changes. For ordinary PR status, use PR bodies,
comments, issues, and CI artifacts.

Use these targets:

- `docs/plan/ROADMAP.md` only for durable goal, gate, or metric changes.
- `docs/architecture/` for durable boundary and architecture contracts.
- `docs/specs/` for behavior specs.
- `docs/site/` for product/site content.
- `docs/how-tsz-works/` for explanatory maps and generated file inventory.

## Documentation Quality Bar

Good additions name:

- the owner layer;
- the data that crosses a boundary;
- the relevant files or directories;
- the verification command or CI gate;
- any cache or identity invariant involved.

Avoid:

- duplicating `ROADMAP.md` status;
- inventing a new architecture vocabulary when `BOUNDARIES.md` already names it;
- documenting temporary agent/runtime skills as TSZ repo skills;
- relying on a narrow local check to prove a broad parity claim.
