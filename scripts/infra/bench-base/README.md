# Benchmark base image (`tsz-bench-base`)

A prebuilt Cloud Build image that bakes the benchmark bootstrap toolchain
(apt packages, Node 22, pnpm, `llvm-tools-preview`) on top of `rust:1.95`.

## Why

`scripts/cloudbuild/cloudbuild-bench-prepare.yaml` and
`scripts/cloudbuild/cloudbuild-bench-shard.yaml` historically ran on the stock
`rust:1.95` image and re-installed the same bootstrap at the start of **every**
build: `apt-get install` (hyperfine/jq/xz-utils/…), a Node tarball from
`nodejs.org`, and `npm install -g pnpm`. With 1 prepare + 9 shards that is 10
redundant installs per bench run, and each is an external-mirror network single
point of failure — a `nodejs.org` or Debian-mirror blip reds a shard, which can
drop a run below the 9-shard publish gate (issue #13751).

Baking the toolchain into an image once removes both the redundancy and the
SPOF, and (because the libraries land in the image filesystem) avoids the
relocatability problems of trying to copy apt binaries between containers.

## Behavior-preserving by default

This is a **zero-behavior-change-by-default** rollout:

- The bench configs take a `_BENCH_IMAGE` substitution that **defaults to
  `rust:1.95`**, so with no configuration the bench builds run exactly as before.
- The inline bootstrap in both configs is guarded by `command -v` checks, so it
  runs on the stock image (tools absent) and is skipped on the baked image
  (tools present). The baked apt/Node/pnpm toolchain matches the inline
  bootstrap, so switching over does not change any benchmark measurement — it
  only removes the bootstrap step. (`llvm-tools-preview` is baked too, matching
  the prepare config's `rustup component add` step.)

## Build

The `bench-image` GitHub workflow (`.github/workflows/bench-image.yml`) builds
and pushes the image. It is **not** a required check and cannot affect PR CI. It
no-ops until a maintainer sets these repo variables:

| Repo variable               | Required | Purpose                                                |
| --------------------------- | -------- | ------------------------------------------------------ |
| `BENCH_IMAGE_GCP_PROJECT`   | yes      | GCP project that hosts the image registry.             |
| `BENCH_IMAGE_REPO`          | yes      | Image repo path, e.g. `us-central1-docker.pkg.dev/<project>/<repo>/tsz-bench-base`. |
| `BENCH_IMAGE_GCP_REGION`    | no       | Cloud Build region (defaults to `us-central1`).        |

Trigger: manual `workflow_dispatch` only while emergency GCP cost scale-down is
active. Re-enable push or scheduled rebuilds only after benchmark Cloud Build
spend is intentionally budgeted again. Every build is tagged with the commit SHA
(provenance) and `:latest`.

## Switch the bench builds over

After the image exists in the registry, set one more repo variable:

| Repo variable      | Purpose                                                            |
| ------------------ | ----------------------------------------------------------------- |
| `BENCH_IMAGE_REF`  | Image reference the bench configs run on, e.g. `<repo>:latest` or a pinned `<repo>:<sha>`. Defaults to `rust:1.95` when unset. |

`.github/workflows/bench.yml` passes `BENCH_IMAGE_REF` to the prepare and shard
Cloud Build submits as the `_BENCH_IMAGE` substitution. Pinning a SHA tag is
recommended for reproducibility; `:latest` tracks the newest build.

To roll back, clear `BENCH_IMAGE_REF`: the next bench run falls back to
`rust:1.95` and the inline bootstrap with no other change.
