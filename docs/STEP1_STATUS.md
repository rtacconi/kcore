# Step 1: Install Dependencies - ✅ COMPLETED

## ✅ All Dependencies Installed:

1. **Go 1.25.0** ✅ Installed
2. **protoc (Protocol Buffers compiler) 32.0** ✅ Installed via Nix
3. **protoc-gen-go** ✅ Installed to `~/go/bin/`
4. **protoc-gen-go-grpc** ✅ Installed to `~/go/bin/`
5. **Go modules** ✅ Downloaded and tidied

## Verification:
```bash
protoc --version          # libprotoc 32.0
which protoc-gen-go       # ~/go/bin/protoc-gen-go
which protoc-gen-go-grpc  # ~/go/bin/protoc-gen-go-grpc
```

## ⚠️ Important Note:
Make sure `~/go/bin` is in your PATH. If not, add this to your shell config:
```bash
export PATH="$HOME/go/bin:$PATH"
```

## ✅ Step 1 Complete!

**Next Step:** Generate Protobuf Code
```bash
cd /Users/riccardotacconi/dev/kcore
export PATH="$HOME/go/bin:$PATH"  # Ensure Go bin is in PATH
./scripts/generate-proto.sh
# or
make proto
```

