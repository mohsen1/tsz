# Website: Eleventy Evaluation

## Goal

Move to a more robust, Markdown-first docs website with a unified layout, without React.

## Current State (tsz-3)

- Current site is custom-built using `website/build.mjs` with `marked`.
- Content is split between:
  - `website/content/*.md`
  - `docs/**/*.md`
  - one large hand-authored HTML page: `docs/architecture.html`
- Styling/layout is mostly in one large stylesheet plus inline CSS in architecture HTML.
- GitHub Pages deploy is already wired in `.github/workflows/gh-pages.yml`.

## Eleventy Fit

Eleventy is a strong fit for this codebase:

- Markdown-first static generation.
- No framework runtime required.
- Good for docs-style sites with hierarchical navigation.
- Incremental migration friendly (can migrate page-by-page).
- Easy to integrate with existing CI and static deploy.

## Recommended Eleventy Stack

- `@11ty/eleventy` (core)
- `@11ty/eleventy-navigation` (sidebar/navigation tree from frontmatter)
- `markdown-it` customization through Eleventy config
- Optional:
  - syntax highlight plugin (if we want upgraded code blocks)
  - link checker in CI (separate tool)

## Content Model

Use docs as source of truth:

- Canonical content: `docs/**/*.md`
- Add frontmatter to each page:
  - `title`
  - `description`
  - `eleventyNavigation`:
    - `key`
    - `parent`
    - `order`
- Convert `docs/architecture.html` into structured markdown pages:
  - `docs/architecture/introduction.md`
  - `docs/architecture/pipeline.md`
  - `docs/architecture/memory-architecture.md`
  - etc.

## Build Architecture

- New Eleventy site root: keep using `website/` folder.
- Eleventy input can be `docs/` plus `website/` templates/includes/static.
- Keep current plain JS widgets for interactive pieces.
- Keep metrics injection, but move it from ad-hoc string replacement to:
  - data file generation step (JSON)
  - Eleventy global data consumption.

## Robustness Requirements

- Build fails on:
  - broken internal links
  - invalid frontmatter schema
  - missing required page metadata
- Add deterministic nav ordering from frontmatter.
- Add a "docs lint" stage in CI:
  - markdown lint
  - link check
  - frontmatter validation

## Migration Plan (phased)

1. Bootstrap Eleventy in `website/` with one shared layout and nav.
2. Wire docs import from `docs/**/*.md`.
3. Migrate `website/content/index.md` and `benchmarks.md`.
4. Split and migrate `docs/architecture.html` into markdown docs.
5. Replace old custom renderer path in deploy workflow.
6. Remove dead templates and legacy build code.

## Risks

- `docs/architecture.html` conversion is non-trivial because it contains bespoke layout and scripting.
- Existing style can regress during migration unless we lock design tokens and component styles early.
- Link stability must be preserved (redirects/permalink map may be needed).

## Suggested Decision

Proceed with Eleventy migration in phases, preserving current site output paths as much as possible to avoid broken links.
