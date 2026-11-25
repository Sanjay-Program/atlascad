# Multi-stage production build for the AtlasCAD Engineering Workstation.
#
# Build context: the repository root. For a smaller context, use deploy/build-image.sh, which
# compiles on the host and ships only the binary + entrypoint (see deploy/Dockerfile).
#
#   docker build -t atlascad:0.1.0 .
#
# The builder tag must match rust-toolchain.toml: this workspace is edition 2024 with
# `rust-version = "1.90"`, so an older image cannot compile it.
FROM rust:1.95.0-slim-bookworm AS builder

WORKDIR /build

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Copy only the build inputs. `apps/solidcraft/vendor` is required because the workspace patches
# truck-* to those vendored crates ([patch.crates-io] in Cargo.toml). Copying the whole repository
# instead pulls the reference checkouts and their build directories into the context.
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates ./crates
COPY apps/solidcraft/vendor ./apps/solidcraft/vendor

# `--locked` keeps the image on the committed Cargo.lock instead of drifting during a build.
RUN cargo build --release --locked -p cad-studio

# Stage 2: minimal, secure runtime container
FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    tini \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --system --gid 10001 atlas \
    && useradd --system --uid 10001 --gid atlas --home-dir /data --shell /usr/sbin/nologin atlas \
    && mkdir -p /data \
    && chown -R atlas:atlas /data

COPY --from=builder /build/target/release/cad-studio /usr/local/bin/cad-studio
RUN chmod 0755 /usr/local/bin/cad-studio

USER atlas
WORKDIR /data

ENV PORT=7519 \
    HOST=0.0.0.0 \
    ATLAS_PROJECT=/data/workspace.atlas

EXPOSE 7519
VOLUME ["/data"]

HEALTHCHECK --interval=20s --timeout=4s --start-period=5s --retries=3 \
    CMD curl -f http://127.0.0.1:7519/api/health || exit 1

ENTRYPOINT ["/usr/bin/tini", "--"]
CMD ["cad-studio", "--listen", "0.0.0.0:7519", "--file", "/data/workspace.atlas"]
