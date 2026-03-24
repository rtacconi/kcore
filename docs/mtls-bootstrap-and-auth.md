# mTLS Bootstrap and Authentication

This document explains how cluster certificates are created, how they are installed on nodes, and how mTLS is enforced between `kctl`, `kcore-controller`, and `kcore-node-agent`.

## 1) Certificate and CA creation

Cluster PKI is generated with:

```bash
kctl create cluster --controller <controller-host:9090>
```

The command creates:

- `ca.crt` / `ca.key`: cluster Certificate Authority
- `controller.crt` / `controller.key`: controller identity (server + client usage)
- `kctl.crt` / `kctl.key`: CLI client identity

By default, files are stored under `~/.kcore/certs` and the active context in `~/.kcore/config` is updated to use:

- `ca`: `~/.kcore/certs/ca.crt`
- `cert`: `~/.kcore/certs/kctl.crt`
- `key`: `~/.kcore/certs/kctl.key`

## 2) Node install bootstrap (cert persistence)

When `kctl node install ...` is called, `kctl`:

1. Loads CA/controller/kctl certs from the local cert dir.
2. Generates a node certificate (`node.crt`/`node.key`) signed by the same CA, with SAN = node host.
3. Sends all PEM materials in `InstallToDiskRequest`.

The node-agent receives these fields and writes them to:

- `/etc/kcore/certs/ca.crt`
- `/etc/kcore/certs/node.crt`
- `/etc/kcore/certs/node.key`
- `/etc/kcore/certs/controller.crt`
- `/etc/kcore/certs/controller.key`
- `/etc/kcore/certs/kctl.crt`
- `/etc/kcore/certs/kctl.key`

Before the OS install finishes, the installer copies `/etc/kcore/*` into `/mnt/etc/kcore` on the target disk. This is what persists certs across reboot into the installed KcoreOS system.

## 3) Runtime mTLS authentication

### `kctl` -> `controller` and `kctl` -> `node-agent`

- `kctl` uses `https://...` unless `--insecure` is set.
- It requires CA cert + client cert + client key in secure mode.
- Server identity is validated by CA trust.
- Client identity is presented to server via mTLS.

### `controller` server and `node-agent` server

Both services support TLS config in YAML:

```yaml
tls:
  caFile: /etc/kcore/certs/ca.crt
  certFile: /etc/kcore/certs/<service>.crt
  keyFile: /etc/kcore/certs/<service>.key
```

When TLS is configured, each server:

- serves TLS with its cert/key
- requires client certificate signed by `caFile` (`client_ca_root`)

### `controller` -> `node-agent`

Controller uses the same configured CA + identity to open outbound connections to node-agent:

- secure path: `https://<node-host:9091>` with client cert
- fallback path: `http://...` only if controller TLS is not configured

## 4) Security posture and current limits

mTLS materially reduces MITM risk and blocks unauthenticated network clients from calling gRPC endpoints when TLS is enabled on both sides.

Remaining gaps to track:

- no certificate rotation workflow yet
- no CRL/OCSP revocation checks
- broad certificate distribution model during bootstrap
- authorization model is still coarse (transport auth is in place, fine-grained RBAC is not)

## 5) Verification checklist

- Generate PKI: `kctl create cluster --controller <controller:9090>`
- Confirm files in `~/.kcore/certs`
- Install node with `kctl node install ...`
- Verify installed node has `/etc/kcore/certs/*`
- Ensure `controller.yaml` and `node-agent.yaml` include `tls` block
- Confirm secure traffic uses HTTPS and rejects untrusted client certificates
