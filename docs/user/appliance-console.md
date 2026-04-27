# Appliance console

kcore nodes display a local appliance console on the host screen instead of a
standard Linux login prompt. The console is read-only and is designed for quick
status checks during install, boot, and onsite troubleshooting.

Use remote management for administration:

```bash
kcorectl login https://<management-ip>:8443
```

Local shell login is disabled by design.

## What you see

The console opens to the **Overview** page and shows:

- Product name: **kcore hypervisor**
- Hostname, kcore version, and build ID
- Management URL and local API endpoint
- Cluster name and node role when known
- Overall health, uptime, and current local time
- Local login status (`disabled`)
- Remote management hint using `kcorectl`

The console keeps refreshing in the background. If the local API is not ready,
the screen still opens and shows `API: unavailable`.

## Pages

| Page | Shows |
| --- | --- |
| Overview | Node identity, version, management URL, health, uptime, and remote-management command |
| Network | NIC table with interface, MAC, operational state, IPv4, IPv6, MTU, speed, driver, default-route marker, and management marker |
| Storage | Disk table with device, path, model, serial, human-readable size, SSD/HDD/NVMe type, read-only flag, mountpoints, health, and usage role |
| Diagnostics | Local kcore service status for node-agent, controller, and dashboard |
| Help | Keyboard shortcuts and security reminders |

Missing values are shown as `—`.

## Keyboard shortcuts

| Key | Action |
| --- | --- |
| `Tab`, `Right` | Next page |
| `Shift+Tab`, `Left` | Previous page |
| `1` to `5` | Jump to Overview, Network, Storage, Diagnostics, or Help |
| `Up`, `Down` | Move selection in the current table |
| `r` | Refresh inventory now |
| `h` or `?` | Open Help |
| `Esc` | Return to Overview |
| `q` | Disabled in production |
| `Ctrl+C` | Disabled in production |

## Security model

The appliance console is intentionally not a privileged shell.

- No local username/password prompt is exposed.
- Reboot and shutdown actions are not available from the local TUI.
- Real administration happens through the authenticated remote API and
  `kcorectl`.
- If the console process exits or crashes, `systemd` restarts it.

For production hosts, also harden the boot path:

- Protect UEFI/firmware setup with a password.
- Protect GRUB and disable unauthenticated kernel command-line editing.
- Use Secure Boot where supported.
- Disable unauthenticated recovery shells.
- Keep management interfaces on trusted networks.

## Troubleshooting

If the console still shows a Linux login prompt:

1. Confirm `kcore-console.service` is enabled and running.
2. Confirm `getty@tty1.service` and `autovt@.service` are masked or disabled.
3. Confirm `getty@tty2.service` through `getty@tty6.service` are also disabled,
   because users can switch virtual terminals with `Ctrl+Alt+F2` to `F6`.
4. Reboot and check the screen attached to `tty1`.

If network or disk tables are incomplete, the node may be missing Linux data
from `ip`, `/sys`, or `lsblk`; the console will keep rendering and refresh
again automatically.
