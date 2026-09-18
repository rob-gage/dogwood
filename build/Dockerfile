# syntax=docker/dockerfile:1.7
FROM rust:1.85-bookworm AS builder
ARG DOGWOOD_TARGET
ARG RUST_TARGET
ARG GAME_PACKAGE=game
ARG GAME_BINARY=game
ARG ZIG_VERSION=0.13.0
RUN apt-get update && apt-get install -y --no-install-recommends clang lld cmake curl ca-certificates && rm -rf /var/lib/apt/lists/*
RUN arch=$(dpkg --print-architecture) && case "$arch" in amd64) zig_arch=x86_64;; arm64) zig_arch=aarch64;; *) echo "unsupported Docker architecture: $arch" >&2; exit 1;; esac && \
    curl -fsSL "https://ziglang.org/download/$ZIG_VERSION/zig-linux-$zig_arch-$ZIG_VERSION.tar.xz" | tar -xJ --strip-components=1 -C /usr/local
RUN cargo install --locked cargo-xwin --version 0.18.2 && cargo install --locked cargo-zigbuild --version 0.20.1
WORKDIR /src
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/src/target \
    --mount=type=cache,target=/root/.cache/cargo-xwin \
    if [ "$DOGWOOD_TARGET" = windows-x64 ] || [ "$DOGWOOD_TARGET" = windows-x86 ] || [ "$DOGWOOD_TARGET" = windows-arm64 ]; then \
      cargo xwin build --locked --release --package "$GAME_PACKAGE" --target "$RUST_TARGET"; \
      mkdir -p /artifact; cp "target/$RUST_TARGET/release/$GAME_BINARY.exe" /artifact/game.exe; \
    else \
      cargo zigbuild --locked --release --package "$GAME_PACKAGE" --target "$RUST_TARGET"; \
      mkdir -p /artifact; cp "target/$RUST_TARGET/release/$GAME_BINARY" /artifact/game; \
    fi

FROM scratch AS artifact
COPY --from=builder /artifact/ /
