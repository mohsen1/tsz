#!/usr/bin/env bash
set -euo pipefail

PROJECT="${PROJECT_ID:-${GOOGLE_CLOUD_PROJECT:-thirdface-ai-oauth}}"
REGION="${LOCATION:-${BUILD_REGION:-us-central1}}"
CURRENT_BUILD_ID="${BUILD_ID:-}"
CURRENT_TRIGGER="${TRIGGER_NAME:-}"
CURRENT_BRANCH="${BRANCH_NAME:-}"

if [[ -z "$CURRENT_BUILD_ID" || -z "$CURRENT_TRIGGER" || -z "$CURRENT_BRANCH" ]]; then
  echo "No Cloud Build trigger context; skipping superseded build cancellation"
  exit 0
fi

current_created="$(
  gcloud builds describe "$CURRENT_BUILD_ID" \
    --project="$PROJECT" \
    --region="$REGION" \
    --format='value(createTime)' 2>/dev/null || true
)"

if [[ -z "$current_created" ]]; then
  echo "Could not resolve current build createTime; skipping superseded build cancellation" >&2
  exit 0
fi

superseded_ids="$(
  gcloud builds list \
    --project="$PROJECT" \
    --region="$REGION" \
    --filter='status=(QUEUED OR WORKING)' \
    --format=json |
    python3 -c '
import json
import sys

current_id, trigger, branch, current_created = sys.argv[1:5]
for build in json.load(sys.stdin):
    if build.get("id") == current_id:
        continue
    substitutions = build.get("substitutions") or {}
    if substitutions.get("TRIGGER_NAME") != trigger:
        continue
    if substitutions.get("BRANCH_NAME") != branch:
        continue
    if build.get("createTime", "") < current_created:
        print(build["id"])
' "$CURRENT_BUILD_ID" "$CURRENT_TRIGGER" "$CURRENT_BRANCH" "$current_created"
)"

if [[ -z "$superseded_ids" ]]; then
  echo "No superseded builds for ${CURRENT_TRIGGER}/${CURRENT_BRANCH}"
  exit 0
fi

while IFS= read -r build_id; do
  [[ -n "$build_id" ]] || continue
  echo "Canceling superseded build ${build_id} for ${CURRENT_TRIGGER}/${CURRENT_BRANCH}"
  gcloud builds cancel "$build_id" \
    --project="$PROJECT" \
    --region="$REGION" \
    --quiet || true
done <<< "$superseded_ids"
