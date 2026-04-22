# GCP CI

CI now runs through Google Cloud Build instead of GitHub Actions.

The repository entrypoint is `cloudbuild.yaml`, which runs
`scripts/ci/gcp-full-ci.sh` on a `c3-highcpu-88` Cloud Build private-pool
worker. The script keeps the old CI gates: Rust formatting, metadata guardrails,
clippy, nextest, WASM build, conformance, emit, fourslash, and snapshot
regression checks. Conformance runs through the repository wrapper with 80
workers, while emit and fourslash default to 4 shards with 20 workers per shard.
That keeps process overhead lower on the 88-vCPU pool while still saturating the
larger machine.

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
gcloud builds worker-pools create tsz-ci-c3-88 \
  --project=thirdface-ai-oauth \
  --region=us-central1 \
  --worker-machine-type=c3-highcpu-88 \
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

Create the pull request trigger in the GCP project:

```bash
gcloud builds triggers create github \
  --project=thirdface-ai-oauth \
  --region=us-central1 \
  --name=tsz-pr-ci \
  --repository=projects/thirdface-ai-oauth/locations/us-central1/connections/tsz-github/repositories/tsz \
  --pull-request-pattern='^main$' \
  --comment-control=COMMENTS_DISABLED \
  --build-config=cloudbuild.yaml \
  --include-logs-with-status \
  --no-require-approval \
  --service-account=projects/thirdface-ai-oauth/serviceAccounts/135226528921-compute@developer.gserviceaccount.com
```

Create the main branch trigger:

```bash
gcloud builds triggers create github \
  --project=thirdface-ai-oauth \
  --region=us-central1 \
  --name=tsz-main-ci \
  --repository=projects/thirdface-ai-oauth/locations/us-central1/connections/tsz-github/repositories/tsz \
  --branch-pattern='^main$' \
  --build-config=cloudbuild.yaml \
  --include-logs-with-status \
  --no-require-approval \
  --service-account=projects/thirdface-ai-oauth/serviceAccounts/135226528921-compute@developer.gserviceaccount.com
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
