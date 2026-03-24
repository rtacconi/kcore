# Node Install Bootstrap Flow

This document defines the node installation procedure with cluster-scoped PKI material and certificate handoff from `kctl` to the target node.

## Goal

Install a node from live ISO to disk and ensure that, after reboot:
- `kcore-node-agent` can start with valid TLS files in `/etc/kcore/certs`
- the node can join or host the controller as configured
- certificate trust is anchored in the selected cluster CA

## Cluster-scoped PKI layout

Expected local layout on the operator machine:

- `~/.kcore/config` (contexts and current context)
- `~/.kcore/<cluster-name>/ca.crt`
- `~/.kcore/<cluster-name>/ca.key`
- `~/.kcore/<cluster-name>/controller.crt`
- `~/.kcore/<cluster-name>/controller.key`
- `~/.kcore/<cluster-name>/kctl.crt`
- `~/.kcore/<cluster-name>/kctl.key`

`kctl` selects a cluster context, resolves its cert directory, and uses that material for bootstrap.

## Procedure

1. Create/select cluster context and PKI.
2. Boot target host from Kcore ISO and confirm `node-agent` API is reachable.
3. Discover target devices (`node disks`, `node nics`).
4. Run `node install` with:
   - OS disk (required)
   - optional data disks
   - join controller endpoint
5. `kctl` prepares install PKI payload:
   - loads cluster CA and existing cert/key material
   - generates node cert/key signed by cluster CA (SAN = node host/IP)
6. `kctl` sends `InstallToDiskRequest` including cert PEM payload.
7. Live `node-agent` writes certs to `/etc/kcore/certs` and starts `install-to-disk`.
8. Installer copies `/etc/kcore/*` into `/mnt/etc/kcore` on target disk.
9. `nixos-install` completes and host reboots from installed disk.
10. Installed services read `/etc/kcore/certs/*` and start successfully.

## Detailed flowchart

```mermaid
flowchart TD
  user[Operator] --> createCluster["kctl create cluster --name clusterName"]
  createCluster --> clusterDir["Create ~/.kcore/clusterName with CA and certs"]
  clusterDir --> selectCtx["Set current context in ~/.kcore/config"]

  user --> bootIso["Boot target host from Kcore ISO"]
  bootIso --> nodeAgentLive["Live node-agent on host:9091"]

  user --> discover["kctl --node host:9091 node disks/nics"]
  discover --> installCmd["kctl node install --os-disk --data-disk --join-controller"]

  installCmd --> loadCtx["Resolve current context and cluster cert dir"]
  loadCtx --> loadClusterPki["Load ca/controller/kctl certs and keys"]
  loadClusterPki --> genNodeCert["Generate node cert/key signed by cluster CA"]
  genNodeCert --> buildReq["Build InstallToDiskRequest with PEM payload"]
  buildReq --> sendRpc["Send NodeAdmin.InstallToDisk RPC"]

  sendRpc --> writeBootstrap["Live node-agent writes /etc/kcore/certs/*"]
  writeBootstrap --> runInstaller["Spawn install-to-disk and log output"]
  runInstaller --> partitionDisk["Partition/format/mount target disk"]
  partitionDisk --> copyKcore["Copy /etc/kcore and binaries to /mnt"]
  copyKcore --> writeNixos["Write /mnt/etc/nixos/configuration.nix"]
  writeNixos --> nixosInstall["Run nixos-install"]
  nixosInstall --> rebootHost["Reboot from installed disk"]

  rebootHost --> startServices["systemd starts kcore-node-agent and optional kcore-controller"]
  startServices --> tlsReady["Services load /etc/kcore/certs and bind 9091/9090"]
  tlsReady --> healthy["Node is reachable and ready for reconciliation"]
```

## Verification checklist

- On live ISO (before install):
  - `kctl --node <host:9091> --insecure node disks`
  - `kctl --node <host:9091> --insecure node nics`
- During install:
  - install response includes accepted status and log path
  - logs show full installer progression and final status
- After reboot:
  - `findmnt /` shows root on installed disk
  - `/etc/kcore/certs` exists with expected files
  - `systemctl is-active kcore-node-agent` is `active`
  - if same-host controller mode, `systemctl is-active kcore-controller` is `active`

