#!/usr/bin/env bash
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
RUNNER_CPU="${RUNNER_CPU:-16}"
RUNNER_MEMORY="${RUNNER_MEMORY:-64Gi}"
RUNNER_INSTANCES="${RUNNER_INSTANCES:-1}"
BUILD_POOL="${BUILD_POOL:-projects/${PROJECT_ID}/locations/${REGION}/workerPools/tsz-ci-n2d-224}"
BUILD_MODE="${BUILD_MODE:-local}"
IMAGE_TAG="${IMAGE_TAG:-${REGION}-docker.pkg.dev/${PROJECT_ID}/cloud-run-source-deploy/tsz-gh-runner:latest}"
TARGET_QUEUE_LENGTH="${TARGET_QUEUE_LENGTH:-1}"
POLLING_INTERVAL="${POLLING_INTERVAL:-10}"
CREMA_IMAGE="${CREMA_IMAGE:-us-central1-docker.pkg.dev/cloud-run-oss-images/crema-v1/autoscaler:1.0}"
CREMA_BASE_IMAGE="${CREMA_BASE_IMAGE:-us-central1-docker.pkg.dev/serverless-runtimes/google-24/runtimes/java25}"

if [[ -z "$PROJECT_ID" ]]; then
  echo "PROJECT_ID is required" >&2
  exit 2
fi

if [[ -z "${GITHUB_TOKEN:-}" ]]; then
  if command -v gh >/dev/null 2>&1; then
    GITHUB_TOKEN="$(gh auth token)"
  fi
fi
if [[ -z "${GITHUB_TOKEN:-}" ]]; then
  echo "GITHUB_TOKEN is required, or gh must be authenticated" >&2
  exit 2
fi

owner="${REPO%%/*}"
repo_name="${REPO#*/}"

echo "Enabling required APIs in ${PROJECT_ID}"
gcloud services enable \
  artifactregistry.googleapis.com \
  cloudbuild.googleapis.com \
  run.googleapis.com \
  secretmanager.googleapis.com \
  parametermanager.googleapis.com \
  monitoring.googleapis.com \
  --project="$PROJECT_ID"

if ! gcloud iam service-accounts describe "$SERVICE_ACCOUNT" --project="$PROJECT_ID" >/dev/null 2>&1; then
  gcloud iam service-accounts create "$SERVICE_ACCOUNT_NAME" \
    --project="$PROJECT_ID" \
    --display-name="TSZ GitHub Actions Cloud Run runners"
fi

if gcloud secrets describe "$SECRET_NAME" --project="$PROJECT_ID" >/dev/null 2>&1; then
  printf '%s' "$GITHUB_TOKEN" | gcloud secrets versions add "$SECRET_NAME" --project="$PROJECT_ID" --data-file=- >/dev/null
else
  printf '%s' "$GITHUB_TOKEN" | gcloud secrets create "$SECRET_NAME" --project="$PROJECT_ID" --data-file=- >/dev/null
fi

gcloud secrets add-iam-policy-binding "$SECRET_NAME" \
  --project="$PROJECT_ID" \
  --member="serviceAccount:${SERVICE_ACCOUNT}" \
  --role="roles/secretmanager.secretAccessor" >/dev/null

gcloud projects add-iam-policy-binding "$PROJECT_ID" \
  --member="serviceAccount:${SERVICE_ACCOUNT}" \
  --role="roles/monitoring.metricWriter" >/dev/null

gcloud projects add-iam-policy-binding "$PROJECT_ID" \
  --member="serviceAccount:${SERVICE_ACCOUNT}" \
  --role="roles/iam.serviceAccountUser" >/dev/null

gcloud projects add-iam-policy-binding "$PROJECT_ID" \
  --member="serviceAccount:${SERVICE_ACCOUNT}" \
  --role="roles/run.viewer" >/dev/null

# Cache access for scripts/ci/gcp-cache.sh.
gcloud projects add-iam-policy-binding "$PROJECT_ID" \
  --member="serviceAccount:${SERVICE_ACCOUNT}" \
  --role="roles/storage.objectAdmin" >/dev/null

echo "Building runner image ${IMAGE_TAG} (${BUILD_MODE})"
case "$BUILD_MODE" in
  local)
    gcloud auth configure-docker "${REGION}-docker.pkg.dev" --quiet
    docker buildx build --platform linux/amd64 --push -t "$IMAGE_TAG" scripts/infra/cloud-run-gh-runner
    ;;
  cloudbuild)
    gcloud builds submit scripts/infra/cloud-run-gh-runner \
      --project="$PROJECT_ID" \
      --region="$REGION" \
      --worker-pool="$BUILD_POOL" \
      --tag="$IMAGE_TAG" \
      --timeout=3600s
    ;;
  skip)
    echo "Skipping image build; reusing ${IMAGE_TAG}"
    ;;
  *)
    echo "unsupported BUILD_MODE=${BUILD_MODE}; use local, cloudbuild, or skip" >&2
    exit 2
    ;;
esac

echo "Deploying Cloud Run worker pool ${POOL_NAME}"
gcloud beta run worker-pools deploy "$POOL_NAME" \
  --project="$PROJECT_ID" \
  --region="$REGION" \
  --image="$IMAGE_TAG" \
  --instances="$RUNNER_INSTANCES" \
  --set-env-vars="GITHUB_REPO=${REPO},RUNNER_LABELS=${RUNNER_LABELS},RUNNER_PREFIX=tsz-cr" \
  --set-secrets="GITHUB_TOKEN=${SECRET_NAME}:latest" \
  --service-account="$SERVICE_ACCOUNT" \
  --memory="$RUNNER_MEMORY" \
  --cpu="$RUNNER_CPU"

gcloud beta run worker-pools add-iam-policy-binding "$POOL_NAME" \
  --project="$PROJECT_ID" \
  --region="$REGION" \
  --member="serviceAccount:${SERVICE_ACCOUNT}" \
  --role="roles/run.developer" >/dev/null

tmp_config="$(mktemp)"
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
                stabilizationWindowSeconds: 30
                policies:
                  - type: Pods
                    value: 100
                    periodSeconds: 10
              scaleUp:
                stabilizationWindowSeconds: 0
                policies:
                  - type: Pods
                    value: 20
                    periodSeconds: 10
YAML

if ! gcloud parametermanager parameters describe "$PARAMETER_ID" --project="$PROJECT_ID" --location="$PARAMETER_REGION" >/dev/null 2>&1; then
  gcloud parametermanager parameters create "$PARAMETER_ID" \
    --project="$PROJECT_ID" \
    --location="$PARAMETER_REGION" \
    --parameter-format=YAML >/dev/null
fi

version="$(date +%Y%m%d%H%M%S)"
gcloud parametermanager parameters versions create "$version" \
  --project="$PROJECT_ID" \
  --location="$PARAMETER_REGION" \
  --parameter="$PARAMETER_ID" \
  --payload-data-from-file="$tmp_config" >/dev/null

gcloud projects add-iam-policy-binding "$PROJECT_ID" \
  --member="serviceAccount:${SERVICE_ACCOUNT}" \
  --role="roles/parametermanager.parameterViewer" >/dev/null

crema_config="projects/${PROJECT_ID}/locations/${PARAMETER_REGION}/parameters/${PARAMETER_ID}/versions/${version}"
echo "Deploying CREMA autoscaler ${SERVICE_NAME} with ${crema_config}"
gcloud run deploy "$SERVICE_NAME" \
  --project="$PROJECT_ID" \
  --image="$CREMA_IMAGE" \
  --region="$REGION" \
  --service-account="$SERVICE_ACCOUNT" \
  --no-allow-unauthenticated \
  --no-cpu-throttling \
  --base-image="$CREMA_BASE_IMAGE" \
  --labels=created-by=crema,app=tsz-gh-runner \
  --set-env-vars="CREMA_CONFIG=${crema_config},OUTPUT_SCALER_METRICS=True"

echo "Cloud Run GitHub runner pool ready: ${POOL_NAME} (${RUNNER_LABELS})"
