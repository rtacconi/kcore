use crate::api::{
    get_compliance_dto, get_network_overview_dto, get_replication_status_dto,
    get_storage_overview_dto, list_networks_dto, list_replication_conflicts_dto, list_vms_page,
};
use crate::dto::{
    ComplianceDto, HostInterfaceDto, LvmLogicalVolumeDto, LvmPhysicalVolumeDto, LvmVolumeGroupDto,
    NetworkOverviewDto, NetworkRowDto, NodeNetworkDto, NodeStorageDto, NodeSummaryDto,
    ReplicationConflictDto, ReplicationStatusDto, StorageDiskRowDto, StorageOverviewDto, VmRowDto,
    VmsPageDto,
};
use leptos::prelude::*;
use leptos_meta::{provide_meta_context, Link, Meta, MetaTags, Stylesheet, Title};
use leptos_router::{
    components::{FlatRoutes, Route, Router},
    hooks::use_query_map,
    path,
};

pub fn shell(_options: LeptosOptions) -> impl IntoView {
    view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8"/>
                <meta name="viewport" content="width=device-width, initial-scale=1"/>
                <Link rel="preconnect" href="https://fonts.googleapis.com"/>
                <Link rel="preconnect" href="https://fonts.gstatic.com" crossorigin="anonymous"/>
                <Link
                    href="https://fonts.googleapis.com/css2?family=DM+Sans:ital,opsz,wght@0,9..40,400;0,9..40,600;0,9..40,800;1,9..40,400&family=JetBrains+Mono:wght@400;600&display=swap"
                    rel="stylesheet"
                />
                <Stylesheet id="kcore-dashboard" href="/dashboard.css"/>
                <Meta name="description" content="kcore — declarative virtualization dashboard"/>
                <MetaTags/>
            </head>
            <body>
                <App/>
            </body>
        </html>
    }
}

#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();
    view! {
        <Title text="kcore dashboard"/>
        <div class="layout">
            <header class="top-nav">
                <div class="brand">
                    <span class="brand-mark">"kcore"</span>
                    <span class="brand-sub">"Dashboard"</span>
                </div>
                <nav class="nav-links">
                    <a href="/">"Overview"</a>
                    <a href="/compliance">"Compliance"</a>
                    <a href="/vms">"Virtual machines"</a>
                    <a href="/networks">"Networks"</a>
                    <a href="/storage">"Storage"</a>
                </nav>
            </header>
            <main class="page">
                <Router>
                    <FlatRoutes fallback=|| view! { <p class="muted">"Not found."</p> }>
                        <Route path=path!("/") view=HomePage/>
                        <Route path=path!("/compliance") view=CompliancePage/>
                        <Route path=path!("/vms") view=VmsPage/>
                        <Route path=path!("/networks") view=NetworksPage/>
                        <Route path=path!("/storage") view=StoragePage/>
                    </FlatRoutes>
                </Router>
            </main>
            <footer class="page footer-note">
                "Data is read from the kcore controller over gRPC (same API as kctl). "
                <a href="https://kcorehypervisor.com/">"kcorehypervisor.com"</a>
            </footer>
        </div>
    }
}

#[component]
fn HomePage() -> impl IntoView {
    view! {
        <section class="hero">
            <h1>"Declarative virtualization"</h1>
            <p>
                "Monitor compliance posture, workloads, and networks from a single dashboard. "
                "No direct database access — everything goes through the controller API."
            </p>
        </section>
        <div class="card-grid cols-2">
            <a href="/compliance" class="card" style="text-decoration: none; color: inherit;">
                <h2>"Compliance"</h2>
                <p class="muted">"Cryptography, access control, and inventory aligned with kctl report."</p>
            </a>
            <a href="/vms" class="card" style="text-decoration: none; color: inherit;">
                <h2>"Virtual machines"</h2>
                <p class="muted">"Cluster-wide VM list with paging when you have more than ten."</p>
            </a>
            <a href="/networks" class="card" style="text-decoration: none; color: inherit;">
                <h2>"Networks"</h2>
                <p class="muted">"NAT, bridge, and overlay networks registered with the controller."</p>
            </a>
            <a href="/storage" class="card" style="text-decoration: none; color: inherit;">
                <h2>"Storage"</h2>
                <p class="muted">"Cluster-wide data-plane backends (filesystem, LVM, ZFS) and block inventory per node."</p>
            </a>
        </div>
    }
}

#[component]
fn CompliancePage() -> impl IntoView {
    let res = Resource::new(|| (), |_| get_compliance_dto());
    let replication_res = Resource::new(|| (), |_| get_replication_status_dto());
    let conflicts_res = Resource::new(|| (), |_| list_replication_conflicts_dto());
    view! {
        <section class="hero">
            <h1>"Compliance report"</h1>
            <p class="muted">"Same evidence categories as "<code class="inline">"kctl report"</code>"."</p>
        </section>
        <Suspense fallback=move || view! { <p class="muted">"Loading compliance…"</p> }>
            {move || Suspend::new(async move {
                match res.await {
                    Ok(data) => compliance_view(data).into_any(),
                    Err(e) => view! { <p class="err">{e.to_string()}</p> }.into_any(),
                }
            })}
        </Suspense>
        <section class="card" style="margin-top: 1rem;">
            <h2>"Replication resilience"</h2>
            <Suspense fallback=move || view! { <p class="muted">"Loading replication status…"</p> }>
                {move || Suspend::new(async move {
                    match replication_res.await {
                        Ok(data) => replication_status_view(data).into_any(),
                        Err(e) => view! { <p class="err">{e.to_string()}</p> }.into_any(),
                    }
                })}
            </Suspense>
        </section>
        <section class="card" style="margin-top: 1rem;">
            <h2>"Replication conflicts"</h2>
            <Suspense fallback=move || view! { <p class="muted">"Loading conflicts…"</p> }>
                {move || Suspend::new(async move {
                    match conflicts_res.await {
                        Ok(data) => conflicts_view(data).into_any(),
                        Err(e) => view! { <p class="err">{e.to_string()}</p> }.into_any(),
                    }
                })}
            </Suspense>
        </section>
    }
}

fn replication_status_view(data: ReplicationStatusDto) -> impl IntoView {
    let health = if data.zero_manual_slo_healthy {
        "healthy"
    } else {
        "violations"
    };
    let has_outgoing = !data.outgoing.is_empty();
    let has_incoming = !data.incoming.is_empty();
    let outgoing = data.outgoing.clone();
    let incoming = data.incoming.clone();
    view! {
        <div>
            <div class="stat-row" style="margin-bottom: 0.75rem;">
                <div class="stat"><div class="label">"SLO"</div><div class="value">{health}</div></div>
                <div class="stat"><div class="label">"Outbox head"</div><div class="value">{data.outbox_head_event_id}</div></div>
                <div class="stat"><div class="label">"Outbox size"</div><div class="value">{data.outbox_size}</div></div>
                <div class="stat"><div class="label">"Conflicts"</div><div class="value">{data.unresolved_conflicts}</div></div>
                <div class="stat"><div class="label">"Comp. pending"</div><div class="value">{data.pending_compensation_jobs}</div></div>
                <div class="stat"><div class="label">"Materialization"</div><div class="value">{data.materialization_backlog}</div></div>
            </div>
            <dl class="kv">
                <dt>"Reservation failures"</dt><dd>{data.failed_reservations}</dd>
                <dt>"Retryable"</dt><dd>{data.failed_retryable_reservations}</dd>
                <dt>"Non-retryable"</dt><dd>{data.failed_non_retryable_reservations}</dd>
                <dt>"Retry exhausted"</dt><dd>{data.retry_exhausted_reservations}</dd>
            </dl>
            <Show when=move || !data.zero_manual_slo_healthy>
                <p class="muted" style="margin-top: 0.75rem;">
                    {format!("Violations: {}", data.zero_manual_slo_violations.join(", "))}
                </p>
            </Show>
            {
                let outgoing_view = outgoing.clone();
                view! {
                    <Show when=move || has_outgoing>
                        <h3 style="margin-top: 1rem; font-size: 0.85rem; text-transform: uppercase; color: var(--muted); letter-spacing: 0.06em;">"Outgoing peers"</h3>
                        <div class="table-wrap" style="margin-top: 0.5rem;">
                            <table class="data">
                                <thead>
                                    <tr>
                                        <th>"Peer"</th>
                                        <th>"Last acked"</th>
                                        <th>"Lag"</th>
                                    </tr>
                                </thead>
                                <tbody>
                                    {outgoing_view.clone().into_iter().map(|o| view! {
                                        <tr>
                                            <td><code class="inline">{o.peer_id.clone()}</code></td>
                                            <td>{o.last_acked_event_id}</td>
                                            <td>{o.lag_events}</td>
                                        </tr>
                                    }).collect_view()}
                                </tbody>
                            </table>
                        </div>
                    </Show>
                }
            }
            {
                let incoming_view = incoming.clone();
                view! {
                    <Show when=move || has_incoming>
                        <h3 style="margin-top: 1rem; font-size: 0.85rem; text-transform: uppercase; color: var(--muted); letter-spacing: 0.06em;">"Incoming peers"</h3>
                        <div class="table-wrap" style="margin-top: 0.5rem;">
                            <table class="data">
                                <thead>
                                    <tr>
                                        <th>"Endpoint"</th>
                                        <th>"Last pulled"</th>
                                        <th>"Last applied"</th>
                                    </tr>
                                </thead>
                                <tbody>
                                    {incoming_view.clone().into_iter().map(|i| view! {
                                        <tr>
                                            <td><code class="inline">{i.peer_endpoint.clone()}</code></td>
                                            <td>{i.last_pulled_event_id}</td>
                                            <td>{i.last_applied_event_id}</td>
                                        </tr>
                                    }).collect_view()}
                                </tbody>
                            </table>
                        </div>
                    </Show>
                }
            }
        </div>
    }
}

fn conflicts_view(conflicts: Vec<ReplicationConflictDto>) -> impl IntoView {
    if conflicts.is_empty() {
        return view! {
            <p class="muted">"No unresolved replication conflicts."</p>
        }
        .into_any();
    }
    view! {
        <div class="table-wrap">
            <table class="data">
                <thead>
                    <tr>
                        <th>"ID"</th>
                        <th>"Resource"</th>
                        <th>"Incumbent"</th>
                        <th>"Challenger"</th>
                        <th>"Reason"</th>
                    </tr>
                </thead>
                <tbody>
                    {conflicts.into_iter().map(|c| {
                        let incumbent = format!("{} ({})", c.incumbent_op_id, c.incumbent_controller_id);
                        let challenger = format!("{} ({})", c.challenger_op_id, c.challenger_controller_id);
                        view! {
                            <tr>
                                <td>{c.id}</td>
                                <td><code class="inline">{c.resource_key.clone()}</code></td>
                                <td>{incumbent}</td>
                                <td>{challenger}</td>
                                <td>{c.reason.clone()}</td>
                            </tr>
                        }
                    }).collect_view()}
                </tbody>
            </table>
        </div>
    }
    .into_any()
}

fn compliance_view(data: ComplianceDto) -> impl IntoView {
    let nodes = data.nodes.clone();
    let has_nodes = !nodes.is_empty();
    let mtls = if data.mtls_enabled {
        "enabled"
    } else {
        "disabled"
    };
    let subca = if data.sub_ca_enabled {
        "enabled"
    } else {
        "disabled"
    };
    view! {
        <div class="card-grid cols-2">
            <section class="card">
                <h2>"Cryptography"</h2>
                <dl class="kv">
                    <dt>"Library"</dt><dd>{data.crypto_library}</dd>
                    <dt>"TLS 1.3"</dt><dd>{data.tls13_cipher_suites.join(", ")}</dd>
                    <dt>"TLS 1.2"</dt><dd>{data.tls12_cipher_suites.join(", ")}</dd>
                    <dt>"Key exchange"</dt><dd>{data.kx_groups.join(", ")}</dd>
                    <dt>"Excluded"</dt><dd>{data.excluded_algorithms.join(", ")}</dd>
                </dl>
            </section>
            <section class="card">
                <h2>"Encryption in transit"</h2>
                <dl class="kv">
                    <dt>"mTLS"</dt><dd>{mtls}</dd>
                    <dt>"Protocol"</dt><dd>"gRPC over mTLS (X.509 client certificates)"</dd>
                </dl>
            </section>
            <section class="card" style="grid-column: 1 / -1;">
                <h2>"Access control"</h2>
                <table class="acl-table">
                    <tbody>
                        {data.access_control.into_iter().map(|e| view! {
                            <tr>
                                <td>{e.rpc_method}</td>
                                <td>{e.allowed_identities}</td>
                            </tr>
                        }).collect_view()}
                    </tbody>
                </table>
            </section>
            <section class="card">
                <h2>"Node inventory"</h2>
                <div class="stat-row">
                    <div class="stat"><div class="label">"Total"</div><div class="value">{data.total_nodes}</div></div>
                    <div class="stat"><div class="label">"Approved"</div><div class="value">{data.approved_nodes}</div></div>
                    <div class="stat"><div class="label">"Pending"</div><div class="value">{data.pending_nodes}</div></div>
                    <div class="stat"><div class="label">"Rejected"</div><div class="value">{data.rejected_nodes}</div></div>
                </div>
            </section>
            <section class="card">
                <h2>"VM inventory"</h2>
                <div class="stat-row">
                    <div class="stat"><div class="label">"Total"</div><div class="value">{data.total_vms}</div></div>
                    <div class="stat"><div class="label">"Running"</div><div class="value">{data.running_vms}</div></div>
                    <div class="stat"><div class="label">"Stopped"</div><div class="value">{data.stopped_vms}</div></div>
                </div>
            </section>
            <section class="card">
                <h2>"Network segmentation"</h2>
                <div class="stat-row">
                    <div class="stat"><div class="label">"Total"</div><div class="value">{data.total_networks}</div></div>
                    <div class="stat"><div class="label">"NAT"</div><div class="value">{data.nat_networks}</div></div>
                    <div class="stat"><div class="label">"Bridge"</div><div class="value">{data.bridge_networks}</div></div>
                    <div class="stat"><div class="label">"VXLAN"</div><div class="value">{data.vxlan_networks}</div></div>
                </div>
            </section>
            <section class="card">
                <h2>"Certificate lifecycle"</h2>
                <dl class="kv">
                    <dt>"Sub-CA"</dt><dd>{subca}</dd>
                    <dt>"Auto-renewal"</dt><dd>{format!("within {} days of expiry", data.cert_auto_renewal_days)}</dd>
                    <dt>"Expiring (<30d)"</dt><dd>{format!("{} nodes", data.nodes_expiring_30d)}</dd>
                    <dt>"Unknown"</dt><dd>{format!("{} nodes", data.nodes_cert_unknown)}</dd>
                </dl>
            </section>
            <section class="card">
                <h2>"Infrastructure"</h2>
                <dl class="kv">
                    <dt>"Controller"</dt><dd>{data.controller_version.clone()}</dd>
                    <dt>"OS"</dt><dd>"NixOS (declarative, atomic updates)"</dd>
                    <dt>"Hypervisor"</dt><dd>"Cloud Hypervisor (KVM)"</dd>
                    <dt>"VM isolation"</dt><dd>"Hardware KVM + per-VM TAP + per-network bridge"</dd>
                    <dt>"Node approval"</dt><dd>"Required (approval queue)"</dd>
                </dl>
            </section>
        </div>
        <Show when=move || has_nodes>
            <NodeDetailsTable nodes=nodes.clone()/>
        </Show>
    }
}

#[component]
fn NodeDetailsTable(nodes: Vec<NodeSummaryDto>) -> impl IntoView {
    view! {
        <section class="card" style="margin-top: 1rem;">
            <h2>"Node details"</h2>
            <div class="table-wrap">
                <table class="data">
                    <thead>
                        <tr>
                            <th>"ID"</th>
                            <th>"Hostname"</th>
                            <th>"Address"</th>
                            <th>"DC"</th>
                            <th>"Status"</th>
                            <th>"Cert (days)"</th>
                        </tr>
                    </thead>
                    <tbody>
                        {nodes.into_iter().map(|n| view! {
                            <tr>
                                <td><code class="inline">{n.node_id}</code></td>
                                <td>{n.hostname}</td>
                                <td>{n.address}</td>
                                <td>{if n.dc_id.is_empty() { "-".to_string() } else { n.dc_id }}</td>
                                <td>{n.approval_status}</td>
                                <td>{n.cert_expiry_days}</td>
                            </tr>
                        }).collect_view()}
                    </tbody>
                </table>
            </div>
        </section>
    }
}

#[component]
fn VmsPage() -> impl IntoView {
    let query = use_query_map();
    let page = Memo::new(move |_| {
        query
            .get()
            .get("page")
            .and_then(|s| s.parse::<u32>().ok())
            .filter(|&p| p >= 1)
            .unwrap_or(1)
    });
    let res = Resource::new(
        move || page.get(),
        |p| async move { list_vms_page(p).await },
    );
    view! {
        <section class="hero">
            <h1>"Virtual machines"</h1>
            <p class="muted">"Ten VMs per page when the cluster has more than ten workloads."</p>
        </section>
        <Suspense fallback=move || view! { <p class="muted">"Loading VMs…"</p> }>
            {move || Suspend::new(async move {
                match res.await {
                    Ok(page_data) => vms_table(page_data).into_any(),
                    Err(e) => view! { <p class="err">{e.to_string()}</p> }.into_any(),
                }
            })}
        </Suspense>
    }
}

fn vms_table(data: VmsPageDto) -> impl IntoView {
    let VmsPageDto {
        vms,
        page,
        total_pages,
        total,
        has_prev,
        has_next,
        ..
    } = data;
    let prev = page.saturating_sub(1);
    let next = page.saturating_add(1);
    let prev_href = if has_prev {
        format!("/vms?page={prev}")
    } else {
        String::new()
    };
    let next_href = if has_next {
        format!("/vms?page={next}")
    } else {
        String::new()
    };
    view! {
        <div class="table-wrap">
            <table class="data">
                <thead>
                    <tr>
                        <th>"Name"</th>
                        <th>"State"</th>
                        <th>"Node"</th>
                        <th>"vCPU"</th>
                        <th>"Memory"</th>
                    </tr>
                </thead>
                <tbody>
                    {vms.into_iter().map(vm_row).collect_view()}
                </tbody>
            </table>
        </div>
        <div class="pager">
            <Show when=move || has_prev>
                <a href=prev_href.clone()>"← Previous"</a>
            </Show>
            <span class="muted">
                {format!("Page {page} of {total_pages} — {total} VMs")}
            </span>
            <Show when=move || has_next>
                <a href=next_href.clone()>"Next →"</a>
            </Show>
        </div>
    }
}

fn vm_row(vm: VmRowDto) -> impl IntoView {
    // `Paused` used to fall through to the `badge-stop` arm and look the
    // same as a halted VM, hiding suspend state. Map it to the "warn"
    // visual so operators can immediately tell the workload is suspended.
    let badge_class = match vm.state.as_str() {
        "Running" => "badge badge-run",
        "Error" => "badge badge-warn",
        "Paused" => "badge badge-warn",
        _ => "badge badge-stop",
    };
    view! {
        <tr>
            <td>
                <strong>{vm.name.clone()}</strong>
                <div class="muted" style="font-size: 0.75rem;">
                    <code class="inline">{vm.id.clone()}</code>
                </div>
            </td>
            <td><span class={badge_class}>{vm.state.clone()}</span></td>
            <td><code class="inline">{vm.node_id.clone()}</code></td>
            <td>{vm.cpu}</td>
            <td>{vm.memory.clone()}</td>
        </tr>
    }
}

#[component]
fn NetworksPage() -> impl IntoView {
    let sdn_res = Resource::new(|| (), |_| list_networks_dto());
    let overview_res = Resource::new(|| (), |_| get_network_overview_dto());
    view! {
        <section class="hero">
            <h1>"Networks"</h1>
            <p class="muted">"Host networking, VLAN configs, and software-defined networks across all nodes."</p>
        </section>

        <h2 class="section-title">"Host networking"</h2>
        <Suspense fallback=move || view! { <p class="muted">"Loading host network data…"</p> }>
            {move || Suspend::new(async move {
                match overview_res.await {
                    Ok(data) => host_networking_view(data).into_any(),
                    Err(e) => view! { <p class="err">{e.to_string()}</p> }.into_any(),
                }
            })}
        </Suspense>

        <h2 class="section-title">"Software-defined networks"</h2>
        <Suspense fallback=move || view! { <p class="muted">"Loading SDN data…"</p> }>
            {move || Suspend::new(async move {
                match sdn_res.await {
                    Ok(rows) => sdn_table(rows).into_any(),
                    Err(e) => view! { <p class="err">{e.to_string()}</p> }.into_any(),
                }
            })}
        </Suspense>
    }
}

fn host_networking_view(data: NetworkOverviewDto) -> impl IntoView {
    let has_nodes = !data.nodes.is_empty();
    view! {
        <section class="card" style="margin-bottom: 1rem;">
            <h2>"Default network config"</h2>
            <dl class="kv">
                <dt>"Gateway iface"</dt><dd><code class="inline">{data.default_gateway_interface.clone()}</code></dd>
                <dt>"External IP"</dt><dd>{data.default_external_ip.clone()}</dd>
                <dt>"Gateway IP"</dt><dd>{data.default_gateway_ip.clone()}</dd>
                <dt>"Netmask"</dt><dd>{data.default_internal_netmask.clone()}</dd>
            </dl>
        </section>
        <Show when=move || has_nodes>
            {data.nodes.clone().into_iter().map(node_network_card).collect_view()}
        </Show>
        <Show when=move || !has_nodes>
            <div class="empty-note">"No approved nodes found."</div>
        </Show>
    }
}

fn node_network_card(node: NodeNetworkDto) -> impl IntoView {
    let vxlan_label = if node.disable_vxlan {
        "disabled"
    } else {
        "enabled"
    };
    let has_ifaces = !node.interfaces.is_empty();
    let interfaces = node.interfaces.clone();
    view! {
        <section class="card node-card">
            <div class="node-card-header">
                <span class="node-label">{node.hostname.clone()}</span>
                <code class="inline">{node.node_id.clone()}</code>
                <span class="muted">{node.address.clone()}</span>
            </div>
            <dl class="kv">
                <dt>"Gateway iface"</dt><dd><code class="inline">{node.gateway_interface.clone()}</code></dd>
                <dt>"VXLAN"</dt><dd>{vxlan_label}</dd>
            </dl>
            <Show when=move || has_ifaces>
                <div class="table-wrap" style="margin-top: 0.75rem;">
                    <table class="data">
                        <thead>
                            <tr>
                                <th>"Interface"</th>
                                <th>"Type"</th>
                                <th>"State"</th>
                                <th>"MAC"</th>
                                <th>"MTU"</th>
                                <th>"Addresses"</th>
                            </tr>
                        </thead>
                        <tbody>
                            {interfaces.clone().into_iter().map(iface_row).collect_view()}
                        </tbody>
                    </table>
                </div>
            </Show>
            <Show when=move || !has_ifaces>
                <p class="muted" style="margin-top: 0.5rem; font-size: 0.85rem;">"Node unreachable — no interface data."</p>
            </Show>
        </section>
    }
}

fn iface_row(iface: HostInterfaceDto) -> impl IntoView {
    let badge_class = format!("badge-iface badge-{}", iface.kind);
    let state_class = if iface.state == "UP" {
        "badge badge-up"
    } else {
        "badge badge-down"
    };
    let addrs = iface.addresses.join(", ");
    view! {
        <tr>
            <td><code class="inline">{iface.name.clone()}</code></td>
            <td><span class={badge_class}>{iface.kind.clone()}</span></td>
            <td><span class={state_class}>{iface.state.clone()}</span></td>
            <td><code class="inline">{iface.mac_address.clone()}</code></td>
            <td>{iface.mtu}</td>
            <td>{addrs}</td>
        </tr>
    }
}

fn sdn_table(rows: Vec<NetworkRowDto>) -> impl IntoView {
    if rows.is_empty() {
        return view! {
            <div class="empty-note">"No software-defined networks configured. Use "<code class="inline">"kctl create network"</code>" to add one."</div>
        }
        .into_any();
    }
    view! {
        <div class="table-wrap">
            <table class="data">
                <thead>
                    <tr>
                        <th>"Name"</th>
                        <th>"Type"</th>
                        <th>"Node"</th>
                        <th>"External"</th>
                        <th>"Gateway"</th>
                        <th>"Netmask"</th>
                        <th>"VLAN"</th>
                        <th>"Outbound NAT"</th>
                    </tr>
                </thead>
                <tbody>
                    {rows.into_iter().map(|n| view! {
                        <tr>
                            <td><strong>{n.name.clone()}</strong></td>
                            <td>{n.network_type.clone()}</td>
                            <td><code class="inline">{n.node_id.clone()}</code></td>
                            <td>{n.external_ip.clone()}</td>
                            <td>{n.gateway_ip.clone()}</td>
                            <td>{n.internal_netmask.clone()}</td>
                            <td>{n.vlan_id}</td>
                            <td>{if n.enable_outbound_nat { "yes" } else { "no" }}</td>
                        </tr>
                    }).collect_view()}
                </tbody>
            </table>
        </div>
    }
    .into_any()
}

#[component]
fn StoragePage() -> impl IntoView {
    let res = Resource::new(|| (), |_| get_storage_overview_dto());
    view! {
        <section class="hero">
            <h1>"Storage"</h1>
            <p class="muted">
                "Cluster-wide VM data-plane type (filesystem, LVM, ZFS) and LUKS posture from the controller; "
                "block devices are queried on each node with "
                <code class="inline">"lsblk"</code>" (top-level disks)."
            </p>
        </section>
        <Suspense fallback=move || view! { <p class="muted">"Loading storage overview…"</p> }>
            {move || Suspend::new(async move {
                match res.await {
                    Ok(data) => storage_overview_view(data).into_any(),
                    Err(e) => view! { <p class="err">{e.to_string()}</p> }.into_any(),
                }
            })}
        </Suspense>
    }
}

fn storage_overview_view(data: StorageOverviewDto) -> impl IntoView {
    let nodes = data.nodes.clone();
    let has_nodes = !nodes.is_empty();
    view! {
        <h2 class="section-title">"Cluster"</h2>
        <div class="card-grid cols-2">
            <section class="card">
                <h2>"VM / data storage type"</h2>
                <p class="muted" style="font-size: 0.85rem; margin-bottom: 0.75rem;">
                    "Per-node backend declared at registration (where VM disks live)."
                </p>
                <div class="stat-row">
                    <div class="stat"><div class="label">"Filesystem"</div><div class="value">{data.backend_filesystem_nodes}</div></div>
                    <div class="stat"><div class="label">"LVM"</div><div class="value">{data.backend_lvm_nodes}</div></div>
                    <div class="stat"><div class="label">"ZFS"</div><div class="value">{data.backend_zfs_nodes}</div></div>
                    <div class="stat"><div class="label">"Unspecified"</div><div class="value">{data.backend_unspecified_nodes}</div></div>
                </div>
            </section>
            <section class="card">
                <h2>"LUKS (cluster)"</h2>
                <dl class="kv">
                    <dt>"TPM2-sealed"</dt><dd>{data.nodes_luks_tpm2}</dd>
                    <dt>"Key file"</dt><dd>{data.nodes_luks_keyfile}</dd>
                    <dt>"Unknown / not reported"</dt><dd>{data.nodes_luks_unknown}</dd>
                </dl>
            </section>
            <section class="card" style="grid-column: 1 / -1;">
                <h2>"Disk inventory"</h2>
                <div class="stat-row">
                    <div class="stat"><div class="label">"Approved nodes"</div><div class="value">{data.approved_nodes}</div></div>
                    <div class="stat"><div class="label">"Nodes with disk list"</div><div class="value">{data.nodes_disk_inventory_ok}</div></div>
                    <div class="stat"><div class="label">"Block devices (total)"</div><div class="value">{data.total_block_devices}</div></div>
                </div>
            </section>
        </div>

        <h2 class="section-title" style="margin-top: 1.5rem;">"Nodes"</h2>
        <Show when=move || has_nodes>
            {nodes.clone().into_iter().map(storage_node_card).collect_view()}
        </Show>
        <Show when=move || !has_nodes>
            <div class="empty-note">"No approved nodes."</div>
        </Show>
    }
}

fn storage_node_card(node: NodeStorageDto) -> impl IntoView {
    let inv_badge = if node.disk_inventory_ok {
        "badge badge-run"
    } else {
        "badge badge-warn"
    };
    let inv_label = if node.disk_inventory_ok {
        "ok"
    } else {
        "unavailable"
    };
    let has_disks = !node.disks.is_empty();
    let disks = node.disks.clone();
    let disk_inventory_ok = node.disk_inventory_ok;
    let luks_dd = if node.luks_method.is_empty() {
        "—".to_string()
    } else {
        node.luks_method.clone()
    };
    let has_vgs = !node.lvm_volume_groups.is_empty();
    let has_lvs = !node.lvm_logical_volumes.is_empty();
    let has_pvs = !node.lvm_physical_volumes.is_empty();
    let has_lvm = node.lvm_inventory_ok && (has_vgs || has_lvs || has_pvs);
    let vgs = node.lvm_volume_groups.clone();
    let lvs = node.lvm_logical_volumes.clone();
    let pvs = node.lvm_physical_volumes.clone();
    view! {
        <section class="card node-card">
            <div class="node-card-header">
                <span class="node-label">{node.hostname.clone()}</span>
                <code class="inline">{node.node_id.clone()}</code>
                <span class="muted">{node.address.clone()}</span>
            </div>
            <dl class="kv">
                <dt>"VM storage type"</dt><dd>{node.storage_backend.clone()}</dd>
                <dt>"LUKS"</dt><dd>{luks_dd}</dd>
                <dt>"Disk inventory"</dt><dd><span class={inv_badge}>{inv_label}</span></dd>
            </dl>
            <Show when=move || has_disks>
                <div class="table-wrap" style="margin-top: 0.75rem;">
                    <table class="data">
                        <thead>
                            <tr>
                                <th>"Device"</th>
                                <th>"Path"</th>
                                <th>"Size"</th>
                                <th>"Model"</th>
                                <th>"FS"</th>
                                <th>"Mount"</th>
                                <th>"Hint"</th>
                            </tr>
                        </thead>
                        <tbody>
                            {disks.clone().into_iter().map(storage_disk_row).collect_view()}
                        </tbody>
                    </table>
                </div>
            </Show>
            <Show when=move || !has_disks>
                <p class="muted" style="margin-top: 0.5rem; font-size: 0.85rem;">
                    {if disk_inventory_ok {
                        "No top-level block devices reported."
                    } else {
                        "Could not list disks (node unreachable or RPC failed)."
                    }}
                </p>
            </Show>
            <Show when=move || has_lvm>
                {lvm_detail_view(vgs.clone(), lvs.clone(), pvs.clone())}
            </Show>
        </section>
    }
}

fn lvm_detail_view(
    vgs: Vec<LvmVolumeGroupDto>,
    lvs: Vec<LvmLogicalVolumeDto>,
    pvs: Vec<LvmPhysicalVolumeDto>,
) -> impl IntoView {
    let has_vgs = !vgs.is_empty();
    let has_lvs = !lvs.is_empty();
    let has_pvs = !pvs.is_empty();
    view! {
        <h3 style="margin-top: 1rem; font-size: 0.85rem; text-transform: uppercase; color: var(--muted); letter-spacing: 0.06em;">"LVM inventory"</h3>
        <Show when=move || has_vgs>
            <div class="table-wrap" style="margin-top: 0.5rem;">
                <table class="data">
                    <thead>
                        <tr>
                            <th>"Volume group"</th>
                            <th>"Size"</th>
                            <th>"Free"</th>
                            <th>"Attr"</th>
                        </tr>
                    </thead>
                    <tbody>
                        {vgs.clone().into_iter().map(|vg| view! {
                            <tr>
                                <td><code class="inline">{vg.name.clone()}</code></td>
                                <td>{vg.size.clone()}</td>
                                <td>{vg.free.clone()}</td>
                                <td><code class="inline">{vg.attr.clone()}</code></td>
                            </tr>
                        }).collect_view()}
                    </tbody>
                </table>
            </div>
        </Show>
        <Show when=move || has_lvs>
            <div class="table-wrap" style="margin-top: 0.5rem;">
                <table class="data">
                    <thead>
                        <tr>
                            <th>"Logical volume"</th>
                            <th>"VG"</th>
                            <th>"Size"</th>
                            <th>"Attr"</th>
                            <th>"Path"</th>
                            <th>"Pool"</th>
                            <th>"Data %"</th>
                        </tr>
                    </thead>
                    <tbody>
                        {lvs.clone().into_iter().map(|lv| view! {
                            <tr>
                                <td><code class="inline">{lv.name.clone()}</code></td>
                                <td><code class="inline">{lv.vg_name.clone()}</code></td>
                                <td>{lv.size.clone()}</td>
                                <td><code class="inline">{lv.attr.clone()}</code></td>
                                <td><code class="inline">{lv.path.clone()}</code></td>
                                <td>{if lv.pool.is_empty() { "—".to_string() } else { lv.pool.clone() }}</td>
                                <td>{if lv.data_percent.is_empty() { "—".to_string() } else { lv.data_percent.clone() }}</td>
                            </tr>
                        }).collect_view()}
                    </tbody>
                </table>
            </div>
        </Show>
        <Show when=move || has_pvs>
            <div class="table-wrap" style="margin-top: 0.5rem;">
                <table class="data">
                    <thead>
                        <tr>
                            <th>"Physical volume"</th>
                            <th>"VG"</th>
                            <th>"Size"</th>
                            <th>"Free"</th>
                            <th>"Attr"</th>
                        </tr>
                    </thead>
                    <tbody>
                        {pvs.clone().into_iter().map(|pv| view! {
                            <tr>
                                <td><code class="inline">{pv.name.clone()}</code></td>
                                <td><code class="inline">{pv.vg_name.clone()}</code></td>
                                <td>{pv.size.clone()}</td>
                                <td>{pv.free.clone()}</td>
                                <td><code class="inline">{pv.attr.clone()}</code></td>
                            </tr>
                        }).collect_view()}
                    </tbody>
                </table>
            </div>
        </Show>
    }
}

fn storage_disk_row(d: StorageDiskRowDto) -> impl IntoView {
    let StorageDiskRowDto {
        name,
        path,
        size,
        model,
        fstype,
        mountpoint,
        role_hint,
    } = d;
    let mp = if mountpoint.is_empty() {
        "—".to_string()
    } else {
        mountpoint
    };
    let fs = if fstype.is_empty() {
        "—".to_string()
    } else {
        fstype
    };
    view! {
        <tr>
            <td><code class="inline">{name}</code></td>
            <td><code class="inline">{path}</code></td>
            <td>{size}</td>
            <td>{model}</td>
            <td>{fs}</td>
            <td>{mp}</td>
            <td><span class="muted">{role_hint}</span></td>
        </tr>
    }
}
