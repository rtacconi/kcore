mod client;
mod commands;
mod config;
mod output;
mod pki;

use std::path::PathBuf;
use std::process;

use clap::{Parser, Subcommand};

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
        /// OS image (URI or local path)
        #[arg(long)]
        image: Option<String>,
        /// Network name
        #[arg(long)]
        network: Option<String>,
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
        join_controller: String,
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
                    network: network.clone(),
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
                .unwrap_or_else(config::default_certs_dir);
            commands::cluster::create(&config_path, controller, &certs_path, context, *force)
        }

        Command::Delete {
            resource: DeleteResource::Vm { vm_id, target_node },
        } => {
            let info = resolve_controller(&cli).unwrap_or_else(|e| fatal(&e));
            commands::vm::delete(&info, vm_id, target_node.clone()).await
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
            resource:
                StopResource::Vm {
                    vm_id,
                    target_node,
                },
        } => {
            let info = resolve_controller(&cli).unwrap_or_else(|e| fatal(&e));
            commands::vm::stop(&info, vm_id, target_node.clone()).await
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
                },
        } => {
            let info = resolve_node(&cli).unwrap_or_else(|e| fatal(&e));
            let certs_dir = info
                .ca
                .as_ref()
                .and_then(|p| PathBuf::from(p).parent().map(|v| v.to_path_buf()))
                .unwrap_or_else(config::default_certs_dir);
            commands::node::install(
                &info,
                os_disk,
                data_disk.clone(),
                join_controller,
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
