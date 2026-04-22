# GCP CI

CI now runs through Google Cloud Build instead of GitHub Actions.

The repository entrypoint is `cloudbuild.yaml`, which runs
`scripts/ci/gcp-full-ci.sh` on Cloud Build private-pool workers. The configured
suite pools use 592 of the 600 available private-pool CPUs in `us-central1`, so
all six suite checks can run concurrently without falling back into queueing:

```text
conformance  n2d-highcpu-224
emit         n2d-highcpu-128
fourslash    n2d-highcpu-128
unit         n2d-highcpu-48
lint         n2d-highcpu-32
wasm         n2d-highcpu-32
```

The script
keeps the old CI gates: Rust formatting, metadata guardrails,
clippy, nextest, WASM build, conformance, emit, fourslash, and snapshot
regression checks. Conformance defaults to up to 216 workers on the current
224-vCPU pool. Emit and fourslash default to 4 shards and compute workers per
shard from the detected CPU count, leaving a small reserve for system overhead.

Triggers set `_TSZ_CI_SUITE` so GitHub shows one check per category:
`lint`, `unit`, `wasm`, `conformance`, `emit`, and `fourslash`. Running without
that substitution keeps the `all` default for ad hoc full builds.

Builds use `queueTtl: 300s`, so a build that cannot start within 5 minutes is
expired instead of waiting indefinitely behind newer commits.

Cloud Build source archives do not preserve git submodule metadata, so
`scripts/ci/typescript-submodule-ref` records the TypeScript submodule commit
used when a git checkout is unavailable. If the TypeScript submodule is bumped,
update that file in the same change.

The first Cloud Build step restores `TypeScript/` from a GCS archive keyed by
that pinned commit:

```text
gs://thirdface-ai-oauth_cloudbuild/tsz-ci-cache/typescript/<sha>.tar.gz
```

On a miss, Cloud Build downloads the GitHub source archive for the pinned commit,
writes `TypeScript/.tsz-cache-ref`, and uploads the tarball for later runs. The
main CI step accepts that source-only tree and avoids a git submodule clone.

Create the private pool before running builds or creating triggers:

```bash
gcloud builds worker-pools create tsz-ci-n2d-224 \
  --project=thirdface-ai-oauth \
  --region=us-central1 \
  --worker-machine-type=n2d-highcpu-224 \
  --worker-disk-size=200GB

gcloud builds worker-pools create tsz-ci-n2d-128 \
  --project=thirdface-ai-oauth \
  --region=us-central1 \
  --worker-machine-type=n2d-highcpu-128 \
  --worker-disk-size=200GB

gcloud builds worker-pools create tsz-ci-n2d-48 \
  --project=thirdface-ai-oauth \
  --region=us-central1 \
  --worker-machine-type=n2d-highcpu-48 \
  --worker-disk-size=200GB

gcloud builds worker-pools create tsz-ci-n2d-32 \
  --project=thirdface-ai-oauth \
  --region=us-central1 \
  --worker-machine-type=n2d-highcpu-32 \
  --worker-disk-size=200GB
```

Connect the GitHub repository to Cloud Build once before creating triggers. If
the Cloud Build GitHub App is already installed, create the connection with its
installation ID and an authorizer token stored in Secret Manager. Otherwise,
start the browser authorization flow:

```bash
gcloud builds connections create github tsz-github \
  --project=thirdface-ai-oauth \
  --region=us-central1
```

Cloud Build will print the authorization and installation links. After the
connection reaches `COMPLETE`, add the repository:

```bash
gcloud builds repositories create tsz \
  --project=thirdface-ai-oauth \
  --region=us-central1 \
  --connection=tsz-github \
  --remote-uri=https://github.com/mohsen1/tsz.git
```

Create one pull request trigger per suite in the GCP project:

```bash
pool_for_suite() {
  case "$1" in
    lint|wasm) printf '%s\n' cloudbuild.n2d-32.yaml ;;
    unit) printf '%s\n' cloudbuild.n2d-48.yaml ;;
    emit|fourslash) printf '%s\n' cloudbuild.n2d-128.yaml ;;
    conformance) printf '%s\n' cloudbuild.yaml ;;
    *) printf '%s\n' cloudbuild.yaml ;;
  esac
}

for suite in lint unit wasm conformance emit fourslash; do
  config="$(pool_for_suite "$suite")"
  gcloud builds triggers create github \
    --project=thirdface-ai-oauth \
    --region=us-central1 \
    --name="tsz-pr-${suite}" \
    --repository=projects/thirdface-ai-oauth/locations/us-central1/connections/tsz-github/repositories/tsz \
    --pull-request-pattern='^main$' \
    --comment-control=COMMENTS_DISABLED \
    --build-config="$config" \
    --include-logs-with-status \
    --no-require-approval \
    --substitutions="_TSZ_CI_SUITE=${suite}" \
    --service-account=projects/thirdface-ai-oauth/serviceAccounts/135226528921-compute@developer.gserviceaccount.com
done
```

Create one main branch trigger per suite:

```bash
for suite in lint unit wasm conformance emit fourslash; do
  config="$(pool_for_suite "$suite")"
  gcloud builds triggers create github \
    --project=thirdface-ai-oauth \
    --region=us-central1 \
    --name="tsz-main-${suite}" \
    --repository=projects/thirdface-ai-oauth/locations/us-central1/connections/tsz-github/repositories/tsz \
    --branch-pattern='^main$' \
    --build-config="$config" \
    --include-logs-with-status \
    --no-require-approval \
    --substitutions="_TSZ_CI_SUITE=${suite}" \
    --service-account=projects/thirdface-ai-oauth/serviceAccounts/135226528921-compute@developer.gserviceaccount.com
done
```

No GitHub repository secrets are required for this path. Cloud Build owns the
GitHub integration and posts build status back to GitHub.

The trigger service account needs to read/write the TypeScript cache and write
logs:

```bash
gcloud storage buckets add-iam-policy-binding gs://thirdface-ai-oauth_cloudbuild \
  --member=serviceAccount:135226528921-compute@developer.gserviceaccount.com \
  --role=roles/storage.objectAdmin

gcloud projects add-iam-policy-binding thirdface-ai-oauth \
  --member=serviceAccount:135226528921-compute@developer.gserviceaccount.com \
  --role=roles/logging.logWriter \
  --condition=None
```
