# Certifications and Compliance Roadmap

This document lays out a practical roadmap for achieving GDPR, SOC 2 Type II, PCI DSS, FIPS 140-3, and SBOM compliance in kcore. Each section describes what the standard requires, what kcore already provides, the gaps, and the concrete work items to close them.

## Current baseline

kcore already implements several controls that are prerequisites across multiple standards:

- **mTLS everywhere** — all inter-component communication (kctl, controller, node-agent) uses mutual TLS with a self-managed CA
- **CN-based gRPC authorization** — every RPC method enforces caller identity via certificate Common Name
- **Private key protection** — key files written with mode `0o600`, CA key never leaves the operator machine
- **Input sanitization** — Nix injection prevention via `nix_escape()` and `sanitize_nix_attr_key()`
- **Dependency auditing** — `cargo audit` runs as part of `make check`
- **Declarative infrastructure** — NixOS provides reproducible, auditable system configurations
- **No unsafe code** — the entire codebase runs within Rust's safety guarantees

---

## 1. SBOM — Software Bill of Materials

**Goal:** Ship a machine-readable SBOM with every kcore release so downstream consumers can track components, licenses, and vulnerabilities.

### What is needed

An SBOM in a standard format (SPDX or CycloneDX) listing every dependency (Rust crates, Nix packages, system libraries, VM images) included in a release artifact.

### Work items

| # | Task | Detail |
|---|------|--------|
| 1.1 | Add `cargo-sbom` or `cargo-cyclonedx` to the Nix dev shell | Generates a CycloneDX JSON from `Cargo.lock` at build time |
| 1.2 | Create a Makefile target `make sbom` | Runs the generator and writes `sbom.cdx.json` into the release directory |
| 1.3 | Integrate SBOM generation into the ISO build | The NixOS ISO build (`build-iso.sh` / flake) should embed the SBOM inside the image at a well-known path (e.g., `/etc/kcore/sbom.cdx.json`) |
| 1.4 | Add Nix-level SBOM | Use `nix-sbom` or a custom derivation to capture the full Nix closure (system packages, firmware, kernel modules) and merge it with the Cargo-level SBOM |
| 1.5 | Expose SBOM via `kctl` | Add a `kctl cluster sbom` command that retrieves and prints the SBOM from a running controller or local build |
| 1.6 | Automate SBOM diff on release | CI compares the new SBOM against the previous release and flags added/removed/upgraded dependencies in the release notes |
| 1.7 | License compliance check | Integrate `cargo-deny` to enforce an allowlist of acceptable licenses and flag copyleft or unknown licenses in the dependency tree |

### Priority

**High — do this first.** SBOM is a prerequisite for PCI DSS 4.0 (Requirement 6.3.2), helps with SOC 2 vendor management, and is increasingly required by enterprise procurement. It is also the lowest-effort item on this list.

---

## 2. FIPS 140-3 — Cryptographic Module Compliance

**Goal:** Run kcore on a FIPS 140-3 validated operating environment so that all cryptographic operations (TLS, certificate generation, hashing) use FIPS-approved algorithms and validated implementations.

### Current state

kcore uses `ring` (via `rustls` and `rcgen`) for all cryptographic operations. `ring` uses BoringSSL's cryptographic core internally, which has a FIPS-validated variant (BoringCrypto), but the `ring` crate itself is **not** FIPS-validated.

### Work items

| # | Task | Detail |
|---|------|--------|
| 2.1 | Evaluate `rustls` with `aws-lc-rs` backend | `aws-lc-rs` wraps AWS-LC, which has an active FIPS 140-3 validation (certificate #4816). `rustls` supports `aws-lc-rs` as a pluggable crypto provider. Switch from `ring` to `aws-lc-rs` for TLS. |
| 2.2 | Switch `rcgen` to use FIPS-validated primitives | `rcgen` 0.14+ supports pluggable crypto backends. Evaluate whether it can use `aws-lc-rs` for certificate generation, or wrap certificate generation in OpenSSL FIPS calls. |
| 2.3 | Kernel-level FIPS mode | Configure the NixOS kernel with `fips=1` boot parameter. This enables the kernel's FIPS mode, which restricts `/dev/random`, disables non-approved algorithms in the kernel crypto API, and runs power-on self-tests. |
| 2.4 | Disable non-FIPS TLS cipher suites | Configure `rustls` to only offer FIPS-approved cipher suites: TLS 1.2 with AES-GCM + ECDHE (P-256/P-384), TLS 1.3 with AES-128-GCM/AES-256-GCM. Disable ChaCha20-Poly1305 (not FIPS-approved). |
| 2.5 | Add `--fips` flag to controller and node-agent | When set, restrict cipher suites, reject non-FIPS key sizes, and log the FIPS mode status at startup. Fail to start if the crypto provider's self-test fails. |
| 2.6 | Document the FIPS boundary | Produce a FIPS security policy document describing the cryptographic boundary: what modules are validated, what algorithms are used, what keys exist, and how they are protected. |
| 2.7 | Automated FIPS regression tests | Add CI tests that start the controller and node-agent with `--fips`, perform a TLS handshake, and verify that only approved cipher suites were negotiated. |

### Priority

**Medium-high.** Required for US federal and financial-sector deployments. The `aws-lc-rs` migration (2.1) is the critical path — everything else follows from it.

---

## 3. GDPR — General Data Protection Regulation

**Goal:** Ensure kcore can be deployed in environments that process EU personal data, and that kcore itself does not create GDPR liability.

### Scope assessment

kcore is infrastructure software that manages VMs. It does **not** process end-user personal data directly. However:

- kcore **does** store operator-level data: node hostnames, IP addresses, certificate CNs, and audit logs
- kcore-managed VMs **may** process personal data — kcore must not interfere with the data subject's rights
- NixOS configurations generated by kcore are stored on disk and may reference identifiable infrastructure

### Work items

| # | Task | Detail |
|---|------|--------|
| 3.1 | Data inventory | Document exactly what data kcore stores (SQLite tables, certificate files, log files, Nix configs), classify each field as personal/non-personal per GDPR Article 4, and record retention periods |
| 3.2 | Audit logging | Add structured audit logs for all state-changing operations (VM create/delete, node register/deregister, config apply). Each log entry must include: timestamp, actor identity (CN), action, target resource, and outcome. Store in append-only format. |
| 3.3 | Log retention and rotation | Implement configurable log retention with automatic purging. Default to 90 days. Ensure deleted logs cannot be recovered (overwrite or use encrypted storage with key destruction). |
| 3.4 | Data subject access and erasure for operator data | Implement `kctl cluster purge-node <node>` that removes all traces of a node from the database, logs, and generated configs. Document the process for responding to a data access request about operator-identifiable information. |
| 3.5 | Encryption at rest | Encrypt the SQLite database at rest using SQLCipher or a dm-crypt volume. Certificate private keys are already permission-restricted but should also reside on an encrypted filesystem. |
| 3.6 | Data Processing Agreement template | Provide a DPA template in `docs/` that kcore operators can use with their own customers, clarifying that kcore is a processor/sub-processor and describing the technical measures in place. |
| 3.7 | Privacy by design documentation | Document the data minimization principles applied: kcore stores only the data necessary for VM orchestration, does not store VM contents or guest OS data, and does not phone home or transmit telemetry. |

### Priority

**Medium.** GDPR applies immediately if kcore is deployed in the EU. Items 3.1 and 3.2 should be done early because they are also prerequisites for SOC 2.

---

## 4. SOC 2 Type II

**Goal:** Demonstrate that kcore meets the Trust Services Criteria (security, availability, processing integrity, confidentiality, privacy) through sustained, auditable controls over a review period (typically 6–12 months).

### What SOC 2 Type II requires

Unlike a point-in-time certification, SOC 2 Type II requires **evidence that controls operated effectively over time**. This means logging, monitoring, change management, and incident response — not just having the right code.

### Work items

#### Security (CC6 — Logical and Physical Access Controls)

| # | Task | Detail |
|---|------|--------|
| 4.1 | Role-based access control | Extend the CN-based authorization model to support distinct roles (admin, operator, viewer) with different RPC permissions. Currently all kctl users have identical access. |
| 4.2 | Certificate lifecycle management | Implement certificate rotation: `kctl cluster rotate-certs` that generates new certs, distributes them to nodes, and revokes the old ones. Add expiry monitoring with warnings at 30/7/1 days before expiry. |
| 4.3 | Session and connection logging | Log every gRPC connection: source IP, certificate CN, connection time, and disconnection time. Store alongside audit logs from 3.2. |

#### Availability (A1 — System Availability)

| # | Task | Detail |
|---|------|--------|
| 4.4 | Health check endpoints | Add gRPC health checking protocol support (`grpc.health.v1.Health`) to controller and node-agent. Expose readiness and liveness probes. |
| 4.5 | Heartbeat failure alerting | When a node misses heartbeats beyond the threshold, emit a structured alert event (log + optional webhook). Document the expected availability SLA. |
| 4.6 | Backup and recovery | Implement `kctl cluster backup` that snapshots the SQLite database and certificate store. Document the recovery procedure and test it. |

#### Processing Integrity (PI1 — Completeness and Accuracy)

| # | Task | Detail |
|---|------|--------|
| 4.7 | Config generation checksums | After generating a Nix config, compute and store a SHA-256 hash. The node-agent should verify the hash before applying. This ensures configs are not tampered with in transit (defense in depth beyond mTLS). |
| 4.8 | Idempotent apply with generation counters | Add a monotonic generation counter to each config push. Node-agent rejects configs with a generation counter less than or equal to the currently applied one. Prevents replay and stale-config bugs. |

#### Confidentiality (C1 — Protection of Confidential Information)

| # | Task | Detail |
|---|------|--------|
| 4.9 | Secrets management | VM cloud-init configs may contain sensitive data (passwords, SSH keys). Ensure these are encrypted at rest in the database and only decrypted during Nix config generation. |
| 4.10 | Network segmentation documentation | Document the expected network architecture: management plane (gRPC between controller/nodes/kctl) vs. data plane (VM traffic). Provide reference NixOS firewall rules. |

#### Change Management (CC8)

| # | Task | Detail |
|---|------|--------|
| 4.11 | Signed releases | Sign release binaries and ISO images with a GPG or Sigstore key. Publish the public key in the repository. |
| 4.12 | Change log automation | Generate a changelog from conventional commits. Include in each release alongside the SBOM. |

#### Monitoring and Incident Response (CC7)

| # | Task | Detail |
|---|------|--------|
| 4.13 | Structured logging with levels | Standardize all logging to structured JSON format with consistent fields (timestamp, level, component, message, trace_id). |
| 4.14 | Incident response runbook | Document how to respond to: compromised node certificate, unauthorized API access, failed config apply, data corruption in SQLite. |

### Priority

**Medium-high.** SOC 2 is the most-requested compliance standard for B2B infrastructure software. Start the audit period as soon as items 3.2, 4.1–4.3, and 4.13 are in place — the clock starts when controls are operating, and the audit needs 6–12 months of evidence.

---

## 5. PCI DSS 4.0 — Payment Card Industry Data Security Standard

**Goal:** Enable kcore to host VMs that are in scope for PCI DSS compliance (e.g., VMs running payment processing applications).

### Scope

kcore itself does not handle cardholder data, but as the hypervisor management layer it is part of the Cardholder Data Environment (CDE) if any managed VM processes payment data. PCI DSS 4.0 Requirements that apply to kcore as a system component:

### Work items

| # | Task | Detail |
|---|------|--------|
| 5.1 | Network segmentation enforcement | Implement network policies that isolate PCI-scoped VMs from non-PCI VMs at the network level. This means separate bridge networks, firewall rules, and potentially separate physical nodes. Add `pci_scope: bool` to VM metadata. |
| 5.2 | Access control with MFA | PCI Requirement 8.3: multi-factor authentication for all administrative access. Integrate kctl with an external MFA provider (e.g., TOTP via a PAM module, or client certificate + hardware token). |
| 5.3 | Vulnerability management | PCI Requirement 6.3: maintain an inventory of custom and third-party software components (covered by SBOM — item 1.x). Add automated vulnerability scanning of the SBOM against NVD/OSV databases in CI. |
| 5.4 | File integrity monitoring | PCI Requirement 11.5: detect unauthorized changes to critical system files. Implement FIM for `/etc/nixos/`, `/etc/kcore/`, and kcore binaries. NixOS's immutable store helps here — alert if any store path is modified outside of `nixos-rebuild`. |
| 5.5 | Penetration testing support | PCI Requirement 11.4: regular penetration testing. Document the attack surface (gRPC endpoints, node-agent API, NixOS management interface) and provide a testing guide for assessors. |
| 5.6 | Audit trail with tamper detection | PCI Requirement 10: log all access to system components. Extend audit logging (3.2) with tamper-evident properties: hash-chain each log entry so that deletion or modification of historical entries is detectable. |
| 5.7 | Clock synchronization | PCI Requirement 10.6: synchronize clocks. Document NTP/chrony configuration requirements for kcore nodes and verify synchronization in heartbeat responses. |

### Priority

**Lower.** PCI compliance is only relevant if kcore is used to host payment workloads. Many items overlap with SOC 2 (audit logging, access control, change management). Address PCI-specific items (5.1, 5.2, 5.6) after the SOC 2 foundation is in place.

---

## Implementation order

The roadmap is ordered to maximize reuse — earlier phases produce artifacts and controls that later phases depend on.

```
Phase 1: Foundation (months 1–2)
├── SBOM generation (1.1–1.7)
├── Audit logging (3.2, 4.13)
└── Data inventory (3.1)

Phase 2: Cryptographic hardening (months 2–4)
├── FIPS 140-3 crypto provider switch (2.1–2.2)
├── FIPS kernel mode (2.3)
├── Cipher suite restriction (2.4–2.5)
└── Encryption at rest (3.5)

Phase 3: Access control and lifecycle (months 3–5)
├── RBAC (4.1)
├── Certificate rotation (4.2)
├── Health checks and alerting (4.4–4.5)
└── Backup and recovery (4.6)

Phase 4: Integrity and change management (months 4–6)
├── Config checksums and generation counters (4.7–4.8)
├── Signed releases (4.11)
├── SBOM diff and changelog automation (1.6, 4.12)
└── File integrity monitoring (5.4)

Phase 5: SOC 2 audit period begins (month 6)
├── All CC6/CC7/CC8 controls operating
├── Evidence collection running
└── 6–12 month observation period

Phase 6: PCI-specific controls (months 6–9, if needed)
├── Network segmentation for PCI VMs (5.1)
├── MFA integration (5.2)
├── Tamper-evident audit trail (5.6)
└── Penetration testing documentation (5.5)

Phase 7: Certification (months 12–18)
├── SOC 2 Type II report issued
├── PCI DSS SAQ or ROC (if applicable)
├── FIPS 140-3 security policy published
└── GDPR documentation package complete
```

## Dependencies between standards

```
SBOM ──────────────► PCI 6.3 (vulnerability management)
                 └─► SOC 2 CC8 (change management)

Audit logging ─────► SOC 2 CC7 (monitoring)
                 └─► PCI 10 (audit trails)
                 └─► GDPR Art. 30 (records of processing)

FIPS crypto ───────► PCI 4.2 (strong cryptography)
                 └─► SOC 2 CC6.1 (encryption)

RBAC ──────────────► SOC 2 CC6.3 (access control)
                 └─► PCI 7 (restrict access)
                 └─► GDPR Art. 32 (security of processing)

Encryption at rest ► SOC 2 C1 (confidentiality)
                 └─► PCI 3.5 (protect stored data)
                 └─► GDPR Art. 32 (encryption)
```

## Estimated total effort

| Phase | Effort | Dependencies |
|-------|--------|--------------|
| 1 — Foundation | 2–3 weeks | None |
| 2 — Crypto hardening | 3–4 weeks | Phase 1 |
| 3 — Access & lifecycle | 3–4 weeks | Phase 1 |
| 4 — Integrity & change mgmt | 2–3 weeks | Phases 2, 3 |
| 5 — SOC 2 audit period | 6–12 months (elapsed) | Phase 4 |
| 6 — PCI-specific | 3–4 weeks | Phase 4 |
| 7 — Certification | 2–4 months (elapsed) | Phases 5, 6 |

Total engineering effort: approximately 3–4 months of focused work, spread across a 12–18 month calendar timeline driven by the SOC 2 observation period.
