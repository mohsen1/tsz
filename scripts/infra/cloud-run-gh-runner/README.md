# Cloud Run self-hosted GitHub Actions runner

This directory holds the image and entrypoint for the self-hosted GitHub
Actions runner fleet that backs jobs labeled `[self-hosted, tsz-cloud-run]`.

| File | Purpose |
| --- | --- |
| `Dockerfile` | Runner image: `ghcr.io/actions/actions-runner:2.334.0` base + Rust toolchain, `nextest`, `wasm-pack`, `pnpm`, `gcloud` SDK, and build deps. |
| `start.sh` | Entrypoint: registers an **ephemeral** runner, runs one job, deregisters, and clears local state so a reused Cloud Run instance never reuses credentials. |
| `cloud-run-service.yaml` | Version-controlled scaling-policy skeleton (IaC reference; **not** auto-applied). |

## Why this exists

The runner image and its Cloud Run scaling policy previously lived only as
opaque GCP-side state — there was no in-repo build/push pipeline, no provenance
trail of what was actually deployed, and no reviewable scaler config. See
issue #13750.

## Runner version pin and the auto-update gap

- The runner version is pinned to **`2.334.0`** in `Dockerfile:1`.
- Auto-update is **disabled**: `start.sh` sets `DISABLE_RUNNER_UPDATE=1` and
  passes `--disableupdate` to `config.sh`. This keeps the toolchain stable but
  means the fleet does **not** follow GitHub's runner-version floor on its own.
- **Risk:** when GitHub raises its required minimum runner version, a pinned +
  auto-update-disabled fleet can hard-fail registration en masse with no in-repo
  remediation. Bumping the `FROM` tag here and rebuilding/redeploying is the
  remediation.
- **TODO (version-floor alert):** extend the existing `runner-health` job in
  `.github/workflows/ci-health.yml` to compare the deployed runner version
  against GitHub's current required minimum and warn *before* the fleet
  hard-fails. Not implemented here to keep this change scoped to the
  build/deploy pipeline and avoid editing the health workflow. Tracked under
  issue #13750.

## Build / deploy pipeline

`.github/workflows/runner-image.yml` builds (and optionally deploys) the image:

- **Triggers:** `workflow_dispatch`, `push` touching the `Dockerfile`,
  `start.sh`, the cloudbuild config, or the workflow itself, and a weekly cron
  (build-only, for base-image security updates).
- **Build** (`build` job): submits
  `scripts/cloudbuild/cloudbuild-runner-image.yaml` via `gcloud builds submit`,
  producing an image tagged with the commit SHA (provenance) **and** `latest`.
- **Deploy** (`deploy` job): runs **only** on a manual `workflow_dispatch` with
  the `deploy` input set to `true`, and is further gated by the
  `runner-image-deploy` environment (add required reviewers for manual
  approval). It updates the Cloud Run service to the SHA-tagged image plus the
  scaling flags from `cloud-run-service.yaml`; existing env/secret bindings on
  the service are preserved.

This workflow is **separate from CI**, is **not** a required check, and cannot
affect PR status. If the GCP configuration below is absent, the build job
**no-ops with a notice** instead of failing.

### Required GCP configuration (repo variables)

A maintainer must set these repository **variables** (Settings → Secrets and
variables → Actions → Variables) before the workflow does anything:

| Variable | Required for | Example / default |
| --- | --- | --- |
| `RUNNER_IMAGE_GCP_PROJECT` | build | `tsz-ci` |
| `RUNNER_IMAGE_REPO` | build | `us-central1-docker.pkg.dev/tsz-ci/runners/tsz-cloud-run-runner` |
| `RUNNER_IMAGE_GCP_REGION` | build/deploy | defaults to `us-central1` |
| `RUNNER_IMAGE_CLOUD_RUN_SERVICE` | deploy | the Cloud Run service name |
| `RUNNER_IMAGE_MIN_INSTANCES` | deploy | defaults to `0` (scale-to-zero) |
| `RUNNER_IMAGE_MAX_INSTANCES` | deploy | defaults to `50` |

Authentication uses the **ambient gcloud credentials** already present on the
self-hosted runners (the same mechanism `ci.yml` / `bench.yml` rely on for
`gcloud builds submit`); no extra service-account secret is wired here. The
runner registration PAT (`GITHUB_TOKEN`) is consumed by `start.sh` at container
start and should be supplied to the Cloud Run service via Secret Manager — see
`cloud-run-service.yaml`.

### Manual deploy

```bash
gh workflow run runner-image.yml -f deploy=true -f reason="bump runner to <ver>"
# then approve the runner-image-deploy environment when prompted
```

## Scaling policy (IaC)

`cloud-run-service.yaml` captures the intended scaling policy
(`concurrency=1`, `minScale=0`, `maxScale=50`, `timeoutSeconds=3600`) as a
reviewable skeleton. It is **not** applied automatically. Before adopting it as
the source of truth, reconcile it with the live service
(`gcloud run services describe <service> --format export`) and wire the
`GITHUB_TOKEN` secret reference. Treat any `minScale>0` warm pool as a measured
cost decision (per-job queue-wait data, #13715), since `minScale>0` defeats
scale-to-zero.

## Related follow-ups (issue #13750)

- **Wire `scripts/ci/cleanup-stale-runners.sh` into the ci-health cron.** It
  safely removes only `offline` runner registrations but is currently
  detection-only (`ci-health.yml` merely prints a suggestion to run it), so
  stale ephemeral registrations accumulate until a human runs it by hand.
- **Add the runner-version-floor check** described above to `runner-health`.
- **Make the low-runner-count health response actionable** rather than
  observe-only.
