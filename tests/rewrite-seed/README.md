# R0 seed oracle matrix

These fixtures are the first clean-slate compatibility floor. The gate runs
each source through the pinned TypeScript 7.0.2 compiler and the replacement
`tsz` binary in isolated directories, then compares the public process result.

For diagnostic cases the comparison is exact over:

- process exit status;
- diagnostic order and count;
- file, category, code, one-based line and column; and
- the complete diagnostic message, including punctuation.

For the emit case it additionally compares the emitted JavaScript byte for
byte. The expected result is intentionally obtained from the pinned oracle at
gate time; there is no hand-maintained output that can drift away from it.

TypeScript's non-pretty CLI diagnostic format does **not** expose diagnostic
span length. The gate therefore does not invent or infer a length. The public
Rust seed `assignment_uses_structured_relation_failure` in
`crates/tsz-core/rewrite-tests/foundation.rs` pins the replacement's byte start
and length directly.

Run from anywhere in the checkout:

```bash
scripts/reset/seed-oracle.sh --tsz .target/debug/tsz
```

The wrapper delegates oracle setup to `scripts/conformance/oracle.sh`. A warm
cache is reused without network access; a cold cache installs the version named
by `scripts/conformance/typescript-versions.json` through that existing
machinery.
