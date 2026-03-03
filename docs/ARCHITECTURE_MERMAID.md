# kcore Architecture — Mermaid Schematics

This document provides Mermaid diagrams of the current kcore architecture, based on the project docs and codebase.

**Viewing tip:** If diagrams still look small, open this file in [Mermaid Live Editor](https://mermaid.live), paste one diagram at a time, and use the zoom/export controls for a larger view.

---

## 1. High-Level System Architecture

```mermaid
%%{init: {'themeVariables': {'fontSize': '20px', 'fontFamily': 'arial'}}}%%
flowchart TB
    subgraph Workstation["Workstation (Mac/Linux)"]
        kctl["kctl (CLI)"]
        config["~/.kcore/config"]
        kctl --> config
    end

    subgraph ControllerHost["Controller Host"]
        Controller["kcore-controller<br/>:8080 (gRPC + mTLS)"]
        ControllerAdmin["ControllerAdmin API"]
        ControlPlane["ControlPlane API<br/>(unified, delegates to Controller)"]
        Controller --> ControllerAdmin
        Controller --> ControlPlane
        State["In-memory state<br/>• Node registry<br/>• VM → Node mapping"]
        Controller --> State
    end

    subgraph Node1["KVM Node 1"]
        NA1["node-agent<br/>:9091"]
        libvirt1["libvirtd"]
        VM1a["VM"] 
        VM1b["VM"]
        NA1 --> libvirt1
        libvirt1 --> VM1a
        libvirt1 --> VM1b
    end

    subgraph Node2["KVM Node 2"]
        NA2["node-agent<br/>:9091"]
        libvirt2["libvirtd"]
        VM2["VMs..."]
        NA2 --> libvirt2
        libvirt2 --> VM2
    end

    subgraph NodeN["KVM Node N"]
        NAN["node-agent<br/>:9091"]
        libvirtN["libvirtd"]
        VMN["VMs..."]
        NAN --> libvirtN
        libvirtN --> VMN
    end

    kctl -->|"gRPC (mTLS)"| Controller
    Controller -->|"gRPC (mTLS)"| NA1
    Controller -->|"gRPC (mTLS)"| NA2
    Controller -->|"gRPC (mTLS)"| NAN
    NA1 -.->|"RegisterNode, Heartbeat, SyncVmState"| Controller
    NA2 -.->|"RegisterNode, Heartbeat, SyncVmState"| Controller
    NAN -.->|"RegisterNode, Heartbeat, SyncVmState"| Controller
```

---

## 2. Controller Services (Single Binary)

The controller binary (`cmd/controller`) exposes three gRPC services on the same port. ControlPlane delegates to the same in-memory Controller implementation.

```mermaid
%%{init: {'themeVariables': {'fontSize': '20px', 'fontFamily': 'arial'}}}%%
flowchart LR
    subgraph ControllerBinary["bin/kcore-controller :8080"]
        direction TB
        GrpcServer["gRPC Server (TLS)"]
        Ctrl["Controller service<br/>proto/controller.proto"]
        Admin["ControllerAdmin service"]
        CP["ControlPlane service<br/>proto/controlplane.proto"]
        GrpcServer --> Ctrl
        GrpcServer --> Admin
        GrpcServer --> CP
        CP -.->|"delegates"| Ctrl
        CP -.->|"delegates"| Admin
    end

    subgraph ControllerImpl["pkg/controller"]
        Server["Server (in-memory)"]
        Server --> Nodes["nodes map"]
        Server --> VmToNode["vmToNode map"]
        Server --> NodeDialCreds["TLS creds to nodes"]
    end

    Ctrl --> Server
    Admin --> Server
```

---

## 3. Node Agent Internal Structure

Each node runs one node-agent process. It implements the node proto services and talks to libvirt and storage drivers.

```mermaid
%%{init: {'themeVariables': {'fontSize': '20px', 'fontFamily': 'arial'}}}%%
flowchart TB
    subgraph NodeAgent["cmd/node-agent (e.g. :9091)"]
        direction TB
        GrpcNA["gRPC Server (TLS)"]
        NodeCompute["NodeCompute"]
        NodeStorage["NodeStorage"]
        NodeInfo["NodeInfo"]
        NodeAdmin["NodeAdmin"]
        GrpcNA --> NodeCompute
        GrpcNA --> NodeStorage
        GrpcNA --> NodeInfo
        GrpcNA --> NodeAdmin
    end

    subgraph NodePkg["node/ package"]
        Server["node.Server"]
        LibvirtMgr["libvirt Manager"]
        StorageReg["Storage Driver Registry"]
        Server --> NodeCompute
        Server --> NodeStorage
        Server --> NodeInfo
        Server --> NodeAdmin
        Server --> LibvirtMgr
        Server --> StorageReg
    end

    subgraph StorageDrivers["Storage drivers"]
        LocalDir["local-dir"]
        LocalLVM["local-lvm"]
        StorageReg --> LocalDir
        StorageReg --> LocalLVM
    end

    LibvirtMgr --> libvirtd["libvirtd"]
    LocalDir --> disk["/var/lib/kcore/disks"]
    LocalLVM --> lvm["LVM VG"]

    subgraph ControllerSync["When ControllerAddr set"]
        Loop["State sync loop (e.g. 10s)"]
        Loop --> Register["RegisterNode"]
        Loop --> Heartbeat["Heartbeat"]
        Loop --> SyncState["SyncVmState"]
        Register --> Controller
        Heartbeat --> Controller
        SyncState --> Controller
    end

    NodeAgent --> Loop
```

---

## 4. Node Registration and VM Creation Flow

```mermaid
%%{init: {'themeVariables': {'fontSize': '20px', 'fontFamily': 'arial'}}}%%
sequenceDiagram
    participant kctl
    participant Controller
    participant NodeAgent
    participant libvirtd

    Note over NodeAgent,Controller: Startup: node-agent → controller
    NodeAgent->>Controller: RegisterNode(node_id, hostname, address, capacity)
    Controller-->>NodeAgent: success
    loop Every heartbeat interval
        NodeAgent->>Controller: Heartbeat(node_id, usage)
        Controller-->>NodeAgent: success
    end
    loop Periodically (e.g. 10s)
        NodeAgent->>Controller: SyncVmState(node_id, vms[])
        Controller-->>NodeAgent: success
    end

    Note over kctl,libvirtd: VM create: kctl → controller → node
    kctl->>Controller: CreateVm(target_node, spec)
    Controller->>Controller: resolve target_node or schedule
    Controller->>NodeAgent: CreateVm(spec)
    NodeAgent->>libvirtd: define & start domain (libvirt)
    libvirtd-->>NodeAgent: VM running
    NodeAgent-->>Controller: CreateVmResponse(vm_id, state)
    Controller->>Controller: record vmToNode[vm_id] = node_id
    Controller-->>kctl: CreateVmResponse(vm_id, node_id, state)
```

---

## 5. List VMs Flow (Single Node vs All Nodes)

```mermaid
%%{init: {'themeVariables': {'fontSize': '20px', 'fontFamily': 'arial'}}}%%
flowchart LR
    subgraph ListAll["kctl get vms (no --node)"]
        kctl1["kctl"] -->|ListVms(target_node='')| Ctrl1["Controller"]
        Ctrl1 -->|query all registered nodes| N1["Node 1"]
        Ctrl1 --> N2["Node 2"]
        Ctrl1 --> N3["Node N"]
        N1 --> Ctrl1
        N2 --> Ctrl1
        N3 --> Ctrl1
        Ctrl1 -->|aggregate| kctl1
    end

    subgraph ListOne["kctl get vms --node 192.168.40.146:9091"]
        kctl2["kctl"] -->|ListVms(target_node=...)| Ctrl2["Controller"]
        Ctrl2 -->|query that node only| N4["Node 192.168.40.146"]
        N4 --> Ctrl2
        Ctrl2 --> kctl2
    end
```

---

## 6. NixOS / ISO Deployment Context

```mermaid
%%{init: {'themeVariables': {'fontSize': '20px', 'fontFamily': 'arial'}}}%%
flowchart TB
    subgraph Build["Build time"]
        Flake["flake.nix"]
        Flake --> ISO["kcore ISO image"]
        Flake --> NodeAgentNix["node-agent (Nix build)"]
    end

    subgraph Install["Install (boot from USB)"]
        USB["USB boot"] --> Live["Live system"]
        Live --> InstallScript["install-to-disk"]
        InstallScript --> Copy["Copy node-agent, certs, config"]
        InstallScript --> Systemd["Configure systemd"]
        Systemd --> Reboot["Reboot"]
    end

    subgraph InstalledNode["Installed node (after reboot)"]
        multi["multi-user.target"]
        net["network-online.target"]
        virtlogd["virtlogd.service"]
        libvirtd["libvirtd.service"]
        kcore_na["kcore-node-agent.service"]
        multi --> net
        multi --> libvirtd
        libvirtd --> virtlogd
        multi --> kcore_na
        kcore_na -->|requires| libvirtd
    end

    ISO --> USB
    NodeAgentNix --> Copy
```

---

## 7. API Surface Summary

```mermaid
%%{init: {'themeVariables': {'fontSize': '20px', 'fontFamily': 'arial'}}}%%
flowchart LR
    subgraph ControllerAPI["Controller (controller.proto)"]
        C1["RegisterNode / Heartbeat / SyncVmState"]
        C2["CreateVm, DeleteVm, StartVm, StopVm, GetVm, ListVms"]
        C3["ListNodes, GetNode"]
    end

    subgraph ControllerAdminAPI["ControllerAdmin (controller.proto)"]
        A1["ApplyNixConfig (controller host)"]
    end

    subgraph ControlPlaneAPI["ControlPlane (controlplane.proto)"]
        P1["All Controller + ControllerAdmin (delegated)"]
        P2["ApplyNodeNixConfig — not implemented"]
        P3["Create/Revoke/List EnrollmentToken — not implemented"]
        P4["GetBootstrapConfig, EnrollNode, RotateNodeCertificate — not implemented"]
        P5["Report/Get/List InstallStatus — not implemented"]
    end

    subgraph NodeAPI["Node (node.proto)"]
        N1["NodeCompute: CreateVm, UpdateVm, DeleteVm, StartVm, StopVm, RebootVm, GetVm, ListVms"]
        N2["NodeCompute: PullImage, ListImages, DeleteImage"]
        N3["NodeStorage: CreateVolume, DeleteVolume, AttachVolume, DetachVolume"]
        N4["NodeInfo: GetNodeInfo"]
        N5["NodeAdmin: ApplyNixConfig (node host)"]
    end
```

---

## 8. Unused / Alternative Path (Reconciler)

The codebase also contains a SQLite-based controller implementation that is **not** wired into the main controller binary.

```mermaid
%%{init: {'themeVariables': {'fontSize': '20px', 'fontFamily': 'arial'}}}%%
flowchart LR
    subgraph Current["Current production path"]
        Main["cmd/controller main.go"]
        Server["pkg/controller.Server"]
        Main --> Server
    end

    subgraph Alternative["Present in codebase but not used"]
        Reconciler["pkg/controller/reconciler.go"]
        SQLite["SQLite DB"]
        Config["YAML specs (VM, StorageClass)"]
        Reconciler --> SQLite
        Reconciler --> Config
        Reconciler -.->|"ApplyVM, scheduling"| NodeClients["Node clients (gRPC)"]
    end
```

---

## Ports and Config Summary

| Component        | Default port | Config / binary |
|-----------------|-------------|------------------|
| Controller      | :8080       | Flags: `-listen`, `-cert`, `-key`, `-ca` |
| Node agent      | :9091       | `/etc/kcore/node-agent.yaml` (NodeID, ControllerAddr, TLS, networks, storage) |
| kctl            | —           | `~/.kcore/config` (controller address, TLS) |

**Communication:** All gRPC with mTLS (client certs for kctl → controller; controller uses its cert to dial nodes).

---

*Generated from docs: ARCHITECTURE.md, ARCHITECTURE_COMPLETE.md, PROJECT_STRUCTURE.md, CONTROLPLANE_API.md, intro.md, NEXT_STEPS.md, and proto definitions.*
