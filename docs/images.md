# VM Images Workflow

This guide documents the supported VM image workflows in `kcore`, including node upload, VM creation from uploaded images, and readiness waits.

## Supported image formats

- `qcow2`
- `raw`

Current upload and VM image workflows do **not** support `iso`.

## Workflow A: Controller-managed URL image

Use this when you want the node to download and verify the image from an HTTPS URL.

```bash
kcore-kctl create vm web-01 \
  --image "https://cloud.debian.org/images/cloud/bookworm/latest/debian-12-genericcloud-amd64.qcow2" \
  --image-sha256 "<sha256>" \
  --network default \
  --cpu 2 \
  --memory 4G
```

Behavior:

- controller stores desired VM state
- controller asks node-agent to ensure/download image
- image is cached under `/var/lib/kcore/images/...`
- node-agent applies generated Nix config

## Workflow B: Node-local upload (recommended for large/local images)

Use this two-step flow to upload once and reuse image path for VM creation.

### 1) Upload image to node cache

```bash
kcore-kctl --node 192.168.40.105:9091 node upload-image \
  -f /mnt/md126/downloads/noble-server-cloudimg-amd64.img \
  --name ubuntu-24.04-noble-cloudimg-amd64.qcow2 \
  --format qcow2
```

Optional checksum validation during upload:

```bash
kcore-kctl --node 192.168.40.105:9091 node upload-image \
  -f /mnt/md126/downloads/ubuntu-24.04-noble-cloudimg-amd64.raw \
  --name ubuntu-24.04-noble-cloudimg-amd64.raw \
  --format raw \
  --image-sha256 bd8ce062216be71fe4f1af6eaa527069b3805c85a32cc601ad2fad6786967ee6
```

Notes:

- upload is chunked/streaming, so multi-GB images are supported
- output includes final path, format, size, and computed SHA256

### 2) Create VM from uploaded node-local path

```bash
kcore-kctl create vm ubuntu-noble-1 \
  --image-path /var/lib/kcore/images/ubuntu-24.04-noble-cloudimg-amd64.qcow2 \
  --image-format qcow2 \
  --network default \
  --cpu 2 \
  --memory 4G
```

## Wait for readiness during create

### Wait for running state

```bash
kcore-kctl create vm app-01 \
  --image-path /var/lib/kcore/images/app-base.qcow2 \
  --image-format qcow2 \
  --network default \
  --wait
```

### Wait for SSH reachability

```bash
kcore-kctl create vm app-ssh-01 \
  --image-path /var/lib/kcore/images/app-base.qcow2 \
  --image-format qcow2 \
  --network default \
  --wait-for-ssh \
  --wait-timeout-seconds 300 \
  --ssh-port 22
```

`--wait-for-ssh` checks:

- VM reaches `Running`
- node-agent resolves VM IP from DHCP lease file(s)
- node-agent probes TCP SSH port from node host

## Important constraints

- A writable disk image path can only be attached to one VM at a time on a node.
  - creating a second VM with the same `--image-path` is rejected with `FailedPrecondition`.
- If create cannot be applied to node config, controller rolls back VM insert (no partial stale VM row).

## Troubleshooting

- `no connection to node ...`
  - node may need re-registration or controller-node reconnect
  - check `kcore-kctl get nodes` and node/controller services
- `image path ... already used by VM ...`
  - upload/copy a second image file and use a different `--image-path`
- `no DHCP lease found for VM yet` during `--wait-for-ssh`
  - guest did not obtain DHCP lease yet; inspect VM unit logs and `/run/kcore/dnsmasq-*.leases`
