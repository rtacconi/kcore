# KCore Compliance Enablement Evaluation

How KCore should be designed so that customers can achieve GDPR, SOC 2, PCI DSS, FIPS 140-3, and related compliance — without KCore itself being the certified entity.

## Framing: platform compliance vs. customer compliance

KCore is infrastructure software. It manages VMs on bare-metal NixOS nodes using cloud-hypervisor. The customers who deploy KCore are the ones who need certifications — to satisfy their own auditors, regulators, or enterprise buyers.

KCore's role is to be a **compliance-enabling platform**: it provides the security controls, APIs, audit trails, isolation guarantees, and documentation that allow a customer's assessor to check the box. KCore is not itself SOC 2 certified or PCI compliant — but a customer running workloads on KCore can be, if KCore gives them the right tools.

This is the same model used by AWS, GCP, and Azure: the cloud provider publishes a "shared responsibility model" and provides compliance artifacts. The customer inherits the platform controls and builds their own compliance on top.

---

## 1. What KCore already provides

### 1.1 Encryption in transit

All inter-component communication uses mutual TLS (mTLS) with a self-managed PKI:

| Connection | Protocol | Authentication |
|------------|----------|---------------|
| kctl → controller | gRPC over mTLS | Client cert (CN: `kcore-kctl`) |
| controller → node-agent | gRPC over mTLS | Client cert (CN: `kcore-controller`) |
| node-agent → controller | gRPC over mTLS | Client cert (CN: `kcore-node-<host>`) |

**Compliance relevance:**
- SOC 2 CC6.1 (encryption), PCI DSS 4.2 (strong cryptography in transit), GDPR Art. 32 (encryption), NCSC Principle 2 (asset protection)
- Customer evidence: "All management plane traffic is encrypted with TLS 1.2+ using mutual certificate authentication. No plaintext management traffic is possible in production configuration."

### 1.2 Identity and access control

Every gRPC method enforces caller identity via the certificate Common Name. The authorization table is hard-coded per RPC method:

- Node-scoped RPCs (RegisterNode, Heartbeat, SyncVmState) accept only `kcore-node-*` certificates
- Operator RPCs (CreateVm, DeleteVm, ListVms, etc.) accept only `kcore-kctl` certificates
- Admin RPCs (ApplyNixConfig) accept only `kcore-kctl`

**Compliance relevance:**
- SOC 2 CC6.3 (access control), PCI DSS 7 (restrict access), GDPR Art. 32 (access limitations)
- Customer evidence: "Every API call is authenticated by a client certificate issued by the cluster CA. Authorization is enforced per-method based on certificate identity."

### 1.3 VM isolation

Each VM runs in a separate cloud-hypervisor process with its own KVM instance:

- Separate memory address spaces via KVM hardware virtualization
- Separate TAP devices and bridge interfaces per network
- No shared filesystem between VMs (no virtio-fs by default)
- No inter-VM communication except through explicitly configured network bridges

**Compliance relevance:**
- PCI DSS 2.2 (system hardening), NCSC Principle 3 (separation between consumers), BSI-VS-AP-0019 (hypervisor isolation)
- Customer evidence: "Workloads run in hardware-isolated virtual machines. Each VM has a dedicated KVM instance, TAP device, and network namespace. No shared resources exist between VMs at the hypervisor level."

### 1.4 Declarative, auditable infrastructure

NixOS provides:

- Reproducible builds from a single flake
- Atomic system updates via `nixos-rebuild switch` (rollback on failure)
- Immutable Nix store (binaries cannot be modified in place)
- Every system configuration change is a new generation with a full dependency tree

**Compliance relevance:**
- SOC 2 CC8 (change management), PCI DSS 6.5 (secure development), NCSC Principle 5 (operational security)
- Customer evidence: "Infrastructure is defined declaratively. Every change produces a new immutable system generation. Rollback is atomic. No ad-hoc configuration drift is possible."

### 1.5 Input validation and injection prevention

All user-controlled strings interpolated into Nix expressions are escaped:

- `nix_escape()` prevents string injection (`"`, `\`, `${`)
- `sanitize_nix_attr_key()` restricts attribute names to `[a-zA-Z0-9_-]`
- Disk paths are validated against traversal attacks

**Compliance relevance:**
- PCI DSS 6.2 (secure coding), SOC 2 PI1 (processing integrity)
- Customer evidence: "All user input is validated and sanitized before being incorporated into system configurations. Injection attacks against the configuration pipeline are prevented by design."

### 1.6 Dependency auditing

`cargo audit` scans all Rust dependencies against the RustSec Advisory Database. It runs as part of `make check` and is available in the Nix development shell.

**Compliance relevance:**
- PCI DSS 6.3 (software inventory and vulnerability management), SOC 2 CC7 (monitoring)
- Customer evidence: "All third-party dependencies are audited against known vulnerability databases on every build."

### 1.7 Network segmentation

KCore supports three network types with distinct isolation properties:

| Type | Isolation | Cross-host | Use case |
|------|-----------|------------|----------|
| NAT | Full — VMs behind masquerade, no inbound except DNAT ports | No | Default workloads |
| Bridge | VMs on physical LAN — upstream firewall controls | No | Bare-metal-like access |
| VXLAN | Overlay L2 — isolated from host LAN, cross-host via UDP tunnel | Yes | Multi-node clusters |

Per-network firewall rules (nftables) are generated declaratively. VLAN tagging (802.1Q) provides L2 isolation when needed.

**Compliance relevance:**
- PCI DSS 1.3 (network segmentation), NCSC Principle 3 (separation), SOC 2 CC6.6 (network access)
- Customer evidence: "Workloads can be placed on isolated networks with configurable firewall rules, VLAN tags, and separate bridge interfaces. PCI-scoped VMs can be network-isolated from non-PCI workloads."

### 1.8 Node approval queue

Nodes must be explicitly approved by an operator (`kctl approve node`) before they can host workloads. Unapproved nodes cannot receive VM placements.

**Compliance relevance:**
- SOC 2 CC6.2 (asset management), PCI DSS 2.4 (system inventory)
- Customer evidence: "No compute node can join the cluster and receive workloads without explicit operator approval."

### 1.9 FIPS-compatible cryptography

All TLS connections use `aws-lc-rs` as the rustls crypto backend. `aws-lc-rs` wraps AWS-LC, which holds FIPS 140-3 validation (certificate #4816). At process startup each binary installs a custom `CryptoProvider` restricted to:

- TLS 1.3: AES-256-GCM-SHA384, AES-128-GCM-SHA256
- TLS 1.2: ECDHE-ECDSA/RSA with AES-256-GCM and AES-128-GCM
- Key exchange: secp384r1 (P-384), secp256r1 (P-256) only
- Excluded: ChaCha20-Poly1305, X25519, RSA key exchange

Certificate generation (`rcgen`) also uses `aws-lc-rs` instead of `ring`.

**Compliance relevance:**
- FIPS 140-3 (cryptographic module validation), PCI DSS 4.2 (strong cryptography)
- Customer evidence: "All management plane TLS uses aws-lc-rs (AWS-LC FIPS 140-3 #4816) with FIPS-approved cipher suites. No non-FIPS algorithms are negotiable."

---

## 2. What KCore must add to enable customer compliance

### 2.1 Audit logging (all standards)

**What customers need:** An immutable, queryable record of who did what, when, and to which resource. Every assessor — SOC 2, PCI, GDPR, NCSC — will ask for this.

**What to build:**

| Component | Detail |
|-----------|--------|
| Audit event structure | `{ timestamp, actor_cn, action, resource_type, resource_id, outcome, source_ip, request_id }` |
| Capture points | Every gRPC handler in controller and node-agent emits an audit event before returning |
| Storage | Append-only log file, structured JSON, one event per line |
| Retention config | Configurable via `auditLog.retentionDays` in controller.yaml (default: 90) |
| API exposure | New gRPC RPC `ListAuditEvents(filter, pagination)` so customers can pull audit data into their SIEM |
| kctl command | `kctl cluster audit-log [--since <time>] [--actor <cn>] [--action <type>]` |

**Why this matters for customers:**
- SOC 2 CC7.2: "The entity monitors system components for anomalies" — requires access to audit logs
- PCI DSS 10.2: "Implement automated audit trails for all system components"
- GDPR Art. 30: "Records of processing activities"
- NCSC Principle 12: "Audit information provided to consumers"

Without this, no customer can pass any audit using KCore. This is the single most important missing piece.

**Effort estimate:** 2–3 weeks engineering. No external cost.

### 2.2 Exportable compliance evidence (all standards)

Customers' auditors need documentary evidence. KCore should generate this automatically.

**What to build:**

| Artifact | Format | Content |
|----------|--------|---------|
| SBOM | CycloneDX JSON | Every Rust crate + Nix package in the release |
| Crypto inventory | Markdown / JSON | TLS version, cipher suites, key sizes, crypto library used |
| Network topology export | JSON | All networks, bridges, firewall rules, VLAN/VXLAN config per node |
| Access control matrix | Markdown / JSON | RPC methods × allowed certificate CNs |
| System configuration snapshot | Nix expression | The exact NixOS configuration running on each node |
| Node inventory | JSON via `ListNodes` | All nodes, their approval status, capacity, labels |
| VM inventory | JSON via `ListVms` | All VMs, their state, network placement, creation time |

**kctl commands to add:**

```
kctl cluster compliance-report    # Generates a bundle of all artifacts above
kctl cluster sbom                 # Prints the SBOM
kctl cluster crypto-inventory     # Prints TLS config and cipher suites in use
```

**Why this matters for customers:**
- SOC 2 auditors request evidence packages; customers should not need to manually document KCore's controls
- PCI QSAs need network diagrams, access control documentation, and software inventories
- GDPR requires a Record of Processing Activities — the data inventory feeds into this

**Effort estimate:** 2–3 weeks engineering. No external cost.

### 2.3 Role-based access control (SOC 2, PCI, NCSC)

The current model has a single `kcore-kctl` identity with full access to all operator RPCs. Customers need granular roles.

**What to build:**

| Role | Permissions |
|------|-------------|
| `admin` | All RPCs including ApplyNixConfig, ApproveNode, DrainNode |
| `operator` | CreateVm, DeleteVm, SetVmDesiredState, CreateNetwork, DeleteNetwork |
| `viewer` | GetVm, ListVms, GetNode, ListNodes, ListNetworks, ListAuditEvents |

Implementation: encode the role in the certificate CN or a custom X.509 extension (e.g., `kcore-kctl-admin`, `kcore-kctl-viewer`). The auth module checks the role against the RPC method's required role.

**Why this matters for customers:**
- SOC 2 CC6.3: "Role-based access that limits users to their required functions"
- PCI DSS 7.1: "Limit access to system components to only those individuals whose job requires such access"
- NCSC Principle 9: "Secure user management"

**Effort estimate:** 1–2 weeks engineering. No external cost.

### 2.4 FIPS-compatible crypto configuration (FIPS, PCI) — IMPLEMENTED

KCore now uses FIPS-validated cryptography by default, without requiring a runtime flag.

**What was built:**

| Item | Status |
|------|--------|
| Crypto provider switch | **Done.** `ring` replaced by `aws-lc-rs` as the `rustls` crypto backend in all three binaries (controller, node-agent, kctl). `rcgen` (certificate generation) also switched to `aws-lc-rs`. |
| FIPS cipher suites | **Done (default, no flag needed).** Each binary installs a custom `CryptoProvider` at startup restricting cipher suites to AES-GCM + ECDHE P-256/P-384 only. ChaCha20-Poly1305 and X25519 are excluded. |
| Crypto inventory API | Planned — expose active crypto configuration via the compliance report (2.2) so customers can present it to their assessors. |

**Active TLS configuration:**

| Setting | Value |
|---------|-------|
| Crypto library | `aws-lc-rs` (wraps AWS-LC, FIPS 140-3 certificate #4816) |
| TLS 1.3 cipher suites | `TLS_AES_256_GCM_SHA384`, `TLS_AES_128_GCM_SHA256` |
| TLS 1.2 cipher suites | `TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384`, `TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256`, `TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384`, `TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256` |
| Key exchange groups | `secp384r1` (P-384), `secp256r1` (P-256) |
| Excluded | ChaCha20-Poly1305, X25519, RSA key exchange |

**Why this matters for customers:**
- The customer's FIPS certification covers their system, not KCore. But if KCore uses non-FIPS crypto under the hood, the customer cannot claim FIPS compliance for their stack.
- KCore does not need its own FIPS 140-3 certificate. It uses a pre-validated library and documents that fact.
- Customer evidence: "All management plane TLS uses aws-lc-rs (AWS-LC FIPS 140-3 #4816) with AES-GCM cipher suites and NIST P-256/P-384 key exchange. No non-FIPS algorithms are negotiable."

### 2.5 Encryption at rest (GDPR, SOC 2, PCI)

Customers need to demonstrate that data at rest is encrypted. KCore stores state in SQLite and certificate files on disk.

**What to build:**

| Item | Detail |
|------|--------|
| SQLite encryption | Support SQLCipher as the SQLite backend, enabled via `database.encryption: true` in controller.yaml. Key derived from a passphrase or hardware token. |
| Disk encryption guidance | Document that KCore nodes should use dm-crypt/LUKS for full-disk encryption. Provide a reference NixOS configuration. |
| VM disk encryption | Document how customers can use guest-level encryption (LUKS inside the VM) for workload data. KCore does not manage guest disk encryption — this is the customer's responsibility. |

**Why this matters for customers:**
- GDPR Art. 32: "encryption of personal data"
- SOC 2 C1: "confidential information is protected"
- PCI DSS 3.5: "protect stored account data"

**Effort estimate:** 1–2 weeks for SQLCipher integration. Documentation only for disk/VM encryption.

### 2.6 Certificate lifecycle management (SOC 2, PCI, NCSC)

Certificates currently have no rotation or revocation workflow. Auditors will flag this.

**What to build:**

| Item | Detail |
|------|--------|
| `kctl cluster rotate-certs` | Generates new certificates, distributes them to nodes, gracefully transitions connections |
| Expiry monitoring | Controller checks certificate expiry on every heartbeat. Emits warnings at 30/7/1 days before expiry via structured log events. |
| `kctl cluster cert-status` | Shows all certificates in the cluster, their expiry dates, and health status |

**Why this matters for customers:**
- SOC 2 CC6.1: "Logical access security — encryption key management"
- PCI DSS 3.6: "Key management procedures"
- NCSC Principle 10: "Identity and authentication"

**Effort estimate:** 2–3 weeks engineering.

### 2.7 Health checks and availability monitoring (SOC 2, NCSC)

**What to build:**

| Item | Detail |
|------|--------|
| gRPC health checks | Implement `grpc.health.v1.Health` on controller and node-agent |
| Heartbeat failure alerting | Emit structured alert events when a node misses heartbeats. Optionally call a webhook. |
| `kctl cluster health` | Shows controller health, node health, and any active alerts |

**Why this matters for customers:**
- SOC 2 A1: "System availability meets the entity's objectives"
- NCSC Principle 2: "Asset protection and resilience"

**Effort estimate:** 1 week engineering.

### 2.8 Shared responsibility documentation

This is the single most important non-engineering deliverable. Customers need a clear document that tells them: "KCore handles X. You are responsible for Y."

**What to write:**

| Control area | KCore responsibility | Customer responsibility |
|-------------|---------------------|----------------------|
| Encryption in transit | mTLS on all management traffic | Encryption of guest-to-guest and guest-to-internet traffic |
| Encryption at rest | SQLCipher for controller DB, key file permissions | Full-disk encryption on nodes (LUKS), guest disk encryption |
| Access control | CN-based RBAC on gRPC API, node approval queue | Managing who holds which certificates, revoking access for departed staff |
| Network segmentation | Per-VM network isolation, VLAN, firewall rules | Defining which workloads go on which networks, designing the topology |
| Audit logging | Structured audit log of all API actions | Forwarding audit logs to SIEM, setting retention policies, monitoring alerts |
| Patch management | `cargo audit`, SBOM, NixOS atomic updates | Applying updates to KCore nodes, patching guest OS images |
| VM security | Hypervisor isolation, TAP separation, config injection prevention | Hardening guest OS, application security, vulnerability scanning inside VMs |
| Backup and recovery | `kctl cluster backup` for controller state | Backing up VM data, testing recovery procedures |
| Incident response | Audit trail, structured logging, alert webhooks | Defining incident response procedures, staffing on-call, notifying regulators |
| Physical security | Not applicable (KCore is software) | Data center physical security, hardware tamper protection |
| Compliance certification | Compliance-enabling controls, evidence export, documentation | Engaging auditors, maintaining certifications, producing compliance reports |

**Effort estimate:** 1 week to write. No external cost.

---

## 3. Mapping to specific standards

### 3.1 GDPR enablement

GDPR does not certify infrastructure — it applies to organizations processing personal data. KCore enables the customer's GDPR posture.

| GDPR Article | What KCore provides | What customer does |
|-------------|--------------------|--------------------|
| Art. 5 (data minimization) | KCore stores only operational data (node IPs, hostnames, cert CNs). No user data. | Customer ensures their VMs comply with data minimization |
| Art. 25 (privacy by design) | No telemetry, no phone-home, no data leaves the cluster | Customer documents their own privacy-by-design measures |
| Art. 30 (records of processing) | Audit log API, data inventory export | Customer maintains their ROPA using KCore's exports |
| Art. 32 (security of processing) | mTLS, RBAC, encryption at rest, VM isolation | Customer applies these controls and documents them |
| Art. 33 (breach notification) | Audit trail enables forensic investigation of when/how a breach occurred | Customer notifies supervisory authority within 72 hours |
| Art. 35 (DPIA) | Not required for KCore itself (no high-risk processing) | Customer performs DPIA for their workloads if required |

**Customer's cost to achieve GDPR using KCore:** €2,000–€5,000 for legal documentation (DPA, privacy policy). Most technical controls come from KCore itself.

### 3.2 SOC 2 Type II enablement

The customer gets audited, not KCore. KCore's controls count as "complementary subservice organization controls" in the customer's SOC 2 report.

| Trust Services Criteria | KCore provides (platform controls) | Customer provides (user entity controls) |
|------------------------|------------------------------------|-----------------------------------------|
| CC6.1 (encryption) | mTLS, FIPS mode, SQLCipher | Key management procedures, guest encryption |
| CC6.3 (access control) | RBAC with three roles, node approval | Managing certificate distribution, offboarding |
| CC7.2 (monitoring) | Structured audit logging, audit log API | SIEM integration, alert triage, incident response |
| CC8.1 (change management) | Declarative NixOS config, SBOM, signed releases | Change approval workflow, deployment procedures |
| A1.2 (availability) | Health checks, heartbeat monitoring, node drain | SLA definition, redundancy planning, DR testing |
| C1.1 (confidentiality) | Network segmentation, encryption at rest | Data classification, secrets management in VMs |
| PI1.1 (processing integrity) | Input validation, config checksums | Application-level validation, data integrity checks |

**Customer's cost to achieve SOC 2 using KCore:** $20,000–$50,000 year 1 (auditor + compliance platform). KCore's compliance report export (2.2) significantly reduces evidence-gathering effort.

### 3.3 PCI DSS 4.0 enablement

PCI applies only if the customer's VMs process cardholder data. KCore is an "in-scope system component" in the customer's CDE.

| PCI Requirement | KCore provides | Customer provides |
|----------------|----------------|-------------------|
| 1.3 (network segmentation) | NAT/VLAN/VXLAN network isolation, per-network firewall rules | Define which networks are PCI scope, validate segmentation |
| 2.2 (system hardening) | NixOS minimal install, no unnecessary services | Guest OS hardening, application hardening |
| 4.2 (strong cryptography) | mTLS with configurable cipher suites, FIPS mode | Ensure FIPS mode is enabled, document crypto config |
| 6.3 (software inventory) | SBOM shipped with every release | Vulnerability management workflow using the SBOM |
| 7.1 (restrict access) | RBAC with admin/operator/viewer roles | Map roles to staff, review access quarterly |
| 8.3 (MFA) | Client certificate on hardware token (YubiKey/PIV support) | Enforce hardware token usage, manage token inventory |
| 10.2 (audit trails) | Structured audit logging with tamper-evident option | Forward to SIEM, retain for 12 months, review daily |
| 11.5 (file integrity) | NixOS immutable store, config generation checksums | Monitor for unauthorized changes, investigate alerts |

**Customer's cost to achieve PCI using KCore:** $40,000–$100,000 year 1 (QSA + scanning + pen test). KCore reduces scope by providing platform-level controls out of the box.

### 3.4 FIPS 140-3 enablement

The customer's system must use FIPS-validated cryptography. KCore enables this without being FIPS-certified itself.

| Component | KCore provides | Customer provides |
|-----------|----------------|-------------------|
| TLS library | `aws-lc-rs` (FIPS 140-3 certificate #4816) as `rustls` backend | Enable `--fips` flag, verify cipher suites |
| Cipher suites | FIPS-approved only when `--fips` is set | Document that FIPS mode is enabled in their security policy |
| Key management | Certificate generation, rotation, and destruction tools | Follow key management procedures, protect CA key |
| Kernel crypto | Documentation for `fips=1` kernel parameter on NixOS | Enable kernel FIPS mode on all nodes |

**Customer's cost to claim FIPS-compliant stack:** $0 external cost if using KCore's FIPS mode with `aws-lc-rs`. The customer documents the validated module and its certificate number in their own FIPS security policy. KCore does not need its own FIPS certificate.

---

## 4. API capabilities for compliance

### 4.1 Existing gRPC APIs useful for compliance

| API | Compliance use |
|-----|---------------|
| `ListNodes` | Inventory of all compute assets — SOC 2 CC6.2, PCI 2.4 |
| `ListVms` | Inventory of all workloads — SOC 2 CC6.2, PCI 2.4 |
| `GetNode` / `GetVm` | Detailed asset information for audit evidence |
| `ListNetworks` | Network topology documentation — PCI 1.3, NCSC Principle 3 |
| `ApproveNode` / `RejectNode` | Access approval workflow — SOC 2 CC6.3 |
| `DrainNode` | Controlled workload migration for maintenance — SOC 2 A1 |
| `Heartbeat` + node status | Availability monitoring — SOC 2 A1, NCSC Principle 2 |

### 4.2 New gRPC APIs to add

| API | Purpose | Compliance use |
|-----|---------|---------------|
| `ListAuditEvents` | Query audit trail with filters | All standards — core compliance evidence |
| `GetClusterHealth` | Cluster-wide health status | SOC 2 A1, NCSC Principle 2 |
| `GetCryptoConfig` | Active TLS version, cipher suites, crypto library | FIPS evidence, PCI 4.2 |
| `GetComplianceReport` | Bundle of all compliance artifacts | Evidence package for auditors |
| `RotateCertificates` | Trigger certificate rotation | SOC 2 CC6.1, PCI 3.6 |
| `GetCertificateStatus` | Expiry dates and health for all certs | SOC 2 CC6.1, PCI 3.6 |
| `ExportSbom` | Return the SBOM for the running version | PCI 6.3, SOC 2 CC8 |

### 4.3 Webhook / event system

For customers integrating KCore with their SIEM, monitoring, or ticketing systems:

| Event | Trigger | Payload |
|-------|---------|---------|
| `audit.event` | Every state-changing API call | Full audit event record |
| `node.heartbeat.missed` | Node misses heartbeat threshold | Node ID, last seen, duration |
| `cert.expiry.warning` | Certificate approaching expiry | Certificate CN, expiry date, days remaining |
| `node.registered` | New node registers | Node ID, hostname, approval status |
| `vm.state.changed` | VM transitions state | VM ID, old state, new state, actor |

Webhook configuration in controller.yaml:

```yaml
webhooks:
  - url: https://customer-siem.example.com/kcore
    events: ["audit.event", "node.heartbeat.missed", "cert.expiry.warning"]
    headers:
      Authorization: "Bearer <token>"
```

