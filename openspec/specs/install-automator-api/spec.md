# install-automator-api Specification

## Purpose

Provide a Talos-style HTTP API on kcoreOS nodes for listing block devices and triggering disk installation. When the node is bootstrapped (live ISO), the API allows unauthenticated access. When kcoreOS is installed, unauthenticated methods are blocked, install is disabled, and GET /disks requires client certificate authentication.

## Requirements

### Requirement: Bootstrap vs installed mode detection

The automator SHALL determine at startup whether the node is in bootstrap mode (live kcoreOS, e.g. from ISO) or installed mode (kcoreOS already on disk).

#### Scenario: Bootstrap mode when root is read-only

- **GIVEN** the node has booted from live media (e.g. ISO)
- **WHEN** the automator starts
- **THEN** it SHALL treat the node as bootstrap (e.g. root filesystem is read-only or `/etc/kcore/installed` does not exist)

#### Scenario: Installed mode when marker exists

- **GIVEN** the file `/etc/kcore/installed` exists (created by install-to-disk at end of install)
- **WHEN** the automator starts
- **THEN** it SHALL treat the node as installed

---

### Requirement: GET /disks — list block devices

The automator SHALL expose GET /disks to return a list of block devices on the node (e.g. name, size, type, model).

#### Scenario: GET /disks in bootstrap mode — no authentication

- **GIVEN** the node is in bootstrap mode
- **WHEN** a client sends GET /disks with no credentials
- **THEN** the server SHALL return 200 with a JSON body listing block devices (e.g. lsblk-style or equivalent)
- **AND** the response SHALL include at least device name and size for each disk

#### Scenario: GET /disks in installed mode — requires client certificate

- **GIVEN** the node is in installed mode
- **WHEN** a client sends GET /disks without a valid client certificate (mTLS)
- **THEN** the server SHALL return 401 Unauthorized

#### Scenario: GET /disks in installed mode — with valid client certificate

- **GIVEN** the node is in installed mode
- **WHEN** a client sends GET /disks with a valid client certificate accepted by the server
- **THEN** the server SHALL return 200 with the same JSON listing of block devices as in bootstrap mode

---

### Requirement: POST /install — trigger install to disk

The automator SHALL expose POST /install to trigger the install-to-disk process (partitioning the target disk and installing kcoreOS). The request body MAY specify the target disk (e.g. JSON `{"os_disk": "sda"}`) or a path to an install manifest file.

#### Scenario: POST /install in bootstrap mode — no authentication

- **GIVEN** the node is in bootstrap mode
- **WHEN** a client sends POST /install with an optional body (e.g. `{"os_disk": "sda"}` or `{"manifest_path": "/etc/kcore/install-manifest.yaml"}`)
- **THEN** the server SHALL start install-to-disk non-interactively (no user prompts)
- **AND** the server SHALL return 202 Accepted with a JSON body indicating the install has started (e.g. `{"status": "started"}`)
- **AND** commands used during install (e.g. wipefs) SHALL be run with flags that prevent interactive confirmation (e.g. wipefs -af)

#### Scenario: POST /install in installed mode — blocked

- **GIVEN** the node is in installed mode
- **WHEN** a client sends POST /install (with or without credentials)
- **THEN** the server SHALL return 403 Forbidden (or equivalent) and SHALL NOT start install-to-disk

---

### Requirement: Transport and port

The automator SHALL listen on a configurable TCP port (default 9092). In bootstrap mode it SHALL serve plain HTTP; in installed mode it SHALL serve HTTPS and require client certificate for GET /disks.

#### Scenario: Bootstrap — HTTP on port 9092

- **GIVEN** the node is in bootstrap mode
- **WHEN** the automator is running
- **THEN** it SHALL listen on port 9092 using HTTP (no TLS)

#### Scenario: Installed — HTTPS and client auth

- **GIVEN** the node is in installed mode
- **WHEN** the automator is running
- **THEN** it SHALL listen on port 9092 using HTTPS
- **AND** it SHALL require and verify client certificates for GET /disks
- **AND** server certificates MAY be the same as those used by the node-agent (e.g. under `/etc/kcore/`)

---

### Requirement: Unauthenticated methods blocked when installed

When the node is in installed mode, the automator SHALL NOT allow unauthenticated access to any endpoint that returns sensitive data or performs destructive actions.

#### Scenario: Install blocked when installed

- **GIVEN** the node is in installed mode
- **WHEN** any client (authenticated or not) sends POST /install
- **THEN** the server SHALL respond with 403 and SHALL NOT run install-to-disk

#### Scenario: GET /disks requires cert when installed

- **GIVEN** the node is in installed mode
- **WHEN** a client sends GET /disks without a valid client certificate
- **THEN** the server SHALL respond with 401

---

### Requirement: Install marker for installed mode

The install-to-disk process SHALL create a marker file (e.g. `/etc/kcore/installed`) at the end of a successful installation so that on the next boot the automator detects the node as installed.

#### Scenario: Marker created after successful install

- **GIVEN** install-to-disk has completed successfully (partitioning, nixos-install, etc.)
- **WHEN** the script finishes
- **THEN** it SHALL create `/etc/kcore/installed` (or equivalent) so that after reboot the automator runs in installed mode
