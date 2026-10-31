# Atlas CAD Studio — runtime image.
#
# Build it with deploy/build-image.sh, which compiles the binary on this host and hands docker a
# two-file context (the binary and deploy/docker-entrypoint.sh). Building the workspace inside the
# image would need the whole crate graph fetched, gigabytes of RAM, and a compiler in the shipped
# artefact; the split keeps the image small and the build where the toolchain already is.
#
#   deploy/build-image.sh --smoke   # build, image, container, HTTP checks, teardown
#
# One container serves one document. For several documents, run several containers with separate
# volumes and ports: there is no multi-document mode and no account system
# (docs/operations/deployment.md).

FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends tini \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --system --gid 10001 atlas \
    && useradd --system --uid 10001 --gid atlas --home-dir /data --shell /usr/sbin/nologin atlas \
    && mkdir -p /data \
    && chown atlas:atlas /data

COPY cad-studio /usr/local/bin/cad-studio
COPY docker-entrypoint.sh /usr/local/bin/docker-entrypoint.sh
RUN chmod 0755 /usr/local/bin/cad-studio /usr/local/bin/docker-entrypoint.sh

USER atlas
WORKDIR /data
ENV ATLAS_PROJECT=/data/part.zcad \
    ATLAS_PORT=7519 \
    ATLAS_NAME=part \
    ATLAS_SIZE=40,30,10

EXPOSE 7519
VOLUME ["/data"]

# A real request, not `--help`: a process that is alive but wedged must fail its healthcheck. The
# liveness route is the one API path a shared studio answers without a token, so this needs no
# secret and reveals nothing about the document. The Host header must carry the port: a shared
# studio accepts any name at its own port and nothing else. bash's /dev/tcp is used because
# installing an HTTP client just for this is not worth the extra packages in the image.
HEALTHCHECK --interval=30s --timeout=3s --start-period=10s --retries=3 \
    CMD ["/bin/bash", "-c", "exec 3<>/dev/tcp/127.0.0.1/$ATLAS_PORT && printf 'GET /api/health HTTP/1.1\\r\\nHost: 127.0.0.1:%s\\r\\nConnection: close\\r\\n\\r\\n' \"$ATLAS_PORT\" >&3 && head -1 <&3 | grep -q '200 OK'"]

ENTRYPOINT ["/usr/bin/tini", "--", "/usr/local/bin/docker-entrypoint.sh"]
CMD []
