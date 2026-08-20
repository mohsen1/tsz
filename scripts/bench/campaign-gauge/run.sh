#!/usr/bin/env bash
# Historical status shim for the retired identity/materialization campaign.
#
# The clean-slate compiler intentionally has none of the old checker/solver
# packages, feature-channel environment variables, or flag-on unit suites this
# gauge measured. The fixture remains checked in as historical evidence, but
# running it against the replacement would manufacture a meaningless result.
set -euo pipefail

case "${1:-}" in
  -h|--help)
    cat <<'EOF'
Usage: scripts/bench/campaign-gauge/run.sh

Unavailable for the clean-slate compiler. This path is retained only to make
historical references fail explicitly; use the seed oracle and project-row
benchmark harnesses for replacement-compiler evidence.
EOF
    exit 0
    ;;
esac

echo "campaign-gauge: unavailable for the clean-slate compiler" >&2
echo "the retired checker/solver feature channels are intentionally absent" >&2
exit 2
