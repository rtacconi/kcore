# TLS Certificates for kcore

## Certificate Files

- `ca.crt` / `ca.key` - Certificate Authority (used to sign other certificates)
- `controller.crt` / `controller.key` - Controller certificate
- `node.crt` / `node.key` - Node agent certificate (example for node-01)

## Usage

### Controller Configuration

The controller uses:
- `ca.crt` - To verify node certificates
- `controller.crt` / `controller.key` - For its own TLS identity

### Node Agent Configuration

Each node agent needs:
- `ca.crt` - To verify controller certificate
- `node.crt` / `node.key` - For its own TLS identity

**Note:** For multiple nodes, generate separate certificates:
- `node-01.crt` / `node-01.key` for first node
- `node-02.crt` / `node-02.key` for second node
- etc.

## Generate Additional Node Certificates

```bash
# For node-02
openssl genrsa -out node-02.key 4096
openssl req -new -key node-02.key -out node-02.csr -subj "/CN=kcore-node-02"
openssl x509 -req -days 365 -in node-02.csr -CA ca.crt -CAkey ca.key -CAcreateserial -out node-02.crt
```

## Security Notes

- Keep `*.key` files secure and never commit them to git
- The `.gitignore` already excludes `*.key`, `*.crt`, `*.pem` files
- For production, use a proper PKI system (e.g., cert-manager, Vault)
- These are self-signed certificates suitable for development/testing

