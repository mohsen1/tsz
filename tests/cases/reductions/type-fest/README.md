# Type-Fest Project Reductions

These fixtures capture the first type-fest project blocker families from
issue #8774. They are intentionally small, self-contained `tsc` oracles rather
than generated project snapshots.

Run the oracle with:

```sh
tsc -p tests/cases/reductions/type-fest/tsconfig.json --pretty false
```

Expected result: no diagnostics. Each fixture uses `Assert<Equal<...>>` aliases
so a checker that widens the utility result, drops a key-space modifier, or
fails a recursive reduction reports a concrete type error.
