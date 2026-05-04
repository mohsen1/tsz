Status: ready
Branch: fix/dts-symbol-observer-computed-member-05031950
Owner: mohsen1
Created: 2026-05-03T19:50:37Z

## Intent

Fix the declaration emit mismatch in `symbolObserverMismatchingPolyfillsWorkTogether`, where an object literal with an emittable computed unique-symbol key emits an extra synthetic numeric index signature.

## Scope

- Keep the production change in declaration-emitter object-literal computed member rewrite logic.
- Add a focused emitter regression test.

## Verification

- `cargo fmt -p tsz-emitter --check`
- `cargo nextest run -p tsz-emitter -E 'test(symbol_observer)'`
- `./scripts/emit/run.sh --dts-only --filter=symbolObserverMismatchingPolyfillsWorkTogether --verbose --skip-build`
