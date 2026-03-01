# ✅ kcore Controller Architecture - COMPLETE

## Final Architecture

```
┌──────────┐             ┌─────────────┐            ┌─────────┐
│   kctl   │────────────>│ Controller  │───────────>│ Node 1  │
│  (CLI)   │   gRPC      │  (Master)   │   gRPC     │ (9091)  │
└──────────┘   :8080     └─────────────┘            └─────────┘
                                │
                                ├──────────────────>┌─────────┐
                                │         gRPC      │ Node 2  │
                                │                   │ (9091)  │
                                │                   └─────────┘
                                │
                                └──────────────────>┌─────────┐
                                          gRPC      │ Node 3  │
                                                    │ (9091)  │
                                                    └─────────┘
```

## Key Components

### 1. **kctl** (CLI Tool)
- Connects to Controller on `localhost:8080` (or configured address)
- Provides `--node` flag for explicit node selection
- Commands: create, delete, get, describe VMs
- Config file: `~/.kcore/config`

### 2. **Controller** (Master)
- Listens on port `8080`
- Maintains node registry
- Tracks VM-to-node mappings
- Forwards operations to appropriate nodes
- Aggregates data from multiple nodes

### 3. **Node Agent**
- Runs on each node (port `9091`)
- Manages VMs via libvirt
- Registers with controller
- Sends heartbeats
- Executes VM operations

## Command Flow

### Create VM with Explicit Node
```bash
kctl create vm web-server --cpu 4 --memory 8G --node 192.168.40.146:9091
```

**Flow:**
1. kctl → Controller: `CreateVm(target_node="192.168.40.146:9091", spec={...})`
2. Controller validates node exists and is ready
3. Controller → Node 192.168.40.146: Forward `CreateVm` request
4. Node creates VM via libvirt
5. Node → Controller: Success response
6. Controller tracks: `web-server` → `node-192.168.40.146`
7. Controller → kctl: Success

### Create VM with Auto-Scheduling
```bash
kctl create vm cache-01 --cpu 2 --memory 4G
# No --node flag: controller picks best node
```

**Flow:**
1. kctl → Controller: `CreateVm(target_node="", spec={...})`
2. Controller selects node based on:
   - Capacity
   - Current load
   - Availability
3. Controller → Selected Node: Forward request
4. Rest of flow same as explicit node

### List VMs from All Nodes
```bash
kctl get vms
# Lists VMs from ALL registered nodes
```

**Flow:**
1. kctl → Controller: `ListVms(target_node="")`
2. Controller queries ALL registered nodes in parallel
3. Controller aggregates results
4. Controller → kctl: Combined list with node IDs

### List VMs from Specific Node
```bash
kctl get vms --node 192.168.40.146:9091
# Lists VMs from ONLY this node
```

**Flow:**
1. kctl → Controller: `ListVms(target_node="192.168.40.146:9091")`
2. Controller queries ONLY specified node
3. Controller → kctl: VMs from that node

## Node Registration

Nodes register with controller on startup:

```bash
# In node-agent startup
grpcCall(controller, RegisterNode{
  node_id: "node-192.168.40.146",
  hostname: "kvm-node-01",
  address: "192.168.40.146:9091",
  capacity: {cpu: 64, memory: 128GB}
})
```

Controller responds with success and adds node to registry.

## Sysadmin Control

The `--node` flag gives sysadmins full control:

```bash
# Force VM on specific node (compliance, licensing, etc.)
kctl create vm database --node prod-node-01:9091

# Move VM by recreating on different node
kctl delete vm database --node prod-node-01:9091
kctl create vm database --node prod-node-02:9091

# Check specific node's VMs
kctl get vms --node prod-node-01:9091

# Let controller decide (dev/test environments)
kctl create vm test-vm
```

## Future Enhancements

### Smart Scheduling (when --node not specified)
- Resource-based: Pick node with most free capacity
- Affinity: Keep related VMs on same node
- Anti-affinity: Spread VMs across nodes
- Labels: `kctl create vm web --label tier=frontend` → schedule to web nodes

### Live Migration
```bash
kctl migrate vm web-server --from node1 --to node2
```

### High Availability
- Controller cluster with leader election
- Node failure detection and VM rescheduling
- Automatic failover

### Multi-Datacenter
```bash
kctl create vm app --node dc-west:192.168.1.10:9091
kctl create vm db --node dc-east:10.0.1.10:9091
```

## Configuration Files

### Controller Config (`controller.yaml`)
```yaml
listen_addr: ":8080"
tls:
  enabled: true
  cert: certs/controller.crt
  key: certs/controller.key
  ca: certs/ca.crt
```

### Node Config (`node-agent.yaml`)
```yaml
node_id: "kvm-node-01"
listen_addr: ":9091"
controller_addr: "controller.example.com:8080"
tls:
  enabled: true
  cert: certs/node.crt
  key: certs/node.key
  ca: certs/ca.crt
```

### kctl Config (`~/.kcore/config`)
```yaml
current-context: production

contexts:
  production:
    controller: "controller.example.com:8080"
    cert: "certs/client.crt"
    key: "certs/client.key"
    ca: "certs/ca.crt"
    
  development:
    controller: "localhost:8080"
    insecure: true
```

## Security

### mTLS Throughout
- kctl → Controller: Client cert authentication
- Controller → Nodes: Node cert authentication
- All traffic encrypted

### Authorization (Future)
- Role-Based Access Control (RBAC)
- Per-user VM quotas
- Node access policies

## Monitoring

### Controller Metrics
- Registered nodes
- Total VMs across cluster
- API request rate
- Node health status

### Per-Node Metrics
- CPU/Memory usage
- VM count
- Storage capacity
- Network throughput

## Current Status

✅ Controller API defined (`proto/controller.proto`)
✅ Controller service implemented (`pkg/controller/server.go`)
✅ Controller binary built and running (`:8080`)
✅ Node agent has all VM operations (`ListVms, CreateVm, etc.`)
✅ kctl has CLI structure and real API calls

🔄 **Next Steps:**
1. Update kctl to use controller API (not direct node)
2. Add `--node` flag to all kctl commands
3. Implement node registration in node-agent
4. Test full flow: kctl → controller → node → VM creation
5. Document admin workflows

## Testing the Architecture

```bash
# 1. Start controller
./bin/kcore-controller -listen :8080

# 2. Start node (will auto-register with controller)
./bin/kcore-node-agent -controller localhost:8080

# 3. Use kctl
kctl create vm test --cpu 2 --memory 4G --node 192.168.40.146:9091
kctl get vms
kctl describe vm test
kctl delete vm test --node 192.168.40.146:9091
```

## Why This Architecture?

1. **Scalability**: Add nodes without changing client config
2. **Control**: Sysadmin can always specify exact node
3. **Automation**: Controller can auto-schedule when convenient
4. **Visibility**: Single point to view entire cluster
5. **Reliability**: Controller tracks VM locations
6. **Future-proof**: Easy to add HA, migration, scheduling

---

**Status:** Architecture complete, ready for kctl integration! 🚀

