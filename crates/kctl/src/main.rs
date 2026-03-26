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
        /// Network name
        #[arg(long)]
        network: Option<String>,
        /// Target node (optional, controller picks if empty)
        #[arg(long = "target-node")]
        target_node: Option<String>,
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
}

#[derive(Subcommand)]
enum PullResource {
    /// Pull an image to a node
    Image {
        /// Image URI (HTTP/HTTPS)
        uri: String,
    },
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
                    network,
                    target_node,
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
                    network: network.clone(),
                    target_node: target_node.clone(),
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

        Command::Pull {
            resource: PullResource::Image { uri },
        } => {
            let info = resolve_node(&cli).unwrap_or_else(|e| fatal(&e));
            commands::image::pull(&info, uri).await
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
