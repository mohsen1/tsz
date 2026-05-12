Status: ready
Owner: codex
Branch: codex/fourslash-organize-imports-type-order-20260512
Scope: Restore fourslash organizeImportsType8 by converting organizeImports edits to FileTextChanges and honoring organizeImportsTypeOrder in named import sorting.

Verification:
- cargo fmt --all
- cargo test -p tsz-cli organize_imports_honors_type_order_preference -- --nocapture
- ./scripts/fourslash/run-fourslash.sh --filter=organizeImportsType8 --workers=1 --timeout=60000 --json-out=/tmp/tsz-main-organizeImportsType8-fixed.json
- ./scripts/fourslash/run-fourslash.sh --filter=organizeImportsType --workers=1 --timeout=60000 --json-out=/tmp/tsz-organizeImportsType-fixed.json
- ./scripts/fourslash/run-fourslash.sh --shard=1/6 --shard-strategy=weighted --skip-build --workers=4 --timeout=60000 --json-out=/tmp/tsz-fourslash-shard1-after-organize.json

Notes:
- Started after #5739 failed fourslash-1 on organizeImportsType8.
- Local reproduction showed the same failure on current main, so this is an independent CI blocker rather than a regression in #5739.
- Fix converts organizeImports edits to TypeScript FileTextChanges and threads organizeImportsTypeOrder through the provider.
- Original failing shard now passes locally: 1094 passed, 0 failed, peak worker RSS 718MB.
