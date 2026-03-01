package controlplane

import (
	"context"
	"testing"

	cppb "github.com/kcore/kcore/api/controlplane"
	"github.com/kcore/kcore/pkg/controller"
)

func TestEnrollmentTokenLifecycle(t *testing.T) {
	svc := NewService(controller.NewServer())
	ctx := context.Background()

	createResp, err := svc.CreateEnrollmentToken(ctx, &cppb.CreateEnrollmentTokenRequest{
		Scope:       cppb.EnrollmentTokenScope_ENROLLMENT_TOKEN_SCOPE_NODE_BOOTSTRAP,
		Description: "test token",
	})
	if err != nil {
		t.Fatalf("CreateEnrollmentToken failed: %v", err)
	}
	if createResp.TokenId == "" {
		t.Fatal("expected token_id to be set")
	}
	if createResp.TokenSecret == "" {
		t.Fatal("expected token_secret to be set")
	}

	listResp, err := svc.ListEnrollmentTokens(ctx, &cppb.ListEnrollmentTokensRequest{})
	if err != nil {
		t.Fatalf("ListEnrollmentTokens failed: %v", err)
	}
	if len(listResp.Tokens) != 1 {
		t.Fatalf("expected 1 token, got %d", len(listResp.Tokens))
	}

	_, err = svc.RevokeEnrollmentToken(ctx, &cppb.RevokeEnrollmentTokenRequest{
		TokenId: createResp.TokenId,
	})
	if err != nil {
		t.Fatalf("RevokeEnrollmentToken failed: %v", err)
	}

	listResp, err = svc.ListEnrollmentTokens(ctx, &cppb.ListEnrollmentTokensRequest{})
	if err != nil {
		t.Fatalf("ListEnrollmentTokens after revoke failed: %v", err)
	}
	if len(listResp.Tokens) != 1 || !listResp.Tokens[0].Revoked {
		t.Fatalf("expected revoked token in list, got: %+v", listResp.Tokens)
	}
}
