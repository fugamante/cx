FROM rust:1.95.0-bookworm@sha256:6258907abe69656e41cd992e0b705cdcfabcbbe3db374f92ed2d47121282d4a1

LABEL org.opencontainers.image.title="XSHELF compatibility environment" \
      org.opencontainers.image.description="Local Linux compatibility image for XSHELF/CX maintainer validation." \
      org.opencontainers.image.source="https://github.com/fugamante/XSHELF" \
      org.opencontainers.image.licenses="MIT"

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        bash \
        ca-certificates \
        diffutils \
        git \
        jq \
        python3 \
    && rm -rf /var/lib/apt/lists/*

RUN rustup component add clippy rustfmt

RUN printf '%s\n' 'export PATH="/usr/local/cargo/bin:${PATH}"' >/etc/profile.d/cargo-path.sh

WORKDIR /opt/cx-bootstrap

COPY rust/cxrs/Cargo.toml rust/cxrs/Cargo.lock rust/cxrs/rust-toolchain.toml ./rust/cxrs/
RUN mkdir -p rust/cxrs/src \
    && printf 'fn main() {}\n' > rust/cxrs/src/main.rs \
    && cargo fetch --manifest-path rust/cxrs/Cargo.toml --locked \
    && chmod -R a+rwX /usr/local/cargo/registry

WORKDIR /work
