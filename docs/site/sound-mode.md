---
title: Sound Mode
layout: layouts/base.njk
page_class: sound-mode
permalink: /sound-mode/index.html
extra_scripts: <script src="https://cdn.jsdelivr.net/npm/monaco-editor@0.52.2/min/vs/loader.js"></script><script src="/sound-mode-editors.js"></script>
---

# Sound Mode

<div class="alert alert-warning">
  <strong>Experimental</strong> - Sound Mode is still in exploration. It stays behind explicit flags, its coverage is intentionally narrow, and its behavior may change as we validate the rollout.
</div>

**tsz** has an experimental **Sound Mode** direction for stricter TypeScript compatibility checks than `tsc` provides by default.

## Current Status

Today, Sound Mode is deliberately small:

1. **Current entrypoint:** hidden CLI flag `--sound`, plus the playground / WASM `soundMode` input
2. **Current scoping:** project-wide checker boolean today; the design target is user-authored TypeScript implementation code first
3. **Current behavior:** tighter checking in a few high-value areas such as method bivariance, partial `any` propagation, and sticky freshness
4. **Current diagnostics:** standard TypeScript codes like `TS2322` / `TS2345`, not the final public TSZ diagnostic surface

```bash
tsz check --sound src/
```

The normal tsc-compatible tsconfig path does **not** currently accept `compilerOptions.sound`; it is reported as an unknown compiler option. Server/LSP support, per-file pragmas, report-only mode, and dedicated TSZ sound diagnostics are still planned work.

## What It Is Not

Sound Mode is **not**:

1. a formal proof of language soundness
2. a promise that all `.d.ts` files are truthful
3. a guarantee that third-party libraries are already purified
4. a claim that every runtime bug is prevented

TypeScript itself treats full soundness as a non-goal. tsz uses the word **sound** as a product direction for a stricter mode, not as a theorem.

## First Rollout

The base rollout is intentionally narrow:

1. start with explicit opt-in via `--sound`
2. focus on user source, not the whole ecosystem
3. keep declaration files as trust boundaries instead of adoption blockers
4. add migration tools before broadening semantics

The first stable target is roughly:

1. user-authored TypeScript source becomes `any`-less
2. method bivariance stays rejected in sound-scoped code
3. `useUnknownInCatchVariables`, `noUncheckedIndexedAccess`, and `exactOptionalPropertyTypes` become part of the default sound profile
4. dedicated TSZ diagnostics and auditable suppressions land before the mode is treated as broadly user-facing

## Planned Flags

These names reflect the intended rollout shape, but they are still planned rather than fully wired today:

Do not put these fields in `compilerOptions` yet. The normal tsc-compatible
tsconfig path rejects `compilerOptions.sound*` today; this is a planned
shape, not currently supported configuration.

```json
{
  "compilerOptions": {
    "sound": true,
    "soundReportOnly": true,
    "soundPedantic": false,
    "soundCheckDeclarations": false
  }
}
```

The owning config object is still an implementation decision. The project may keep Sound Mode CLI-only for longer, use a `tszOptions`-style object, or later accept a `compilerOptions.sound` family if coexistence with vanilla `tsc` is acceptable. The public direction is a flat `sound*` family; we do **not** plan to expose a nested `sound: { ... }` config object as the main public shape.

## Later Pilot Work

Some of the most ambitious parts of Sound Mode are intentionally **not** part of the base guarantee yet.

These stay separate until they are proven:

1. declaration-boundary projection for third-party and project-reference `.d.ts`
2. curated sound declaration overlays
3. broader ecosystem-facing migration features

The plan is to keep those behind a separate experimental track such as `soundBoundaryPilot`, instead of quietly folding them into base `sound` too early.

## Why This Rollout Shape

This approach keeps Sound Mode practical:

1. teams can try it without first cleaning all of npm
2. user-authored code gets stricter first
3. declaration boundaries can be improved later without pretending they are solved today
4. the product story stays honest while the implementation matures

## Playground

You can try the current demo in the [Playground](/playground/?example=sound_mode). The example is intentionally centered on the checks we are more comfortable advertising today, and the UI labels Sound Mode as experimental.

## Further Reading

The detailed plan is tracked in [SOUND_MODE.md](https://github.com/mohsen1/tsz/blob/main/docs/plan/SOUND_MODE.md), and broader milestones are in the [Internal Roadmap](https://github.com/mohsen1/tsz/blob/main/docs/plan/ROADMAP.md).
