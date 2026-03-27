# mTLS Bootstrap and Authentication

This document explains how cluster certificates are created, how they are installed on nodes, and how mTLS is enforced between `kctl`, `kcore-controller`, and `kcore-node-agent`.

## 1) Certificate and CA creation

Cluster PKI is generated with:

```bash
kctl create cluster --controller <controller-host:9090>
```

The command creates:

- `ca.crt` / `ca.key`: cluster root Certificate Authority
- `sub-ca.crt` / `sub-ca.key`: intermediate sub-CA for automatic node cert renewal
- `controller.crt` / `controller.key`: controller identity (server + client usage)
- `kctl.crt` / `kctl.key`: CLI client identity

The sub-CA has `pathlen:0` (can sign leaf certs but not further sub-CAs) and a 5-year validity period. It is signed by the root CA and deployed to the controller for automatic certificate renewal.

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
- `/etc/kcore/certs/sub-ca.crt` (controller nodes only)
- `/etc/kcore/certs/sub-ca.key` (controller nodes only)

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

## 4) Automatic certificate renewal

Node certificates are valid for 1 year. The node-agent includes an automatic renewal client:

1. At startup and once daily, the node-agent reads its certificate from disk and checks the expiry date.
2. If the certificate expires in more than 30 days, no action is taken.
3. If within 30 days of expiry, the node-agent calls `RenewNodeCert` on the controller over the existing mTLS connection.
4. The controller verifies the node is approved, then signs a new certificate using its **sub-CA** (intermediate CA). It returns the new leaf cert + sub-CA chain PEM and a new private key.
5. The node-agent writes the renewed cert and key to disk. The new certificate takes effect on the next service restart.

The trust chain works as follows:
- `ca.crt` on each node contains only the root CA (trust anchor)
- After renewal, `node.crt` contains the leaf cert + sub-CA cert (concatenated PEM). rustls resolves the chain automatically.
- Existing root-CA-signed certs continue working. Renewals transition to sub-CA-signed certs.

### Sub-CA rotation

The operator can rotate the sub-CA at any time:

```bash
kctl rotate sub-ca
```

This generates a new sub-CA from the root CA, writes it locally, and pushes it to the controller via the `RotateSubCa` RPC. The controller hot-reloads the new sub-CA without restart. Future renewals use the new sub-CA while existing certs remain valid.

### Controller certificate rotation

```bash
kctl rotate certs --controller <new-host:port>
```

This re-signs the controller certificate with a new SAN. The new cert must be deployed to the controller node and the service restarted.

## 5) Security posture and current limits

mTLS materially reduces MITM risk and blocks unauthenticated network clients from calling gRPC endpoints when TLS is enabled on both sides.

Additional security measures:

- **Node approval queue**: new nodes register as `pending` and must be approved before participating in the cluster.
- **Sub-CA auto-rotation**: node certs are renewed automatically; the sub-CA is revocable by the operator without affecting the root CA.
- **Certificate expiry visibility**: each node reports its certificate expiry at registration. `kctl get nodes` shows a `CERT EXPIRY` column with days remaining and a `⚠` warning when within 30 days of expiry.

### FIPS-compatible cryptography

All TLS connections use **aws-lc-rs** as the rustls crypto backend. aws-lc-rs wraps AWS-LC, which holds FIPS 140-3 validation (certificate #4816). At process startup, each binary (controller, node-agent, kctl) installs a custom `CryptoProvider` that restricts:

- **Cipher suites**: TLS 1.3 AES-128-GCM, AES-256-GCM; TLS 1.2 ECDHE-ECDSA/RSA with AES-128-GCM and AES-256-GCM. ChaCha20-Poly1305 is excluded.
- **Key exchange groups**: secp256r1 (P-256) and secp384r1 (P-384) only. X25519 is excluded.

Certificate generation (`rcgen`) also uses aws-lc-rs instead of ring.

Remaining gaps to track:

- no CRL/OCSP revocation checks (sub-CA rotation provides a partial mitigation)
- authorization model is still coarse (transport auth is in place, fine-grained RBAC is not)

## 6) Verification checklist

- Generate PKI: `kctl create cluster --controller <controller:9090>`
- Confirm files in `~/.kcore/certs` (including `sub-ca.crt` and `sub-ca.key`)
- Install node with `kctl node install ...`
- Verify installed node has `/etc/kcore/certs/*`
- Verify controller node has `/etc/kcore/certs/sub-ca.crt` and `sub-ca.key`
- Ensure `controller.yaml` and `node-agent.yaml` include `tls` block
- Ensure `controller.yaml` includes `subCaCertFile` and `subCaKeyFile`
- Confirm secure traffic uses HTTPS and rejects untrusted client certificates
- Confirm node-agent logs `certificate valid, no renewal needed` at startup
- Test rotation: `kctl rotate sub-ca` and verify controller logs `sub-CA rotated via kctl`
