package terraformisolated

import (
	"context"
	"crypto/tls"
	"crypto/x509"
	"encoding/json"
	"fmt"
	"net"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
	"time"

	ctrlpb "github.com/kcore/kcore/api/controller"
	"google.golang.org/grpc"
	"google.golang.org/grpc/codes"
	"google.golang.org/grpc/credentials"
	"google.golang.org/grpc/status"
)

const (
	e2eEnableEnv         = "KCORE_TF_E2E"
	defaultControllerEnv = "KCORE_CONTROLLER_ADDRESS"
	defaultController    = "192.168.40.10:9090"
)

type tofuOutput struct {
	Value interface{} `json:"value"`
}

func TestTerraformIsolatedApplyDestroy(t *testing.T) {
	if os.Getenv(e2eEnableEnv) != "1" {
		t.Skipf("set %s=1 to run this integration test", e2eEnableEnv)
	}

	repoRoot, err := filepath.Abs(filepath.Join("..", ".."))
	if err != nil {
		t.Fatalf("resolve repo root: %v", err)
	}

	projectDir := filepath.Join(repoRoot, "examples", "terraform-isolated")
	applyScript := filepath.Join(projectDir, "apply.sh")
	destroyScript := filepath.Join(projectDir, "destroy.sh")

	for _, p := range []string{applyScript, destroyScript} {
		if _, err := os.Stat(p); err != nil {
			t.Fatalf("required file missing %s: %v", p, err)
		}
	}

	controllerAddr := strings.TrimSpace(os.Getenv(defaultControllerEnv))
	if controllerAddr == "" {
		controllerAddr = defaultController
	}
	if _, err := net.DialTimeout("tcp", controllerAddr, 3*time.Second); err != nil {
		t.Skipf("controller not reachable at %s: %v", controllerAddr, err)
	}

	ctx, cancel := context.WithTimeout(context.Background(), 12*time.Minute)
	defer cancel()

	testID := fmt.Sprintf("gotest-%d", time.Now().UnixNano())

	// Attempt cleanup even if assertions fail after apply.
	applied := false
	defer func() {
		if !applied {
			return
		}
		_ = runCmd(ctx, repoRoot, destroyScript, testID)
	}()

	if err := runCmd(ctx, repoRoot, applyScript, testID); err != nil {
		t.Fatalf("apply failed: %v", err)
	}
	applied = true

	vmID, vmName, err := readTofuOutputs(ctx, repoRoot, projectDir)
	if err != nil {
		t.Fatalf("read tofu outputs: %v", err)
	}
	if vmID == "" {
		t.Fatal("vm_id output is empty after apply")
	}
	if !strings.Contains(vmName, testID) {
		t.Fatalf("vm_name %q does not contain test id %q", vmName, testID)
	}

	clientConn, client, err := newControllerClient(controllerAddr, repoRoot)
	if err != nil {
		t.Fatalf("connect controller: %v", err)
	}
	defer clientConn.Close()

	getCtx, getCancel := context.WithTimeout(ctx, 10*time.Second)
	defer getCancel()
	if _, err := client.GetVm(getCtx, &ctrlpb.GetVmRequest{VmId: vmID}); err != nil {
		t.Fatalf("GetVm after apply failed (vm_id=%s): %v", vmID, err)
	}

	if err := runCmd(ctx, repoRoot, destroyScript, testID); err != nil {
		t.Fatalf("destroy failed: %v", err)
	}

	verifyCtx, verifyCancel := context.WithTimeout(ctx, 10*time.Second)
	defer verifyCancel()
	_, err = client.GetVm(verifyCtx, &ctrlpb.GetVmRequest{VmId: vmID})
	if err == nil {
		t.Fatalf("expected VM %s to be gone after destroy, but GetVm succeeded", vmID)
	}
	code := status.Code(err)
	if code == codes.NotFound {
		return
	}
	// Current controller may wrap node not-found into Internal; accept this while
	// still requiring a clear "not found" signal in the message.
	if code == codes.Internal && strings.Contains(strings.ToLower(err.Error()), "not found") {
		return
	}
	if code != codes.NotFound {
		t.Fatalf("expected NotFound after destroy for VM %s, got: %v", vmID, err)
	}
}

func runCmd(ctx context.Context, repoRoot string, name string, args ...string) error {
	cmd := exec.CommandContext(ctx, name, args...)
	cmd.Dir = repoRoot
	out, err := cmd.CombinedOutput()
	if err != nil {
		return fmt.Errorf("command %s %s failed: %w\n%s", name, strings.Join(args, " "), err, string(out))
	}
	return nil
}

func readTofuOutputs(ctx context.Context, repoRoot, projectDir string) (vmID, vmName string, err error) {
	cmd := exec.CommandContext(
		ctx,
		filepath.Join(repoRoot, "nix_shell"),
		"tofu",
		"-chdir="+projectDir,
		"output",
		"-json",
	)
	cmd.Dir = repoRoot
	out, err := cmd.CombinedOutput()
	if err != nil {
		return "", "", fmt.Errorf("tofu output failed: %w\n%s", err, string(out))
	}

	jsonStart := strings.IndexByte(string(out), '{')
	if jsonStart < 0 {
		return "", "", fmt.Errorf("tofu output did not contain JSON payload:\n%s", string(out))
	}
	jsonBytes := out[jsonStart:]

	var raw map[string]tofuOutput
	if err := json.Unmarshal(jsonBytes, &raw); err != nil {
		return "", "", fmt.Errorf("parse tofu output json: %w", err)
	}

	idV, ok := raw["vm_id"]
	if !ok {
		return "", "", fmt.Errorf("missing vm_id output")
	}
	nameV, ok := raw["vm_name"]
	if !ok {
		return "", "", fmt.Errorf("missing vm_name output")
	}

	vmID, _ = idV.Value.(string)
	vmName, _ = nameV.Value.(string)
	return vmID, vmName, nil
}

func newControllerClient(controllerAddr, repoRoot string) (*grpc.ClientConn, ctrlpb.ControllerClient, error) {
	caPath := filepath.Join(repoRoot, "certs", "dev", "ca.crt")
	certPath := filepath.Join(repoRoot, "certs", "dev", "node.crt")
	keyPath := filepath.Join(repoRoot, "certs", "dev", "node.key")

	caPEM, err := os.ReadFile(caPath)
	if err != nil {
		return nil, nil, fmt.Errorf("read CA cert: %w", err)
	}
	cert, err := tls.LoadX509KeyPair(certPath, keyPath)
	if err != nil {
		return nil, nil, fmt.Errorf("load client cert/key: %w", err)
	}

	pool := x509.NewCertPool()
	if !pool.AppendCertsFromPEM(caPEM) {
		return nil, nil, fmt.Errorf("append CA cert failed")
	}

	creds := credentials.NewTLS(&tls.Config{
		Certificates:       []tls.Certificate{cert},
		RootCAs:            pool,
		InsecureSkipVerify: true,
		MinVersion:         tls.VersionTLS12,
	})

	conn, err := grpc.Dial(controllerAddr, grpc.WithTransportCredentials(creds))
	if err != nil {
		return nil, nil, err
	}
	return conn, ctrlpb.NewControllerClient(conn), nil
}
