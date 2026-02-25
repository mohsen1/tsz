#!/usr/bin/env bash
# emit: targets the #5 worst-pass-rate area (redirected from emitter to conformance)
source "$(git rev-parse --show-toplevel)/scripts/sessions/_conformance-core.sh"
emit_prompt 5
