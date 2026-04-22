# GCP CI

CI now runs through Google Cloud Build instead of GitHub Actions.

The repository entrypoints are the `cloudbuild*.yaml` files, which restore
shared caches with `scripts/ci/gcp-cache.sh`, run `scripts/ci/gcp-full-ci.sh`,
save updated caches, then report the original suite status. Main conformance
uses 224-vCPU N2D workers, while PR conformance uses the existing C3 highcpu
pool to avoid waiting on the same large N2D machine shape. The other suites stay
smaller so one full PR run fits under the current private-pool CPU quota:

```text
main conformance  tsz-ci-n2d-224     n2d-highcpu-224
PR conformance    tsz-ci-c3-88       c3-highcpu-176
emit              tsz-ci-n2d-96      n2d-highcpu-96
fourslash         tsz-ci-n2d-96      n2d-highcpu-96
unit              tsz-ci-n2d-64      n2d-highcpu-64
lint              tsz-ci-n2d-48      n2d-highcpu-48
wasm              tsz-ci-n2d-48      n2d-highcpu-48
```

The script
keeps the old CI gates: Rust formatting, metadata guardrails,
clippy, nextest, WASM build, conformance, emit, fourslash, and snapshot
regression checks. Conformance is CPU- and memory-capped, defaulting to at most
128 workers and about one worker per 2GB of RAM, because over-filling a highcpu
machine with conformance batch workers can be slower than leaving headroom. Emit
and fourslash default to 4 shards. Emit uses up to 32 workers per shard with a
30s per-test timeout, while fourslash uses up to 16 workers per shard to avoid
crashing the Node worker pool before it can record shard results.

Triggers set `_TSZ_CI_SUITE` so GitHub shows one check per category:
`lint`, `unit`, `wasm`, `conformance`, `emit`, and `fourslash`. Running without
that substitution keeps the `all` default for ad hoc full builds.

Every build writes a sanitized markdown digest to
`.ci-status/check-summary.md` and prints it from the final Cloud Build step.
The summary is intentionally compact enough for GitHub Checks output:
conformance includes aggregate counts, current failure cases, regression
signals, and top diagnostic-code buckets; emit includes aggregate counts,
timeouts, and failed baselines; fourslash includes shard totals and failed
cases from the current run.

Cloud Build owns the check runs that appear in GitHub, so repository code cannot
directly edit the Google Cloud Build app's check-run markdown. Do not put a
GitHub token or GitHub App key into the PR build steps to work around that; this
is an open source repository and PR code can change the build config. To expose
the digest publicly, run a trusted publisher outside the PR build, for example a
Cloud Run service subscribed to Cloud Build events. That publisher can read the
printed digest or a stored artifact, then create or update a sibling GitHub
Check Run or PR comment with only `.ci-status/check-summary.md`.

Builds use `queueTtl: 7200s`, so a build can survive Cloud Build private-pool
cold starts and quota scheduling. Each build also best-effort cancels older
queued or running builds for the same trigger and branch before restoring cache,
and cancels itself if a newer build already exists for that trigger and branch.
That keeps rapid pushes from spending workers on obsolete commits.

Cloud Build source archives do not preserve git submodule metadata, so
`scripts/ci/typescript-submodule-ref` records the TypeScript submodule commit
used when a git checkout is unavailable. If the TypeScript submodule is bumped,
update that file in the same change.

The first Cloud Build step restores cache archives from GCS:

```text
gs://thirdface-ai-oauth_cloudbuild/tsz-ci-cache/typescript/<sha>.tar.gz
gs://thirdface-ai-oauth_cloudbuild/tsz-ci-cache/cargo-home/<Cargo.lock hash>.tar.gz
gs://thirdface-ai-oauth_cloudbuild/tsz-ci-cache/npm/<scripts deps hash>.tar.gz
gs://thirdface-ai-oauth_cloudbuild/tsz-ci-cache/scripts-node-modules/<scripts deps hash>.tar.gz
gs://thirdface-ai-oauth_cloudbuild/tsz-ci-cache/typescript-harness/<sha>.tar.gz
gs://thirdface-ai-oauth_cloudbuild/tsz-ci-cache/typescript-node-modules/<sha>-<TypeScript deps hash>.tar.gz
gs://thirdface-ai-oauth_cloudbuild/tsz-ci-cache/dist-fast/<commit sha>.tar.gz
```

On a TypeScript source miss, Cloud Build downloads the GitHub source archive for
the pinned commit, writes `TypeScript/.tsz-cache-ref`, and uploads the tarball
for later runs. The main CI step accepts that source-only tree and avoids a git
submodule clone.

The other caches cover Cargo registry/git state, npm download state,
`scripts/node_modules`, the built fourslash harness under `TypeScript/built`,
`TypeScript/node_modules`, and dist-fast binaries for repeated jobs on the same
commit. Cache saving runs after the suite command even when that command fails,
then the final Cloud Build step exits with the original suite status.

Create the private pool before running builds or creating triggers:

```bash
gcloud builds worker-pools create tsz-ci-n2d-224 \
  --project=thirdface-ai-oauth \
  --region=us-central1 \
  --worker-machine-type=n2d-highcpu-224 \
  --worker-disk-size=200GB

gcloud builds worker-pools create tsz-ci-c3-88 \
  --project=thirdface-ai-oauth \
  --region=us-central1 \
  --worker-machine-type=c3-highcpu-176 \
  --worker-disk-size=200GB

gcloud builds worker-pools create tsz-ci-n2d-96 \
  --project=thirdface-ai-oauth \
  --region=us-central1 \
  --worker-machine-type=n2d-highcpu-96 \
  --worker-disk-size=200GB

gcloud builds worker-pools create tsz-ci-n2d-48 \
  --project=thirdface-ai-oauth \
  --region=us-central1 \
  --worker-machine-type=n2d-highcpu-48 \
  --worker-disk-size=200GB

gcloud builds worker-pools create tsz-ci-n2d-64 \
  --project=thirdface-ai-oauth \
  --region=us-central1 \
  --worker-machine-type=n2d-highcpu-64 \
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
pr_config_for_suite() {
  case "$1" in
    lint|wasm) printf '%s\n' cloudbuild.n2d-48.yaml ;;
    unit) printf '%s\n' cloudbuild.n2d-64.yaml ;;
    emit|fourslash) printf '%s\n' cloudbuild.n2d-96.yaml ;;
    conformance) printf '%s\n' cloudbuild.pr-conformance.yaml ;;
    *) printf '%s\n' cloudbuild.yaml ;;
  esac
}

for suite in lint unit wasm conformance emit fourslash; do
  config="$(pr_config_for_suite "$suite")"
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
main_config_for_suite() {
  case "$1" in
    lint|wasm) printf '%s\n' cloudbuild.n2d-48.yaml ;;
    unit) printf '%s\n' cloudbuild.n2d-64.yaml ;;
    emit|fourslash) printf '%s\n' cloudbuild.n2d-96.yaml ;;
    conformance) printf '%s\n' cloudbuild.yaml ;;
    *) printf '%s\n' cloudbuild.yaml ;;
  esac
}

for suite in lint unit wasm conformance emit fourslash; do
  config="$(main_config_for_suite "$suite")"
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

gcloud projects add-iam-policy-binding thirdface-ai-oauth \
  --member=serviceAccount:135226528921-compute@developer.gserviceaccount.com \
  --role=roles/cloudbuild.builds.editor \
  --condition=None
```
