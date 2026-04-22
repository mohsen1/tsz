---
title: Sound Mode
layout: layouts/base.njk
page_class: sound-mode
permalink: /sound-mode/index.html
extra_scripts: <script src="/sound-mode-editors.js"></script>
---

# Sound Mode

<div class="alert alert-warning">
  <strong>Experimental</strong> - Sound Mode is still in exploration. It stays behind explicit flags, its coverage is intentionally narrow, and its behavior may change as we validate the rollout.
</div>

**tsz** has an experimental **Sound Mode** for teams that want stricter TypeScript compatibility checks than `tsc` provides by default.

## Current Status

Today, Sound Mode is deliberately small:

1. **Current entrypoints:** CLI `--sound` and `compilerOptions.sound`
2. **Current target:** user-authored TypeScript implementation code first
3. **Current behavior:** tighter checking in a few high-value areas such as method bivariance, `any` propagation, and sticky freshness
4. **Current diagnostics:** standard TypeScript codes like `TS2322` / `TS2345`, not the final public TSZ diagnostic surface

```bash
tsz check --sound src/
```

```json
{
  "compilerOptions": {
    "sound": true
  }
}
```

## What It Is Not

Sound Mode is **not**:

1. a formal proof of language soundness
2. a promise that all `.d.ts` files are truthful
3. a guarantee that third-party libraries are already purified
4. a claim that every runtime bug is prevented

TypeScript itself treats full soundness as a non-goal. tsz uses the word **sound** as a product direction for a stricter mode, not as a theorem.

## First Rollout

The base rollout is intentionally narrow:

1. start with explicit opt-in via `--sound` or `compilerOptions.sound`
2. focus on user source, not the whole ecosystem
3. keep declaration files as trust boundaries instead of adoption blockers
4. add migration tools before broadening semantics

The first stable target is roughly:

1. user-authored TypeScript source becomes `any`-less
2. method bivariance stays rejected in sound-scoped code
3. `useUnknownInCatchVariables`, `noUncheckedIndexedAccess`, and `exactOptionalPropertyTypes` become part of the default sound profile
4. dedicated TSZ diagnostics and auditable suppressions land before the mode is treated as broadly user-facing

## Planned Flags

Only `compilerOptions.sound` is currently wired. The other names below reflect the intended rollout shape and are still planned:

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

Intended meaning:

1. `sound`: enable the default sound profile
2. `soundReportOnly`: surface sound diagnostics without failing the run yet
3. `soundPedantic`: add stricter bug-finding checks that are useful but not core to the first rollout
4. `soundCheckDeclarations`: later opt-in for checking first-party declaration files too

The public direction is a flat `sound*` family. We do **not** plan to expose a nested `sound: { ... }` config object as the main public shape.

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

## Read More

1. [Internal Sound Mode Plan](https://github.com/mohsen1/tsz/blob/main/docs/plan/SOUND_MODE.md)
2. [Playground](/playground/)
