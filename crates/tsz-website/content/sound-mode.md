# Sound Mode

Sound Mode is a direction for `tsz`: a future way to ask for TypeScript checking that is stricter about the places where today's TypeScript intentionally trades type safety for compatibility and convenience.

It is not the main workstream right now. The project is focused first on matching `tsc` behavior and then on performance tuning. Until those two foundations are in place, Sound Mode is an aspiration and a small prototype, not a product promise.

<div class="alert alert-warning">
  <strong>Not active product work</strong> - Sound Mode exists today only as an experimental prototype behind explicit entrypoints. Its behavior is narrow, its public configuration is not settled, and it may change substantially.
</div>

## What It Is Not

Sound Mode is not:

1. a formal proof of language soundness
2. a claim that every runtime bug can be prevented by a type checker
3. a guarantee that all `.d.ts` files are truthful
4. a promise that third-party libraries are already safe under stricter rules
5. a stable public contract for configuration, diagnostics, editor support, or suppression syntax

The word "sound" is a product direction here: stricter, more explicit checking for known TypeScript escape hatches. It is not a theorem.

## Why Keep The Page

Sound Mode is on the site because the project still wants the conversation. TypeScript users have more practical experience with strictness tradeoffs than any design document can capture on its own.

The useful question is not "can TypeScript become perfectly sound?" The useful question is: if `tsz` eventually offers a stricter mode, what should it reject, what should it leave alone, and what migration support would make it usable on real projects?

## What It Wants To Be

The goal is not to prove TypeScript sound. TypeScript explicitly values JavaScript compatibility and ecosystem adoption over a fully sound type system, and `tsz` should not pretend otherwise.

The more practical goal is a mode for teams that would choose more explicit boundaries in exchange for fewer type-driven runtime surprises in their own source code. A useful first version would probably:

1. focus on user-authored TypeScript implementation files first
2. reject a small, named set of patterns that are known to hide unsafe values or unsafe assignments
3. treat declaration files and third-party packages as trust boundaries rather than asking every team to clean all of npm
4. provide migration paths, report-only workflows, and auditable escapes before expecting broad adoption

In other words: Sound Mode should make application code more honest without making the existing TypeScript ecosystem somebody's manual cleanup project.

## A Plausible First Version

The first useful version should be boringly narrow. The project plan currently points toward a default profile that:

1. bans explicit `any` in sound-scoped user source
2. disables method parameter bivariance in sound-scoped assignability
3. implies `useUnknownInCatchVariables`, `noUncheckedIndexedAccess`, and `exactOptionalPropertyTypes`
4. keeps declaration files as trust-boundary inputs by default
5. emits dedicated TSZ sound diagnostics with auditable suppressions
6. supports staged adoption, likely including a report-only mode

More ambitious ideas, such as declaration-boundary projection, curated declaration overlays, broader ecosystem migration tooling, stricter array variance, and pedantic bug-finding checks, need to prove themselves separately. They should not be quietly marketed as part of the first stable guarantee.

## Configuration Is Still Open

Do not put Sound Mode options in `compilerOptions` today. The normal `tsc`-compatible config path rejects `compilerOptions.sound`.

The eventual public shape could stay CLI-only for longer, use a `tszOptions`-style object, or later expose a flat `sound*` option family if that can coexist cleanly with vanilla `tsc` workflows. The exact owner is still an implementation decision.

Names like these describe the direction, not working config:

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

## Help Shape It

The most important input now is not more theory in isolation. It is feedback from TypeScript users about where stricter checking would actually help and where it would make real projects worse.

Useful questions:

1. Which TypeScript unsoundness has caused real bugs in your codebase?
2. Which stricter checks would you accept in application code but not in library declarations?
3. Where would report-only mode, suppressions, or migration tools be required before adoption?
4. How should `tsz` handle values crossing from third-party `.d.ts` files into sound-scoped source?
5. Which existing TypeScript flags already feel like part of a stricter baseline?

If you care about a stricter TypeScript, the project wants that feedback before Sound Mode becomes a larger implementation effort.

## Playground

You can try the current demo in the [Playground](/playground/?example=sound_mode). The example is intentionally limited to checks the project is comfortable showing today, and the UI labels Sound Mode as experimental.

## Further Reading

The detailed plan is tracked in [SOUND_MODE.md](https://github.com/mohsen1/tsz/blob/main/docs/plan/SOUND_MODE.md), and broader milestones are in the [Internal Roadmap](https://github.com/mohsen1/tsz/blob/main/docs/plan/ROADMAP.md).
