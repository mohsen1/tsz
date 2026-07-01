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
- **Version-floor alert (implemented):** the `runner-health` job in
  `.github/workflows/ci-health.yml` reads this pin (auto-update is disabled, so
  the pin equals the deployed version), compares it against the latest
  `actions/runner` release, and warns *before* the fleet hard-fails — a loud
  annotation once the pin trails by `RUNNER_VERSION_FLOOR_GAP` minor releases
  (default `10`), an informational summary line for a smaller lag.

## Build / deploy pipeline

`.github/workflows/runner-image.yml` builds (and optionally deploys) the image.
During emergency GCP cost scale-down it is manual-only:

- **Triggers:** `workflow_dispatch` only. Re-enable push or scheduled rebuilds
  only after the Cloud Run runner fleet is intentionally budgeted again.
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
| `RUNNER_IMAGE_MAX_INSTANCES` | deploy | defaults to `1` |

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
(`concurrency=1`, `minScale=0`, `maxScale=1`, `timeoutSeconds=3600`) as a
reviewable skeleton. It is **not** applied automatically. Before adopting it as
the source of truth, reconcile it with the live service
(`gcloud run services describe <service> --format export`) and wire the
`GITHUB_TOKEN` secret reference. Treat any `minScale>0` warm pool as a measured
cost decision (per-job queue-wait data, #13715), since `minScale>0` defeats
scale-to-zero. Raising `maxScale` above `1` is also a measured cost decision:
it can multiply active 8-vCPU / 32 GiB runner spend during CI bursts.

## Health automation (issue #13750)

The `runner-health` job in `.github/workflows/ci-health.yml` (cron every 15
minutes) now closes the in-repo follow-ups that previously lived here:

- **Stale-offline purge.** It runs `scripts/ci/cleanup-stale-runners.sh`
  automatically when offline registrations are present, instead of only
  printing a suggestion to run it by hand. The script deletes **only** `offline`
  runners, so it cannot disrupt in-flight jobs. Deleting registrations needs
  repo administration, which the default `GITHUB_TOKEN` cannot grant, so the
  step prefers `TSZ_PR_AUTOMATION_TOKEN` and degrades to a warning if no token
  has the scope.
- **Runner-version-floor check.** See the section above.
- **Actionable low-runner-count signal.** Saturation and low-count conditions
  now emit `::warning::` annotations (surfaced in the Actions UI / notifications)
  in addition to the step-summary lines.

## Remaining infra follow-ups (issue #13750, need GCP access)

- A dedicated build pool so a bench burst cannot throttle the required
  merge-queue submit (see #13751), and baking the runner image's apt/Node/pnpm
  bootstrap into a prebuilt image to remove the per-shard network SPOF.
