# Deployment

Atlas CAD Studio is a single-document, single-process application. Three shapes are supported, and
they are the only three: a container behind a published port, a systemd unit behind Caddy, and a
studio shared directly on a LAN. Anything else — accounts, several documents per server, TLS inside
the binary, horizontal scaling — is not implemented, and no amount of configuration will produce it.

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
in `docker inspect`'s command, in `ps`, or in shell history. The printed share link is redacted when
the token comes from a file or the environment.

Environment variables: `ATLAS_PROJECT` (default `/data/part.zcad`), `ATLAS_PORT`, `ATLAS_HOST`
(bind address), `ATLAS_ADVERTISE` (host in the share link), `ATLAS_NAME` and `ATLAS_SIZE` for the
document created on first start.

Publish on `127.0.0.1` and put a proxy in front, or publish on a LAN address deliberately. Publishing
`0.0.0.0:7519` to the internet exposes an unencrypted, token-in-URL editor.

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
  wedged is reported unhealthy. `--help` would have answered 200 in that case.
- The access log is the only log. A shared link carries its token in the query string, so a proxy log
  that records request URIs is a secret: keep it `0600`, on the same host, rotated, not shipped.

## 6. Backup and recovery

The `.zcad` file is the whole document. Copy it while the studio is stopped, or accept a snapshot
taken at a save boundary. Unsaved edits live in `<file>.autosave` next to it and are offered on the
next start (`recovered` in `/api/state`); they are not a backup, only crash protection.

## 7. Upgrade and rollback

The document format is versioned (`schema`). A studio refuses a document newer than it understands
rather than trying to read it, so rollback means restoring the previous binary **and** a document
written by it. Take a copy of the `.zcad` before upgrading.

## 8. What this deployment does not give you

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
