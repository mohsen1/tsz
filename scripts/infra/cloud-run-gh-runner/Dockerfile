FROM ghcr.io/actions/actions-runner:2.334.0

USER root

ENV DEBIAN_FRONTEND=noninteractive \
    CARGO_HOME=/home/runner/.cargo \
    RUSTUP_HOME=/home/runner/.rustup \
    PATH=/home/runner/.cargo/bin:/google-cloud-sdk/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin

# All runtime tools are pinned and baked here so a rust_compile/unit/wasm job
# does not download them from a GitHub release on the critical path. Keep
# SCCACHE_VER / NEXTEST_VER / WASM_PACK_VER in sync with SCCACHE_VERSION /
# NEXTEST_VERSION / WASM_PACK_VERSION in scripts/ci/gcp-full-ci.sh, which keeps
# pinned `command -v`-guarded fallback downloads for non-baked environments.
RUN apt-get update -qq \
    && apt-get install -y --no-install-recommends \
      bash \
      bc \
      binaryen \
      build-essential \
      ca-certificates \
      curl \
      git \
      gnupg \
      jq \
      nodejs \
      npm \
      pkg-config \
      python3 \
      tar \
      unzip \
      xz-utils \
      zstd \
    && HYPERFINE_VER=1.18.0 \
    && SCCACHE_VER=0.9.1 \
    && NEXTEST_VER=0.9.137 \
    && WASM_PACK_VER=0.15.0 \
    && curl -sL "https://github.com/sharkdp/hyperfine/releases/download/v${HYPERFINE_VER}/hyperfine-v${HYPERFINE_VER}-x86_64-unknown-linux-musl.tar.gz" \
       | tar xz --strip-components=1 -C /usr/local/bin "hyperfine-v${HYPERFINE_VER}-x86_64-unknown-linux-musl/hyperfine" \
    && curl -sL "https://github.com/mozilla/sccache/releases/download/v${SCCACHE_VER}/sccache-v${SCCACHE_VER}-x86_64-unknown-linux-musl.tar.gz" \
       | tar xz --strip-components=1 -C /usr/local/bin "sccache-v${SCCACHE_VER}-x86_64-unknown-linux-musl/sccache" \
    && curl -fsSL https://sdk.cloud.google.com > /tmp/install-gcloud.sh \
    && bash /tmp/install-gcloud.sh --disable-prompts --install-dir=/ \
    && rm -rf /tmp/install-gcloud.sh \
    && curl https://sh.rustup.rs -sSf | su runner -c 'sh -s -- -y --profile minimal --default-toolchain stable' \
    && su runner -c '/home/runner/.cargo/bin/rustup component add clippy rustfmt llvm-tools-preview' \
    && su runner -c '/home/runner/.cargo/bin/rustup target add wasm32-unknown-unknown' \
    && curl -LsSf "https://get.nexte.st/${NEXTEST_VER}/linux" | tar zxf - -C /usr/local/bin \
    && curl -sL "https://github.com/rustwasm/wasm-pack/releases/download/v${WASM_PACK_VER}/wasm-pack-v${WASM_PACK_VER}-x86_64-unknown-linux-musl.tar.gz" \
       | tar xz --strip-components=1 -C /usr/local/bin "wasm-pack-v${WASM_PACK_VER}-x86_64-unknown-linux-musl/wasm-pack" \
    && npm install -g pnpm \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

COPY start.sh /start.sh
RUN chmod +x /start.sh \
    && chown runner:runner /start.sh \
    && mkdir -p /home/runner/_work \
    && chown runner:runner /home/runner/_work

USER runner
WORKDIR /home/runner
ENTRYPOINT ["/start.sh"]
