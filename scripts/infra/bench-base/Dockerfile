# Prebuilt base image for the benchmark prepare/shard Cloud Builds.
#
# Today cloudbuild-bench-prepare.yaml and cloudbuild-bench-shard.yaml run on the
# stock `rust:1.95` image and re-install the same bench bootstrap (apt packages,
# Node, pnpm) at the start of EVERY build — 1 prepare + 9 shards = 10 redundant
# installs per bench run, each an external-mirror network SPOF (a nodejs.org /
# Debian-mirror blip reds a shard and drops below the 9-shard publish gate; see
# issue #13751). Baking those tools here once, at image-build time, removes both
# the redundancy and the SPOF: the libraries land in the image filesystem, so
# there is no relocatability problem and no critical-path download.
#
# The apt/Node/pnpm bootstrap below is kept identical to the inline bootstrap in
# the two cloudbuild configs so switching a build over to this image does not
# change any benchmark measurement — it only removes the bootstrap step. Keep the
# versions in sync with those files. `llvm-tools-preview` is additionally baked
# (matching the prepare config's `rustup component add` step) as a harmless
# superset so prepare needs no rustup download either.
FROM rust:1.95

ENV DEBIAN_FRONTEND=noninteractive

# One layer: install, then sanity-check the toolchain so a broken image fails the
# build instead of reaching a bench run.
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
      bc \
      ca-certificates \
      curl \
      git \
      hyperfine \
      jq \
      pkg-config \
      xz-utils \
    && rm -rf /var/lib/apt/lists/* \
    && node_version="22.13.1" \
    && node_arch="x64" \
    && curl -fsSL "https://nodejs.org/dist/v${node_version}/node-v${node_version}-linux-${node_arch}.tar.xz" \
       | tar -xJ -C /usr/local --strip-components=1 \
    && npm install -g pnpm --silent \
    && rustup component add llvm-tools-preview \
    && cargo --version \
    && node --version \
    && pnpm --version \
    && hyperfine --version \
    && jq --version
