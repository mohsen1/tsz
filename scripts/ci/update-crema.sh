#!/usr/bin/env bash
# Update CREMA autoscaler config without rebuilding the runner image.
# Usage: scripts/ci/update-crema.sh [--min N] [--max N] [--poll-interval N]
set -Eeuo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

PROJECT_ID="${PROJECT_ID:-$(gcloud config get-value project 2>/dev/null)}"
REGION="${REGION:-us-central1}"
REPO="${GITHUB_REPO:-mohsen1/tsz}"
POOL_NAME="${POOL_NAME:-tsz-gh-runner}"
SERVICE_NAME="${CREMA_SERVICE_NAME:-tsz-gh-runner-crema}"
SERVICE_ACCOUNT_NAME="${SERVICE_ACCOUNT_NAME:-tsz-gh-runners}"
SERVICE_ACCOUNT="${SERVICE_ACCOUNT_NAME}@${PROJECT_ID}.iam.gserviceaccount.com"
SECRET_NAME="${SECRET_NAME:-github_runner_token}"
PARAMETER_ID="${PARAMETER_ID:-tsz-gh-runner-crema-config}"
PARAMETER_REGION="${PARAMETER_REGION:-global}"
RUNNER_LABELS="${RUNNER_LABELS:-tsz-cloud-run}"

# Defaults tuned for fast CI (spend credits freely):
#   min=30 pre-warms runners so PRs pick up immediately
#   targetQueueLength=1 triggers scale-up as soon as one job queues
#   scaleUp=30/10s gets us +60 runners in 20s under burst load
#   scaleDown is intentionally slow: Cloud Run runner instances can be running
#   jobs that have already left GitHub's queue, so aggressive downscale can kill
#   active CI/bench jobs without useful logs.
MIN_REPLICAS="${MIN_REPLICAS:-30}"
MAX_REPLICAS="${MAX_REPLICAS:-200}"
TARGET_QUEUE_LENGTH="${TARGET_QUEUE_LENGTH:-1}"
POLLING_INTERVAL="${POLLING_INTERVAL:-10}"
SCALE_UP_VALUE="${SCALE_UP_VALUE:-30}"
SCALE_UP_PERIOD="${SCALE_UP_PERIOD:-10}"
SCALE_DOWN_WINDOW="${SCALE_DOWN_WINDOW:-1800}"
SCALE_DOWN_VALUE="${SCALE_DOWN_VALUE:-1}"
SCALE_DOWN_PERIOD="${SCALE_DOWN_PERIOD:-300}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --min)           MIN_REPLICAS="$2"; shift 2 ;;
    --max)           MAX_REPLICAS="$2"; shift 2 ;;
    --poll-interval) POLLING_INTERVAL="$2"; shift 2 ;;
    *)               echo "Unknown arg: $1" >&2; exit 1 ;;
  esac
done

owner="${REPO%%/*}"
repo_name="${REPO#*/}"

tmp_config="$(mktemp)"
trap 'rm -f "$tmp_config"' EXIT

cat > "$tmp_config" <<YAML
apiVersion: crema/v1
kind: CremaConfig
metadata:
  name: tsz-gh-runners
spec:
  pollingInterval: ${POLLING_INTERVAL}
  triggerAuthentications:
    - metadata:
        name: github-trigger-auth
      spec:
        gcpSecretManager:
          secrets:
            - parameter: personalAccessToken
              id: ${SECRET_NAME}
              version: latest
  scaledObjects:
    - spec:
        minReplicaCount: ${MIN_REPLICAS}
        maxReplicaCount: ${MAX_REPLICAS}
        scaleTargetRef:
          name: projects/${PROJECT_ID}/locations/${REGION}/workerpools/${POOL_NAME}
        triggers:
          - type: github-runner
            name: tsz-gh-runner
            metadata:
              owner: ${owner}
              runnerScope: repo
              repos: ${repo_name}
              labels: ${RUNNER_LABELS}
              targetWorkflowQueueLength: "${TARGET_QUEUE_LENGTH}"
            authenticationRef:
              name: github-trigger-auth
        advanced:
          horizontalPodAutoscalerConfig:
            behavior:
              scaleDown:
                stabilizationWindowSeconds: ${SCALE_DOWN_WINDOW}
                policies:
                  - type: Pods
                    value: ${SCALE_DOWN_VALUE}
                    periodSeconds: ${SCALE_DOWN_PERIOD}
              scaleUp:
                stabilizationWindowSeconds: 0
                policies:
                  - type: Pods
                    value: ${SCALE_UP_VALUE}
                    periodSeconds: ${SCALE_UP_PERIOD}
YAML

echo "=== New CREMA config ==="
cat "$tmp_config"
echo ""

version="$(date +%Y%m%d%H%M%S)"
echo "Writing parameter version ${version} ..."
gcloud parametermanager parameters versions create "$version" \
  --project="$PROJECT_ID" \
  --location="$PARAMETER_REGION" \
  --parameter="$PARAMETER_ID" \
  --payload-data-from-file="$tmp_config"

crema_config="projects/${PROJECT_ID}/locations/${PARAMETER_REGION}/parameters/${PARAMETER_ID}/versions/${version}"
echo "Redeploying CREMA with config ${crema_config} ..."
gcloud run services update "$SERVICE_NAME" \
  --project="$PROJECT_ID" \
  --region="$REGION" \
  --update-env-vars="CREMA_CONFIG=${crema_config}"

echo ""
echo "Done. CREMA will now:"
echo "  • Keep >= ${MIN_REPLICAS} runners warm at all times"
echo "  • Scale up to ${MAX_REPLICAS} max"
echo "  • Add up to ${SCALE_UP_VALUE} runners per ${SCALE_UP_PERIOD}s (no stabilization window)"
echo "  • Remove up to ${SCALE_DOWN_VALUE} runner(s) per ${SCALE_DOWN_PERIOD}s after ${SCALE_DOWN_WINDOW}s stabilization"
