# kcore appliance local console (Ratatui)

kcore nodes ship a **read-only, full-screen TUI** on the primary virtual
terminal (`/dev/tty1`) in place of a local shell login. Administration is
intended to happen over **SSH** and the **remote control plane** (`kcorectl` / API).

- **Implementation:** `crates/kcore-console` (Ratatui + crossterm)
- **NixOS / kcoreOS:** `modules/kcore-branding.nix` enables
  `systemd.services.kcore-console` and disables agetty/autovt
- **Reference unit (other distros / inspection):** `packaging/systemd/kcore-console.service`

## Disabling getty and autovt

Hiding the classic Linux login is **not** limited to `getty@tty1`. NixOS also
wires the first virtual console through **`autovt@tty1`**. You should mask
**all** of the following so operators cannot `Ctrl+Alt+F2`–`F6` into a login
getty and obtain a local shell (unless you deliberately re-enable a rescue path):

```bash
systemctl stop getty@tty1.service 2>/dev/null || true
for i in 1 2 3 4 5 6; do
  systemctl mask "getty@tty${i}.service" 2>/dev/null || true
done
systemctl mask 'autovt@.service' 2>/dev/null || true
```

Then enable the appliance console and reload:

```bash
systemctl enable kcore-console.service
systemctl daemon-reload
systemctl start kcore-console.service
```

NixOS declarations (see `kcore-branding.nix`) already set
`systemd.services."getty@tty*".enable = false` and
`systemd.services."autovt@".enable = false`.

## Development vs production

| Mode | Invocation | Quit |
| --- | --- | --- |
| Development | `kcore-console --dev` (or `cargo run -p kcore-console -- --dev`) | `q` and `Ctrl+C` exit the process. |
| Production (default) | `kcore-console` or with `--tty /dev/tty1` under systemd | `q` and `Ctrl+C` are ignored so the TUI never drops to a local shell. |

`systemd` is configured with `Restart=always` so the TUI is restarted on crash
or exit.

## Operator usage

On a running kcore node, the physical or virtual console should display the
appliance TUI automatically on `tty1`.

- Use **Tab**, **Right**, or **Left** to switch between pages.
- Use **Up** and **Down** to move the highlighted row inside Network and
  Storage tables.
- Press **r** to refresh inventory immediately.
- Press **?** or **h** for Help.
- In production, **q** and **Ctrl+C** are intentionally ignored.

The console pages are:

| Page | Purpose |
| --- | --- |
| Overview | Product, node, version, API endpoint, management URL, health, uptime, local-login status |
| Network | NIC inventory: interface, state, MAC, IPv4/IPv6, MTU, speed, driver, default-route and management markers |
| Storage | Disk inventory: device, path, model, serial, size, SSD/HDD/NVMe, read-only flag, mountpoints, health, role |
| Diagnostics | Local kcore service status for node-agent, controller, and dashboard |
| Help | Security model and operator reminders |

This screen is not a recovery shell. If the TUI shows `API: unavailable`, keep
using SSH or out-of-band management for troubleshooting; the console will still
render local NIC and disk inventory where Linux can provide it.

## Quiet boot (GRUB)

To reduce serial / framebuffer noise during boot, set in `/etc/default/grub`
(or the equivalent in your image):

```bash
GRUB_CMDLINE_LINUX_DEFAULT="quiet loglevel=3 systemd.show_status=false"
```

Regenerate the GRUB config (Debian/Ubuntu style):

```bash
sudo update-grub
```

NixOS:

```nix
boot.kernelParams = [ "quiet" "loglevel=3" "systemd.show_status=false" ];
```

Optional: **Plymouth** can be enabled later for a vendor splash; this is
orthogonal to the TTY console.

## Bootloader and recovery hardening (production)

A real **appliance** should not allow trivial recovery or kernel bypass:

- UEFI **firmware** password; disable legacy boot and unused boot options
- **GRUB** password; disable **editor** and **single-user** boot unless required
- **Secure Boot** where the distribution supports it and you can maintain keys
- Avoid `init=/bin/sh` and similar in kernel parameters in production
- For IPMI, restrict serial-over-LAN to trusted management networks

kcore will document operator workflows separately; the **local TUI only shows
status** and is not a privileged management shell.

## Security model (summary)

- The appliance console is **read-only** from an operator’s perspective: no
  local login, no shell, no in-band reboot or power actions from the TUI in
  this first implementation.
- **Reboot** / **shutdown** / break-glass should be **remote** (authenticated
  API) or a **separate, audited recovery path** (e.g. recovery mode with
  one-time token).
- The node-agent gRPC on `127.0.0.1:9091` is probed for a simple “API:
  available” line; a failure still allows the TUI to run.

## Environment variables (optional)

| Variable | Effect |
| --- | --- |
| `KCORE_MANAGEMENT_URL` | Full management URL; overrides derived `https://ip:port` |
| `KCORE_MANAGEMENT_IP` / `KCORE_MANAGEMENT_PORT` | Build default management URL |
| `KCORE_CLUSTER_NAME` / `KCORE_NODE_ROLE` | Shown on the Overview page |
| `KCORE_MGMT_IFACE` | Mark management NIC when default route is ambiguous |
| `KCORE_API_PORT` | Override local API port (default 9091) for reachability checks |

`KCORE_GIT_REV` is set at **build** time in Nix (see `flake.nix`) to populate the
**Build** field when available.
