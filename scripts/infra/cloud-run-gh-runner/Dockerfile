FROM ghcr.io/actions/actions-runner:2.332.0

USER root

ENV DEBIAN_FRONTEND=noninteractive \
    CARGO_HOME=/home/runner/.cargo \
    RUSTUP_HOME=/home/runner/.rustup \
    PATH=/home/runner/.cargo/bin:/google-cloud-sdk/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin

RUN apt-get update -qq \
    && apt-get install -y --no-install-recommends \
      bash \
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
    && curl -fsSL https://sdk.cloud.google.com > /tmp/install-gcloud.sh \
    && bash /tmp/install-gcloud.sh --disable-prompts --install-dir=/ \
    && rm -rf /tmp/install-gcloud.sh \
    && curl https://sh.rustup.rs -sSf | su runner -c 'sh -s -- -y --profile minimal --default-toolchain stable' \
    && su runner -c '/home/runner/.cargo/bin/rustup component add clippy rustfmt' \
    && su runner -c '/home/runner/.cargo/bin/rustup target add wasm32-unknown-unknown' \
    && curl -LsSf https://get.nexte.st/latest/linux | tar zxf - -C /usr/local/bin \
    && curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

COPY start.sh /start.sh
RUN chmod +x /start.sh \
    && chown runner:runner /start.sh \
    && mkdir -p /home/runner/_work \
    && chown -R runner:runner /home/runner

USER runner
WORKDIR /home/runner
ENTRYPOINT ["/start.sh"]
