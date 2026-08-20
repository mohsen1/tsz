# Legacy internal test corpus

These files are the dedicated unit-test and test-support modules extracted
from the pre-rewrite compiler at checkpoint
`2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`.

They are deliberately outside every Cargo package.  The reset keeps them as a
porting oracle without preserving the private APIs, crate graph, or semantic
assumptions of the retired implementation.  A test becomes active again only
after it is rewritten against a public replacement boundary and its behavior
is confirmed against pinned TypeScript 7.0.2.

The migration retained 969 dedicated source-tree test/support files. It also
extracted 2,521 inline test functions from deleted implementation files into
`inline/`; every fragment carries its source path, line, commit, and content
hash. Two inline tests whose only purpose was retired policy were omitted.
Verify that archive with:

```bash
python3 scripts/reset/verify-legacy-inline.py --verify-source
```

The source check requires the recorded parent commit. The shorter command
without `--verify-source` validates the committed archive in shallow clones.
Black-box suites under `crates/*/tests`, external harnesses under `scripts`,
and benchmark fixtures remain in their original locations.
