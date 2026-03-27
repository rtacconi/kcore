# Security: Kubernetes vs kcore

This document compares the security model of Kubernetes with kcore's current
implementation. The goal is transparency about what kcore does and does not
provide, and to guide future hardening work.

## Trust Model

**Kubernetes** uses a multi-layered trust model designed for multi-tenant
environments running untrusted workloads. Identity is established through
service accounts, RBAC policies, and certificate signing requests (CSRs)
that go through an approval workflow.

**kcore** uses a simpler single-operator model. Trust is rooted in a shared
CA whose private key lives on the operator's machine. Certificates signed
by that CA grant access; the Common Name (CN) in the certificate determines
the role (`kcore-controller`, `kcore-kctl`, or `kcore-node-<host>`).

---

## Certificate and Identity Management

| Area | Kubernetes | kcore |
|------|-----------|-------|
| Bootstrap | TLS Bootstrap with short-lived tokens (24h default). Node submits a CSR; the control plane signs it. Token expires automatically. | Operator pushes a CA-signed cert directly to the node during `kctl node install`. No bootstrap token. |
| Cert rotation | kubelet auto-renews its certificate before expiry. Built into the kubelet. | No auto-renewal. Node certs are valid for 1 year (configurable in `pki.rs`). Manual rotation with `kctl rotate certs` for the controller cert. |
| Revocation | Can deny CSR renewals, delete the node object, or rotate the CA. RBAC blocks access immediately. | No revocation mechanism. A leaked certificate remains valid until the CA is regenerated. |
| Identity granularity | Each component has a distinct service account with RBAC bindings. Least-privilege by default. | Three CN-based roles: controller, kctl, and node. No fine-grained permissions within a role. |

### How kcore bootstrap works

1. Operator runs `kctl create cluster --controller <ip:port>` on their
   local machine. This generates a self-signed CA, a controller cert
   (with the controller IP as a SAN), and a kctl client cert.
2. When adding a node, `kctl node install --join-controller <ip:port>`
   signs a fresh node certificate using the local CA key and pushes it
   (along with the CA cert) to the node over gRPC.
3. After reboot, the node-agent uses its cert to establish mTLS with the
   controller and self-registers.

The security boundary is the CA private key stored at
`~/.kcore/<context>/ca.key`. Whoever possesses it can add nodes to the
cluster.

---

## Node Lifecycle

| Area | Kubernetes | kcore |
|------|-----------|-------|
| Admission | Node CSR must be approved (automatically or manually). Node object is created only after approval. | Any node with a valid CA-signed cert self-registers immediately on startup. No approval gate. |
| Heartbeat | kubelet sends periodic leases. After `node-monitor-grace-period` (40s default) the node is marked `NotReady`. Pods get evicted after `pod-eviction-timeout`. | Heartbeat mechanism exists but there is no automatic VM migration when a node becomes unreachable. |
| Graceful removal | `kubectl drain` cordons the node and evicts pods respecting PodDisruptionBudgets. `kubectl delete node` removes the identity. | `kctl drain node` migrates VMs but there is no cordon (prevent new scheduling without draining). Deleting a node from the DB does not invalidate its certificate. |

---

## API Security

| Area | Kubernetes | kcore |
|------|-----------|-------|
| Authorization | RBAC with Roles, ClusterRoles, and bindings. Per-resource, per-verb granularity (e.g., "this service account can only list pods in namespace X"). | CN-based: a connection is either kctl (full access), a node (node-level operations), or the controller. No middle ground. |
| Audit logging | Built-in audit log pipeline recording request metadata, response codes, and actor identity. Configurable verbosity levels. | Structured `tracing` logs exist but there is no dedicated audit trail of who performed which mutating action. |
| Admission control | Validating and mutating admission webhooks that can intercept any API request before persistence. | Validation happens inside gRPC handlers. No pluggable admission mechanism. |
| API versioning | Strict versioning (v1, v1beta1) with deprecation policy and conversion webhooks. | Protobuf fields are added in a backward-compatible way but there is no formal version negotiation or compatibility contract. |

---

## Secrets Management

| Area | Kubernetes | kcore |
|------|-----------|-------|
| Secrets storage | Dedicated Secret resource. Supports encryption at rest via KMS providers. Access is RBAC-scoped. | SSH public keys are stored in plaintext in SQLite. No general-purpose secrets management. |
| Workload identity | Automatic service account token mounting with short-lived projected tokens (bound service account tokens). | VMs do not receive identity tokens from the control plane. |

---

## What This Means in Practice

Most of the Kubernetes machinery described above exists because Kubernetes
is designed for **multi-tenant environments** running **untrusted
workloads**. kcore is a **single-operator bare-metal hypervisor**. The
threat model and practical priorities are different.

### Security measures already in place

- **mTLS everywhere**: all gRPC communication (controller to node, kctl to
  controller) requires mutual TLS authentication.
- **CA key stays local**: the CA private key never leaves the operator's
  machine. Only signed certificates are transmitted to nodes.
- **CN-based authorization**: the controller checks the certificate Common
  Name to enforce role separation (kctl vs node vs controller).
- **Certificate rotation command**: `kctl rotate certs --controller <addr>`
  re-signs the controller certificate with a new SAN using the existing CA.

### Planned improvements (high impact, moderate effort)

1. **Node approval queue** -- hold new registrations in a `pending` status
   until the operator runs `kctl approve node <id>`. Prevents rogue nodes
   from joining even if they possess a valid certificate.
2. **Audit log** -- structured log of all mutating API calls recording
   actor identity, action, resource, and timestamp. Essential for
   debugging and compliance.
3. **Cert expiry warning** -- `kctl get nodes` displays days until each
   node's certificate expires to prevent silent expiry.

### Future improvements (high effort, important at scale)

4. **Automatic cert renewal** -- node-agent requests a new certificate
   from the controller before its current cert expires. Eliminates manual
   rotation.
5. **RBAC** -- multiple operator roles such as read-only, vm-admin, and
   cluster-admin with fine-grained permission control.

### Not planned (Kubernetes-specific complexity)

- **CSR approval flow** -- kcore controls the CA directly; a CSR workflow
  adds complexity without meaningful benefit for a single-operator model.
- **Admission webhooks** -- no plugin ecosystem to justify the abstraction.
- **API versioning** -- single codebase with all components deployed
  together; protocol compatibility is maintained through additive protobuf
  changes.
