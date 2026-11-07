# Deployment

Atlas CAD Studio is a single-document, single-process application. Three shapes are supported, and
they are the only three: a container behind a published port, a systemd unit behind Caddy, and a
studio shared directly on a LAN. Anything else — accounts, several documents per server, TLS inside
the binary, horizontal scaling — is not implemented, and no amount of configuration will produce it.
A CAE study is **not** a fourth studio shape: it is a separate batch container that reads the same
document file (section 8).

Everything below was run on the machine that wrote it: the image was built and smoke-tested, the
proxy shape was exercised with a real `Host` and an `https://` origin, and the unit was checked with
`systemd-analyze verify`.

## 1. What a deployment actually is

| Piece | Reality |
| --- | --- |
| Process | one `cad-studio` per document, listening on one port |
| State | one `.zcad` file on disk, plus `.zcad.autosave` snapshots for unsaved edits |
| Auth | one bearer token, required on every `/api/` route except liveness |
| Transport | plain HTTP; TLS belongs to a proxy in front |
| Users | any number of readers with the link; every one of them can edit, undo and save |
| Persistence | the document file is the state; restarting the process re-reads it |
| Sessions | none. A token is not tied to a browser, and cannot be revoked without restarting |

## 2. Container

```sh
deploy/build-image.sh            # release binary + image
deploy/build-image.sh --smoke    # ... and prove it serves before shipping
```

`--smoke` starts the image, waits for the container's own healthcheck, then checks four things over
HTTP: liveness without a token (200), the document without a token (401), the document with the
token (200), and a plotted PDF (`%PDF-1.4`). It prints the document file inside the volume and tears
everything down. If that passes, the image serves a document.

Run it:

```sh
docker run -d --name atlas \
  -p 127.0.0.1:7519:7519 \
  -v atlas-data:/data \
  -e ATLAS_STUDIO_TOKEN="$(openssl rand -hex 16)" \
  atlas-studio:0.1.0
```

The token is read from the environment by the binary itself (`ATLAS_STUDIO_TOKEN`) or from a file
(`ATLAS_STUDIO_TOKEN_FILE`). It is never a command-line argument in the image, so it does not appear
in `ps` or in shell history. An environment variable **is** shown by `docker inspect` (in
`Config.Env`), which is why the compose stack in section 2a mounts a file secret instead; use the
`-e` form above only for a throwaway container whose inspect output is not exposed. The printed
share link is redacted when the token comes from a file or the environment.

Environment variables: `ATLAS_PROJECT` (default `/data/part.zcad`), `ATLAS_PORT`, `ATLAS_HOST`
(bind address), `ATLAS_ADVERTISE` (host in the share link), `ATLAS_NAME` and `ATLAS_SIZE` for the
document created on first start.

Publish on `127.0.0.1` and put a proxy in front, or publish on a LAN address deliberately. Publishing
`0.0.0.0:7519` to the internet exposes an unencrypted, token-in-URL editor.

## 2a. Hosted stack from this repository (`compose.yaml`)

The supported hosted shape is `deploy/compose.yaml`: two containers on one host, the studio and
Caddy, with the document in a named volume and TLS on a public name.

```sh
cd deploy
cp hosted-env.example .env      # set STUDIO_PUBLIC_HOST; the rest already has working defaults
./gen-token.sh                  # writes ./studio-token.txt, 0600
./up.sh                         # preflight, up --wait, print https://<name>
```

`up.sh` refuses to start rather than starting something broken: it checks docker and the compose
plugin, that compose can interpolate the stack, that the token file exists and is a plausible
secret (16–128 characters, no whitespace, `0600`), and that the studio image is present. Without
it, `docker compose up -d --wait` in `deploy/` does the same thing but leaves the token checks to
the operator.

| Variable | Default | Meaning |
| --- | --- | --- |
| `STUDIO_PUBLIC_HOST` | *(required)* | site name and certificate name; also the share link's host |
| `STUDIO_TOKEN_FILE` | `./studio-token.txt` | the bearer token file staged into the studio by the one-shot `atlas-token` service |
| `STUDIO_IMAGE_TAG` | `0.1.0` | studio image tag |
| `CADDY_IMAGE_TAG` | `2` | Caddy image tag; pin a minor version in production |
| `STUDIO_HTTP_PORT` / `STUDIO_HTTPS_PORT` | `80` / `443` | host ports for the proxy |
| `STUDIO_BIND_PORT` | `7519` | loopback host port for the studio |
| `STUDIO_NAME` / `STUDIO_SIZE` | `part` / `40,30,10` | the document created on first start only |

How the pieces fit:

- **The token is a file, staged by a one-shot service.** Compose bind-mounts the operator's 0600
  `studio-token.txt` into an `atlas-token` container that runs once, copies it into a private
  volume as `/run/secrets/atlas-studio-token`, and exits; the studio waits for that
  (`service_completed_successfully`) and reads it with `--token-file`. The copy exists because
  Compose ignores `uid`/`gid`/`mode` on file secrets and bind-mounts the host file as-is, which uid
  10001 cannot read (measured on Compose 5.3.1). The token is absent from the container environment
  and therefore from `docker inspect`. Rotating it means writing a new file and running `./up.sh`
  again, which force-recreates the stack: the token is read once at startup.
- **The healthcheck is the image's own**, a real `GET /api/health` over bash's `/dev/tcp`. Compose
  does not override it, so `up --wait` and the proxy's `depends_on: condition: service_healthy`
  both reflect whether the studio actually answers, not merely whether the process exists.
- **The public name is not hardcoded.** `Caddyfile` takes the site name, the upstream and the log
  path from the environment (`STUDIO_PUBLIC_HOST`, `STUDIO_UPSTREAM`, `STUDIO_LOG_FILE`), so the
  same file serves the systemd shape (upstream `127.0.0.1:7519`) and the compose shape (upstream
  `atlas-studio:7519`).
- **Both long-running containers are hardened**: read-only root filesystem with a small `/tmp`
  tmpfs, all capabilities dropped (Caddy adds back only `NET_BIND_SERVICE`), `no-new-privileges`, a
  pid limit, a memory limit, and bounded json-file logging. The one-shot token stage runs as root
  with only `DAC_READ_SEARCH` and `DAC_OVERRIDE`, which is what reading the operator's 0600 file and
  writing the 0444 copy requires. Caddy's access log is a named volume because every request URI can
  carry the token.

```sh
docker compose -f compose.yaml ps
docker compose -f compose.yaml logs -f
docker compose -f compose.yaml down        # keep the volume
docker compose -f compose.yaml down -v     # remove the document
```

## 2b. One-command end-to-end smoke of the hosted shape

```sh
deploy/smoke.sh            # build if needed, start, verify, drive a browser, tear down
deploy/smoke.sh --keep     # leave it running and print the URL
```

`deploy/smoke.sh` is the deployment gate for the compose stack, and it is deliberately stricter
than `build-image.sh --smoke`: it runs the *proxied* shape, not the bare container.

1. Builds `atlas-studio:0.1.0` with `build-image.sh` if the image is missing.
2. Generates a throwaway token, then starts `compose.smoke.yaml` under its own compose project and
   on its own host ports, so it can run while a real deployment keeps 80/443.
3. Lets the container **create a new document** on a fresh volume — the service's first-start path,
   not a reopen — and asserts the `/api/state` payload names it, has no failed feature and reports a
   positive volume through the proxy.
4. Checks liveness over both plain HTTP and HTTPS (a real TLS handshake, local-CA certificate).
5. Checks the token boundary through the proxy: `401` without a token, `200` with one.
6. Runs `scripts/browser-check.mjs` against the proxied page (31 assertions: the tree, the measured
   volume, drawing edits, named revisions, a raised-plane boss, and STL/DXF/PDF downloads), then
   tears the stack and its volume down.

Host ports and the document shape are variables rather than constants, so a second stack (a smoke
run, or a second document) can coexist with the first:

| Variable | Default | Meaning |
| --- | --- | --- |
| `STUDIO_HTTP_PORT` | `80` | host port mapped to the proxy's HTTP listener |
| `STUDIO_HTTPS_PORT` | `443` | host port mapped to the proxy's HTTPS listener |
| `STUDIO_BIND_PORT` | `7519` | loopback port mapped to the studio |
| `STUDIO_SIZE` | `40,30,10` | size of the document created on first start |
| `STUDIO_NAME` | `part` | name of the document created on first start |

The smoke Caddyfile answers on named sites (`http://localhost`, `https://localhost`,
`http://127.0.0.1`, `https://127.0.0.1`). A bare `:443` site has no name for the internal CA to
issue against, so every handshake failed with `tlsv1 alert internal error` — measured, then fixed;
the smoke now proves the TLS path instead of assuming it.

## 3. systemd + Caddy (TLS, one public name)

```sh
useradd --system --home /var/lib/atlas-studio atlas
install -d -o atlas -g atlas /var/lib/atlas-studio
install -d -o root -g atlas -m 0750 /etc/atlas-studio
install -o root -g atlas -m 0640 /dev/null /etc/atlas-studio/token
head -c 32 /dev/urandom | od -An -tx1 | tr -d ' \n' > /etc/atlas-studio/token
install -o root -g root -m 0644 deploy/atlas-studio.service /etc/systemd/system/
install -o root -g root -m 0640 deploy/atlas-studio.env.example /etc/atlas-studio/env
systemctl daemon-reload && systemctl enable --now atlas-studio
caddy run --config deploy/Caddyfile --adapter caddyfile
```

The unit listens on **127.0.0.1 only** and still requires the token, because the public name reaches
it through the proxy. That combination is why `--share --host 127.0.0.1 --token-file …` is accepted:
before, “shared” was inferred from the bind address, so a proxied studio was either token-free or
unable to bind. Both are bugs; both are now covered by tests.

The unit is hardened (`ProtectSystem=strict`, `MemoryDenyWriteExecute`, a syscall allow-list,
`MemoryMax=2G`, `TasksMax=64`). It restarts on failure with a start limit so a corrupt document does
not become a restart loop.

## 4. Direct on a LAN (no proxy)

```sh
cad-studio --file part.zcad --share --advertise 192.168.1.20 \
  --token-file /etc/atlas/token
```

`--share` binds every interface and prints a tokenised link. Anyone on the network with the link can
edit the document until the process stops. This is the right shape for a workshop network and the
wrong shape for the internet.

## 5. Health, logs, metrics

- `GET /api/health` → `{"ok":true,"version":"0.1.0"}`. The only API route a shared studio answers
  without a token: a container healthcheck or proxy cannot hold the secret, and the reply names no
  part of the document. Everything else under `/api/` needs the token.
- Container healthcheck: a real HTTP request over bash's `/dev/tcp`, so a process that is alive but
  wedged is reported unhealthy. `--help` would have answered 200 in that case. Compose inherits this
  check rather than replacing it with a liveness shortcut.
- The access log is the only log. A shared link carries its token in the query string, so a proxy log
  that records request URIs is a secret: keep it `0600`, on the same host, rotated, not shipped.

## 6. Backup and recovery

The `.zcad` file is the whole document. Copy it while the studio is stopped, or accept a snapshot
taken at a save boundary. Unsaved edits live in `<file>.autosave` next to it and are offered on the
next start (`recovered` in `/api/state`); they are not a backup, only crash protection.

The compose stack has a scripted path (both refuse to run while the stack is up, unless you accept
a save-boundary copy):

```sh
cd deploy
docker compose -f compose.yaml down
./backup.sh                       # backups/atlas-studio-data-<utc>.tar.gz + .sha256, keeps 14
./backup.sh --out /srv/backups --keep 30
./restore.sh --file backups/atlas-studio-data-<utc>.tar.gz --yes
./up.sh
```

`backup.sh` streams `tar` out of a one-shot container with the volume mounted read-only, so the
archive belongs to the operator and nothing in the stack is touched. `restore.sh` verifies the
`.sha256`, takes a `pre-restore-*` safety copy, then replaces the volume contents as root so the
studio's uid 10001 ownership survives. Without compose, the equivalent one-liner is in
`compose.yaml`'s header comment.

## 7. Upgrade and rollback

The document format is versioned (`schema`). A studio refuses a document newer than it understands
rather than trying to read it, so rollback means restoring the previous binary **and** a document
written by it. Take a copy of the `.zcad` before upgrading.

```sh
cd deploy
./backup.sh                              # the document first, while stopped
./build-image.sh --tag 0.2.0             # build the new image
STUDIO_IMAGE_TAG=0.2.0 STUDIO_PUBLIC_HOST=cad.example.com ./up.sh
# rollback: the previous image tag is still on the host
STUDIO_IMAGE_TAG=0.1.0 STUDIO_PUBLIC_HOST=cad.example.com ./up.sh
```

Images are tagged by version and never overwritten, so a rollback is a tag change plus, if the
schema moved, `restore.sh` of the document taken before the upgrade. The compose volume survives
`up`/`down`; only `down -v` removes it.

## 8. Analysis jobs

The studio does not run CAE. A study is a batch container of its own, built by
`deploy/build-cae-image.sh --smoke` from `deploy/Dockerfile.cae`: one project file and one versioned
study file in, one self-contained result file out, and an exit code that distinguishes bad input (2)
from an analysis refusal (3). There is no port, no healthcheck and no queue. It can mount the same
data volume as the studio, but the project file is single-writer: stop the studio before a job that
records a simulation. The contract, hardening flags, measured resource envelope and limits are in
[`cae-deployment.md`](cae-deployment.md).

## 9. What this deployment does not give you

- No accounts, no per-user permissions: one token, all-or-nothing, every holder can edit.
- No revocation short of restarting with a new token.
- No multi-document mode. N documents = N processes = N ports.
- No TLS in-process, no certificate handling, no HSTS without a proxy.
- No audit trail of who changed what; the undo history is the project's, not a user's.
- No horizontal scaling or shared storage. The file is the lock.
- No PDF/A, no STEP/IGES exchange — DXF (R12) and PDF are what the binary can produce.

The **licence** is the other limit: the root `LICENSE` is personal/non-commercial and a commercial
deployment needs the signed agreement in `COMMERCIAL-LICENSE.md`. Deploying this for paying work
without that is not a technical question.
