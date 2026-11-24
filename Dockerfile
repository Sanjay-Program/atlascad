# Multi-stage production build for AtlasCAD Engineering Workstation
# Stage 1: Build the standalone binary
FROM rust:1.80-slim-bookworm AS builder

WORKDIR /build

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

COPY . .

RUN cargo build --release -p cad-studio

# Stage 2: Minimal, secure runtime container
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
