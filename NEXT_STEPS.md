# Next Steps for kcore

## ✅ Completed

1. **Project Structure** - Created complete Go project layout
2. **Protobuf/gRPC API** - Defined node and controller APIs
3. **SQLite Database** - Schema and access layer implemented
4. **Control Plane** - Reconciliation logic and VM lifecycle management
5. **Node Agent** - Libvirt integration and gRPC server
6. **Storage Drivers** - Local-dir and local-lvm implementations
7. **YAML Specs** - Parsing for VM, Volume, StorageClass
8. **NixOS Flake** - kcode branding and node configuration
9. **Protobuf Code** - Generated Go code from .proto files
10. **Controller Binary** - Built successfully (`bin/kcore-controller`)

## 📋 Next Steps

### Step 1: Set Up TLS Certificates (mTLS)

Create certificates for secure communication between controller and node-agents:

```bash
mkdir -p certs
cd certs

# Generate CA
openssl genrsa -out ca.key 4096
openssl req -new -x509 -days 365 -key ca.key -out ca.crt -subj "/CN=kcore-ca"

# Generate controller certificate
openssl genrsa -out controller.key 4096
openssl req -new -key controller.key -out controller.csr -subj "/CN=kcore-controller"
openssl x509 -req -days 365 -in controller.csr -CA ca.crt -CAkey ca.key -CAcreateserial -out controller.crt

# Generate node certificate (repeat for each node)
openssl genrsa -out node.key 4096
openssl req -new -key node.key -out node.csr -subj "/CN=kcore-node-01"
openssl x509 -req -days 365 -in node.csr -CA ca.crt -CAkey ca.key -CAcreateserial -out node.crt
```

### Step 2: Configure Controller

Create `controller.yaml`:

```yaml
databasePath: ./kcore.db
listenAddr: ":9090"

tls:
  caFile: ./certs/ca.crt
  certFile: ./certs/controller.crt
  keyFile: ./certs/controller.key

nodeNetworks:
  default: br0
```

### Step 3: Start Controller

```bash
./bin/kcore-controller
```

The controller will:
- Initialize SQLite database
- Start reconciliation loop
- Wait for node registrations

### Step 4: Build Node Agent on Linux

On your ThinkCentre node (or any Linux system):

```bash
# Install dependencies
sudo apt-get install libvirt-dev pkg-config gcc

# Clone/sync kcore code
cd /path/to/kcore

# Build node-agent
make node-agent-podman
# OR if you have the code synced:
GOOS=linux GOARCH=amd64 CGO_ENABLED=1 go build -o bin/kcore-node-agent-linux-amd64 ./cmd/node-agent
```

### Step 5: Configure Node Agent

On each ThinkCentre node, create `/etc/kcode/node-agent.yaml`:

```yaml
nodeId: thinkcentre-01
controllerAddr: "192.168.1.100:9090"  # Your Mac's IP

tls:
  caFile: /etc/kcode/ca.crt
  certFile: /etc/kcode/node.crt
  keyFile: /etc/kcode/node.key

networks:
  default: br0

storage:
  drivers:
    local-dir:
      type: local-dir
      parameters:
        path: /var/lib/kcode/disks
    local-lvm:
      type: local-lvm
      parameters:
        volumeGroup: vg0
```

Copy certificates to node:
```bash
scp certs/ca.crt certs/node.crt certs/node.key user@thinkcentre:/tmp/
ssh user@thinkcentre 'sudo mkdir -p /etc/kcode && sudo mv /tmp/{ca,node}.{crt,key} /etc/kcode/'
```

### Step 6: Deploy Node Agent

```bash
# Copy binary
scp bin/kcore-node-agent-linux-amd64 user@thinkcentre:/tmp/

# Install and start service
ssh user@thinkcentre 'sudo mkdir -p /opt/kcode && sudo mv /tmp/kcore-node-agent-linux-amd64 /opt/kcode/kcore-node-agent && sudo chmod +x /opt/kcode/kcore-node-agent && sudo systemctl restart kcode-node-agent'
```

Or use the deployment script:
```bash
make deploy NODE=192.168.1.100 USER=youruser
```

### Step 7: Create Storage Classes

```bash
# Create storage classes
cat > storage-classes.yaml <<EOF
apiVersion: kcore.io/v1
kind: StorageClass
metadata:
  name: local-dir
spec:
  driver: local-dir
  shared: false
  parameters:
    path: /var/lib/kcode/disks

---
apiVersion: kcore.io/v1
kind: StorageClass
metadata:
  name: local-lvm
spec:
  driver: local-lvm
  shared: false
  parameters:
    volumeGroup: vg0
EOF

# Apply (note: controller doesn't have apply command yet - this is a placeholder)
# For now, you'll need to add storage classes directly to SQLite or implement the apply command
```

### Step 8: Create and Apply VM Spec

```bash
# Create a VM spec
cat > test-vm.yaml <<EOF
apiVersion: kcore.io/v1
kind: VM
metadata:
  name: test-vm
  namespace: default
spec:
  nodeSelector:
    dc: dc-a
  cpu: 2
  memoryBytes: 4GiB
  disks:
    - name: root
      sizeBytes: 40GiB
      storageClassName: local-lvm
      bus: virtio
  nics:
    - network: default
      model: virtio
EOF

# Apply VM spec
./bin/kcore-controller -apply-vm test-vm.yaml
```

### Step 9: Verify VM Creation

Check controller logs and verify VM is created:
- Controller should show VM creation in logs
- Reconciliation loop should schedule VM to a node
- Node-agent should receive CreateVm request
- VM should appear in libvirt (`virsh list`)

## 🔧 Implementation Notes

### Missing Features (To Implement)

1. **Storage Class Management**
   - Add `-apply-storageclass` command to controller
   - Or implement REST API for applying specs

2. **Node Registration**
   - Node-agent needs to register with controller on startup
   - Controller needs to handle registration requests

3. **VM State Management**
   - Track VM state changes
   - Handle VM migrations
   - Implement VM updates

4. **Volume Lifecycle**
   - Volume provisioning workflow
   - Volume attachment/detachment tracking

5. **Network Management**
   - Network creation and management
   - Bridge configuration validation

## 🧪 Testing Checklist

- [ ] Controller starts and initializes database
- [ ] Node-agent connects to controller
- [ ] Node registration works
- [ ] Storage class creation works
- [ ] VM creation from YAML works
- [ ] VM appears in libvirt
- [ ] VM can be started/stopped
- [ ] Volume provisioning works
- [ ] Volume attachment works

## 📚 Documentation Needed

- [ ] API documentation
- [ ] Configuration reference
- [ ] Troubleshooting guide
- [ ] Architecture diagrams
- [ ] Deployment guide

