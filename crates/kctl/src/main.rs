mod client;
mod commands;
mod config;
mod output;
mod pki;

use std::path::PathBuf;
use std::process;

use clap::{Parser, Subcommand, ValueEnum};

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Parser)]
#[command(name = "kctl", version = VERSION, about = "kcore CLI")]
struct Cli {
    /// Path to config file
    #[arg(short = 'c', long, global = true)]
    config: Option<PathBuf>,

    /// Controller address (host:port)
    #[arg(short = 's', long, global = true)]
    controller: Option<String>,

    /// Skip TLS (connect over plain HTTP)
    #[arg(short = 'k', long, global = true)]
    insecure: bool,

    /// Node address for direct node commands (host:port)
    #[arg(long, global = true)]
    node: Option<String>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Create a resource
    Create {
        #[command(subcommand)]
        resource: CreateResource,
    },
    /// Delete a resource
    Delete {
        #[command(subcommand)]
        resource: DeleteResource,
    },
    /// Start a resource
    Start {
        #[command(subcommand)]
        resource: StartResource,
    },
    /// Stop a resource
    Stop {
        #[command(subcommand)]
        resource: StopResource,
    },
    /// Set desired resource state (declarative)
    Set {
        #[command(subcommand)]
        resource: SetResource,
    },
    /// Update a resource
    Update {
        #[command(subcommand)]
        resource: UpdateResource,
    },
    /// Get or list resources
    Get {
        #[command(subcommand)]
        resource: GetResource,
    },
    /// Node administration commands
    Node {
        #[command(subcommand)]
        action: NodeAction,
    },
    /// Pull a resource
    Pull {
        #[command(subcommand)]
        resource: PullResource,
    },
    /// Manage SSH keys
    #[command(alias = "ssh-key")]
    SshKey {
        #[command(subcommand)]
        action: SshKeyAction,
    },
    /// Drain a node (migrate all VMs to other nodes)
    Drain {
        #[command(subcommand)]
        resource: DrainResource,
    },
    /// Apply a NixOS configuration to the controller
    Apply {
        /// Path to the NixOS configuration file
        #[arg(short = 'f', long = "filename")]
        file: String,
        /// Print the config without applying
        #[arg(long)]
        dry_run: bool,
    },
    /// Show version
    Version,
}

#[derive(Subcommand)]
enum CreateResource {
    /// Create a virtual machine
    Vm {
        /// VM name (optional if using -f)
        name: Option<String>,
        /// Create from YAML manifest
        #[arg(short = 'f', long = "filename")]
        file: Option<String>,
        /// Number of CPU cores
        #[arg(long, default_value = "2")]
        cpu: i32,
        /// Memory size (e.g. 2G, 4096M)
        #[arg(long, default_value = "2G")]
        memory: String,
        /// VM boot image HTTPS URL
        #[arg(long)]
        image: Option<String>,
        /// Required SHA256 for --image URL
        #[arg(long = "image-sha256")]
        image_sha256: Option<String>,
        /// Node-local image path (alternative to --image URL mode)
        #[arg(long = "image-path")]
        image_path: Option<String>,
        /// Image format for --image-path mode (raw or qcow2)
        #[arg(long = "image-format")]
        image_format: Option<String>,
        /// Network name
        #[arg(long)]
        network: Option<String>,
        /// Target node (optional, controller picks if empty)
        #[arg(long = "target-node")]
        target_node: Option<String>,
        /// Wait until VM reaches running state
        #[arg(long)]
        wait: bool,
        /// Wait until VM is running and SSH port is reachable from node host
        #[arg(long = "wait-for-ssh")]
        wait_for_ssh: bool,
        /// Max wait time in seconds for --wait/--wait-for-ssh
        #[arg(long = "wait-timeout-seconds", default_value_t = 300)]
        wait_timeout_seconds: u64,
        /// SSH port to probe when using --wait-for-ssh
        #[arg(long = "ssh-port", default_value_t = 22)]
        ssh_port: i32,
        /// SSH TCP probe timeout (milliseconds) when using --wait-for-ssh
        #[arg(long = "ssh-probe-timeout-ms", default_value_t = 1500)]
        ssh_probe_timeout_ms: i32,
        /// SSH key names to inject (can specify multiple times)
        #[arg(long = "ssh-key")]
        ssh_keys: Vec<String>,
        /// Required VM storage backend (filesystem, lvm, zfs)
        #[arg(long = "storage-backend", value_enum)]
        storage_backend: StorageBackend,
        /// Required VM storage size in bytes (for backend provisioning metadata)
        #[arg(long = "storage-size-bytes")]
        storage_size_bytes: i64,
    },
    /// Create a network on a node (declarative)
    Network {
        /// Network name
        name: String,
        /// External IP used as NAT source for this network
        #[arg(long = "external-ip")]
        external_ip: String,
        /// Gateway IP assigned to the bridge
        #[arg(long = "gateway-ip")]
        gateway_ip: String,
        /// Internal netmask (default: 255.255.255.0)
        #[arg(long = "internal-netmask", default_value = "255.255.255.0")]
        internal_netmask: String,
        /// Target node (optional, controller picks if empty)
        #[arg(long = "target-node")]
        target_node: Option<String>,
    },
    /// Create cluster PKI and local context for mTLS
    Cluster {
        /// Controller address (host:port)
        #[arg(long)]
        controller: String,
        /// Optional cert output directory
        #[arg(long)]
        certs_dir: Option<PathBuf>,
        /// Context name to write in config
        #[arg(long, default_value = "default")]
        context: String,
        /// Overwrite existing certificate files
        #[arg(long)]
        force: bool,
    },
}

#[derive(Subcommand)]
enum DeleteResource {
    /// Delete a virtual machine
    Vm {
        /// VM ID or name
        vm_id: String,
        /// Target node (optional)
        #[arg(long = "target-node")]
        target_node: Option<String>,
    },
    /// Delete a network
    Network {
        /// Network name
        name: String,
        /// Target node (optional; required if network exists on multiple nodes)
        #[arg(long = "target-node")]
        target_node: Option<String>,
    },
    /// Delete an image from a node
    Image {
        /// Image name
        name: String,
        /// Force deletion
        #[arg(long)]
        force: bool,
    },
}

#[derive(Subcommand)]
enum StartResource {
    /// Request desired running state for a virtual machine (declarative apply)
    Vm {
        /// VM ID or name
        vm_id: String,
        /// Target node (optional)
        #[arg(long = "target-node")]
        target_node: Option<String>,
    },
}

#[derive(Subcommand)]
enum StopResource {
    /// Request desired stopped state for a virtual machine (declarative apply)
    Vm {
        /// VM ID or name
        vm_id: String,
        /// Target node (optional)
        #[arg(long = "target-node")]
        target_node: Option<String>,
    },
}

#[derive(Clone, ValueEnum)]
enum DesiredVmState {
    Running,
    Stopped,
}

#[derive(Subcommand)]
enum SetResource {
    /// Set desired state for a virtual machine (declarative apply)
    Vm {
        /// VM ID or name
        vm_id: String,
        /// Desired VM state
        #[arg(long, value_enum)]
        state: DesiredVmState,
        /// Target node (optional)
        #[arg(long = "target-node")]
        target_node: Option<String>,
    },
}

#[derive(Subcommand)]
enum UpdateResource {
    /// Update a virtual machine (resize CPU/memory)
    Vm {
        /// VM ID or name
        vm_id: String,
        /// New CPU count
        #[arg(long)]
        cpu: Option<i32>,
        /// New memory size (e.g. 4G, 8192M)
        #[arg(long)]
        memory: Option<String>,
        /// Target node (optional)
        #[arg(long = "target-node")]
        target_node: Option<String>,
    },
}

#[derive(Subcommand)]
enum GetResource {
    /// Get or list virtual machines
    #[command(alias = "vm")]
    Vms {
        /// VM ID or name (omit to list all)
        name: Option<String>,
        /// Filter by node
        #[arg(long = "target-node")]
        target_node: Option<String>,
    },
    /// Get or list nodes
    #[command(alias = "node")]
    Nodes {
        /// Node ID (omit to list all)
        name: Option<String>,
    },
    /// List custom networks
    #[command(alias = "network")]
    Networks {
        /// Filter by node
        #[arg(long = "target-node")]
        target_node: Option<String>,
    },
}

#[derive(Subcommand)]
enum NodeAction {
    /// List block devices on a node
    Disks,
    /// List network interfaces on a node
    Nics,
    /// Install kcore to disk on a node
    Install {
        /// OS disk (e.g. /dev/sda)
        #[arg(long)]
        os_disk: String,
        /// Data disks (e.g. /dev/nvme0n1)
        #[arg(long)]
        data_disk: Vec<String>,
        /// Controller to join after install
        #[arg(long)]
        join_controller: Option<String>,
        /// Install and run controller on this node
        #[arg(long)]
        run_controller: bool,
        /// Storage mode for data disks: filesystem, lvm, or zfs
        #[arg(long, default_value = "filesystem")]
        data_disk_mode: String,
        /// Typed storage backend for install flow (preferred over --data-disk-mode)
        #[arg(long = "storage-backend", value_enum)]
        storage_backend: Option<StorageBackend>,
        /// Optional LVM VG name (used when backend is lvm)
        #[arg(long = "lvm-vg-name")]
        lvm_vg_name: Option<String>,
        /// Optional LVM LV prefix (used when backend is lvm)
        #[arg(long = "lvm-lv-prefix")]
        lvm_lv_prefix: Option<String>,
        /// Optional ZFS pool name (used when backend is zfs)
        #[arg(long = "zfs-pool-name")]
        zfs_pool_name: Option<String>,
        /// Optional ZFS dataset prefix (used when backend is zfs)
        #[arg(long = "zfs-dataset-prefix")]
        zfs_dataset_prefix: Option<String>,
    },
    /// Apply a NixOS configuration to a node
    ApplyNix {
        /// Path to the NixOS configuration file
        #[arg(short = 'f', long = "filename")]
        file: String,
        /// Write config without running nixos-rebuild
        #[arg(long)]
        no_rebuild: bool,
    },
    /// Upload a local image file to a node image cache
    UploadImage {
        /// Local image filename (qcow2 or raw)
        #[arg(short = 'f', long = "filename")]
        file: String,
        /// Optional destination filename on node
        #[arg(long)]
        name: Option<String>,
        /// Image format (defaults to extension-based inference)
        #[arg(long, value_enum)]
        format: Option<NodeImageFormat>,
        /// Optional expected SHA256 checksum
        #[arg(long = "image-sha256")]
        image_sha256: Option<String>,
    },
}

#[derive(Subcommand)]
enum PullResource {
    /// Pull an image to a node
    Image {
        /// Image URI (HTTPS)
        uri: String,
        /// Required SHA256 checksum for integrity verification
        #[arg(long)]
        sha256: Option<String>,
    },
}

#[derive(Subcommand)]
enum SshKeyAction {
    /// Create an SSH key
    Create {
        /// Key name
        name: String,
        /// Public key content (e.g., "ssh-rsa AAAA... user@host")
        #[arg(long = "public-key")]
        public_key: String,
    },
    /// Delete an SSH key
    Delete {
        /// Key name
        name: String,
    },
    /// List all SSH keys
    List,
    /// Get SSH key details
    Get {
        /// Key name
        name: String,
    },
}

#[derive(Subcommand)]
enum DrainResource {
    /// Drain a node
    Node {
        /// Node ID to drain
        node_id: String,
        /// Target node to move VMs to (optional; auto-schedules if empty)
        #[arg(long = "target-node")]
        target_node: Option<String>,
    },
}

#[derive(Clone, ValueEnum)]
enum NodeImageFormat {
    Raw,
    Qcow2,
}

#[derive(Clone, ValueEnum)]
enum StorageBackend {
    Filesystem,
    Lvm,
    Zfs,
}

fn resolve_controller(cli: &Cli) -> Result<config::ConnectionInfo, String> {
    let config_path = cli
        .config
        .clone()
        .unwrap_or_else(config::default_config_path);
    config::resolve_controller(&config_path, &cli.controller, cli.insecure)
}

fn resolve_node(cli: &Cli) -> Result<config::ConnectionInfo, String> {
    let config_path = cli
        .config
        .clone()
        .unwrap_or_else(config::default_config_path);
    config::resolve_node(&config_path, &cli.node, cli.insecure)
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let result = match &cli.command {
        Command::Create {
            resource:
                CreateResource::Vm {
                    name,
                    file,
                    cpu,
                    memory,
                    image,
                    image_sha256,
                    image_path,
                    image_format,
                    network,
                    target_node,
                    wait,
                    wait_for_ssh,
                    wait_timeout_seconds,
                    ssh_port,
                    ssh_probe_timeout_ms,
                    ssh_keys,
                    storage_backend,
                    storage_size_bytes,
                },
        } => {
            let info = resolve_controller(&cli).unwrap_or_else(|e| fatal(&e));
            commands::vm::create(
                &info,
                commands::vm::CreateArgs {
                    name: name.clone(),
                    filename: file.clone(),
                    cpu: *cpu,
                    memory: memory.clone(),
                    image: image.clone(),
                    image_sha256: image_sha256.clone(),
                    image_path: image_path.clone(),
                    image_format: image_format.clone(),
                    network: network.clone(),
                    target_node: target_node.clone(),
                    wait: *wait,
                    wait_for_ssh: *wait_for_ssh,
                    wait_timeout_seconds: *wait_timeout_seconds,
                    ssh_port: *ssh_port,
                    ssh_probe_timeout_ms: *ssh_probe_timeout_ms,
                    ssh_keys: ssh_keys.clone(),
                    storage_backend: match storage_backend {
                        StorageBackend::Filesystem => "filesystem".to_string(),
                        StorageBackend::Lvm => "lvm".to_string(),
                        StorageBackend::Zfs => "zfs".to_string(),
                    },
                    storage_size_bytes: *storage_size_bytes,
                },
            )
            .await
        }
        Command::Create {
            resource:
                CreateResource::Network {
                    name,
                    external_ip,
                    gateway_ip,
                    internal_netmask,
                    target_node,
                },
        } => {
            let info = resolve_controller(&cli).unwrap_or_else(|e| fatal(&e));
            commands::network::create(
                &info,
                commands::network::CreateArgs {
                    name: name.clone(),
                    external_ip: external_ip.clone(),
                    gateway_ip: gateway_ip.clone(),
                    internal_netmask: internal_netmask.clone(),
                    target_node: target_node.clone(),
                },
            )
            .await
        }
        Command::Create {
            resource:
                CreateResource::Cluster {
                    controller,
                    certs_dir,
                    context,
                    force,
                },
        } => {
            let config_path = cli
                .config
                .clone()
                .unwrap_or_else(config::default_config_path);
            let certs_path = certs_dir
                .clone()
                .unwrap_or_else(|| config::default_cluster_certs_dir(context));
            commands::cluster::create(&config_path, controller, &certs_path, context, *force)
        }

        Command::Delete {
            resource: DeleteResource::Vm { vm_id, target_node },
        } => {
            let info = resolve_controller(&cli).unwrap_or_else(|e| fatal(&e));
            commands::vm::delete(&info, vm_id, target_node.clone()).await
        }
        Command::Delete {
            resource: DeleteResource::Network { name, target_node },
        } => {
            let info = resolve_controller(&cli).unwrap_or_else(|e| fatal(&e));
            commands::network::delete(&info, name, target_node.clone()).await
        }

        Command::Delete {
            resource: DeleteResource::Image { name, force },
        } => {
            let info = resolve_node(&cli).unwrap_or_else(|e| fatal(&e));
            commands::image::delete(&info, name, *force).await
        }

        Command::Start {
            resource: StartResource::Vm { vm_id, target_node },
        } => {
            let info = resolve_controller(&cli).unwrap_or_else(|e| fatal(&e));
            commands::vm::start(&info, vm_id, target_node.clone()).await
        }

        Command::Stop {
            resource: StopResource::Vm { vm_id, target_node },
        } => {
            let info = resolve_controller(&cli).unwrap_or_else(|e| fatal(&e));
            commands::vm::stop(&info, vm_id, target_node.clone()).await
        }

        Command::Set {
            resource:
                SetResource::Vm {
                    vm_id,
                    state,
                    target_node,
                },
        } => {
            let info = resolve_controller(&cli).unwrap_or_else(|e| fatal(&e));
            let (desired, label) = match state {
                DesiredVmState::Running => {
                    (client::controller_proto::VmDesiredState::Running, "running")
                }
                DesiredVmState::Stopped => {
                    (client::controller_proto::VmDesiredState::Stopped, "stopped")
                }
            };
            commands::vm::set_desired_state(&info, vm_id, desired, target_node.clone(), label).await
        }

        Command::Update {
            resource:
                UpdateResource::Vm {
                    vm_id,
                    cpu,
                    memory,
                    target_node,
                },
        } => {
            let info = resolve_controller(&cli).unwrap_or_else(|e| fatal(&e));
            commands::vm::update(&info, vm_id, *cpu, memory.clone(), target_node.clone()).await
        }

        Command::Get {
            resource: GetResource::Vms { name, target_node },
        } => {
            let info = resolve_controller(&cli).unwrap_or_else(|e| fatal(&e));
            if let Some(n) = name {
                commands::vm::get(&info, n, target_node.clone()).await
            } else {
                commands::vm::list(&info, target_node.clone()).await
            }
        }

        Command::Get {
            resource: GetResource::Nodes { name },
        } => {
            let info = resolve_controller(&cli).unwrap_or_else(|e| fatal(&e));
            if let Some(n) = name {
                commands::node::get_node(&info, n).await
            } else {
                commands::node::list_nodes(&info).await
            }
        }
        Command::Get {
            resource: GetResource::Networks { target_node },
        } => {
            let info = resolve_controller(&cli).unwrap_or_else(|e| fatal(&e));
            commands::network::list(&info, target_node.clone()).await
        }

        Command::Node {
            action: NodeAction::Disks,
        } => {
            let info = resolve_node(&cli).unwrap_or_else(|e| fatal(&e));
            commands::node::disks(&info).await
        }

        Command::Node {
            action: NodeAction::Nics,
        } => {
            let info = resolve_node(&cli).unwrap_or_else(|e| fatal(&e));
            commands::node::nics(&info).await
        }

        Command::Node {
            action:
                NodeAction::Install {
                    os_disk,
                    data_disk,
                    join_controller,
                    run_controller,
                    data_disk_mode,
                    storage_backend,
                    lvm_vg_name,
                    lvm_lv_prefix,
                    zfs_pool_name,
                    zfs_dataset_prefix,
                },
        } => {
            let info = resolve_node(&cli).unwrap_or_else(|e| fatal(&e));
            let config_path = cli
                .config
                .clone()
                .unwrap_or_else(config::default_config_path);
            let certs_dir = config::resolve_install_certs_dir(&config_path).unwrap_or_else(|e| {
                fatal(&format!("resolving install cert directory: {e}"));
            });
            commands::node::install(
                &info,
                os_disk,
                data_disk.clone(),
                join_controller.as_deref(),
                *run_controller,
                data_disk_mode,
                storage_backend.as_ref().map(|v| match v {
                    StorageBackend::Filesystem => "filesystem",
                    StorageBackend::Lvm => "lvm",
                    StorageBackend::Zfs => "zfs",
                }),
                lvm_vg_name.as_deref(),
                lvm_lv_prefix.as_deref(),
                zfs_pool_name.as_deref(),
                zfs_dataset_prefix.as_deref(),
                &certs_dir,
            )
            .await
        }

        Command::Node {
            action: NodeAction::ApplyNix { file, no_rebuild },
        } => {
            let info = resolve_node(&cli).unwrap_or_else(|e| fatal(&e));
            commands::node::apply_nix(&info, file, !no_rebuild).await
        }
        Command::Node {
            action:
                NodeAction::UploadImage {
                    file,
                    name,
                    format,
                    image_sha256,
                },
        } => {
            let info = resolve_node(&cli).unwrap_or_else(|e| fatal(&e));
            let fmt = format.as_ref().map(|f| match f {
                NodeImageFormat::Raw => "raw",
                NodeImageFormat::Qcow2 => "qcow2",
            });
            commands::node::upload_image(&info, file, name.as_deref(), fmt, image_sha256.as_deref())
                .await
        }

        Command::Pull {
            resource: PullResource::Image { uri, sha256 },
        } => {
            let info = resolve_node(&cli).unwrap_or_else(|e| fatal(&e));
            commands::image::pull(&info, uri, sha256.as_deref()).await
        }

        Command::SshKey {
            action: SshKeyAction::Create { name, public_key },
        } => {
            let info = resolve_controller(&cli).unwrap_or_else(|e| fatal(&e));
            commands::ssh_key::create(&info, name, public_key).await
        }
        Command::SshKey {
            action: SshKeyAction::Delete { name },
        } => {
            let info = resolve_controller(&cli).unwrap_or_else(|e| fatal(&e));
            commands::ssh_key::delete(&info, name).await
        }
        Command::SshKey {
            action: SshKeyAction::List,
        } => {
            let info = resolve_controller(&cli).unwrap_or_else(|e| fatal(&e));
            commands::ssh_key::list(&info).await
        }
        Command::SshKey {
            action: SshKeyAction::Get { name },
        } => {
            let info = resolve_controller(&cli).unwrap_or_else(|e| fatal(&e));
            commands::ssh_key::get(&info, name).await
        }

        Command::Drain {
            resource:
                DrainResource::Node {
                    node_id,
                    target_node,
                },
        } => {
            let info = resolve_controller(&cli).unwrap_or_else(|e| fatal(&e));
            let mut client = client::controller_client(&info)
                .await
                .unwrap_or_else(|e| fatal(&format!("{e}")));
            let resp = client
                .drain_node(client::controller_proto::DrainNodeRequest {
                    node_id: node_id.to_string(),
                    target_node: target_node.clone().unwrap_or_default(),
                })
                .await;
            match resp {
                Ok(r) => {
                    let r = r.into_inner();
                    println!("{}", r.message);
                    if r.success {
                        Ok(())
                    } else {
                        Err(anyhow::anyhow!("drain had errors"))
                    }
                }
                Err(e) => Err(anyhow::anyhow!("drain failed: {e}")),
            }
        }

        Command::Apply { file, dry_run } => {
            let info = resolve_controller(&cli).unwrap_or_else(|e| fatal(&e));
            commands::apply::apply(&info, file, *dry_run).await
        }

        Command::Version => {
            println!("kctl {VERSION}");
            Ok(())
        }
    };

    if let Err(e) = result {
        eprintln!("Error: {e}");
        process::exit(1);
    }
}

fn fatal(msg: &str) -> ! {
    eprintln!("Error: {msg}");
    process::exit(1);
}
