package controlplane

import (
	"context"
	"sync"
	"time"

	ctrlpb "github.com/kcore/kcore/api/controller"
	cppb "github.com/kcore/kcore/api/controlplane"
	"github.com/kcore/kcore/pkg/controller"
	"google.golang.org/grpc/codes"
	"google.golang.org/grpc/status"
	"google.golang.org/protobuf/types/known/timestamppb"
)

// Service exposes a unified API surface and delegates existing orchestration
// methods to the current controller implementation.
type Service struct {
	cppb.UnimplementedControlPlaneServer

	legacy *controller.Server

	mu     sync.RWMutex
	tokens map[string]*cppb.EnrollmentTokenInfo
}

func NewService(legacy *controller.Server) *Service {
	return &Service{
		legacy: legacy,
		tokens: make(map[string]*cppb.EnrollmentTokenInfo),
	}
}

func (s *Service) RegisterNode(ctx context.Context, req *ctrlpb.RegisterNodeRequest) (*ctrlpb.RegisterNodeResponse, error) {
	return s.legacy.RegisterNode(ctx, req)
}

func (s *Service) Heartbeat(ctx context.Context, req *ctrlpb.HeartbeatRequest) (*ctrlpb.HeartbeatResponse, error) {
	return s.legacy.Heartbeat(ctx, req)
}

func (s *Service) SyncVmState(ctx context.Context, req *ctrlpb.SyncVmStateRequest) (*ctrlpb.SyncVmStateResponse, error) {
	return s.legacy.SyncVmState(ctx, req)
}

func (s *Service) CreateVm(ctx context.Context, req *ctrlpb.CreateVmRequest) (*ctrlpb.CreateVmResponse, error) {
	return s.legacy.CreateVm(ctx, req)
}

func (s *Service) DeleteVm(ctx context.Context, req *ctrlpb.DeleteVmRequest) (*ctrlpb.DeleteVmResponse, error) {
	return s.legacy.DeleteVm(ctx, req)
}

func (s *Service) StartVm(ctx context.Context, req *ctrlpb.StartVmRequest) (*ctrlpb.StartVmResponse, error) {
	return s.legacy.StartVm(ctx, req)
}

func (s *Service) StopVm(ctx context.Context, req *ctrlpb.StopVmRequest) (*ctrlpb.StopVmResponse, error) {
	return s.legacy.StopVm(ctx, req)
}

func (s *Service) GetVm(ctx context.Context, req *ctrlpb.GetVmRequest) (*ctrlpb.GetVmResponse, error) {
	return s.legacy.GetVm(ctx, req)
}

func (s *Service) ListVms(ctx context.Context, req *ctrlpb.ListVmsRequest) (*ctrlpb.ListVmsResponse, error) {
	return s.legacy.ListVms(ctx, req)
}

func (s *Service) ListNodes(ctx context.Context, req *ctrlpb.ListNodesRequest) (*ctrlpb.ListNodesResponse, error) {
	return s.legacy.ListNodes(ctx, req)
}

func (s *Service) GetNode(ctx context.Context, req *ctrlpb.GetNodeRequest) (*ctrlpb.GetNodeResponse, error) {
	return s.legacy.GetNode(ctx, req)
}

func (s *Service) ApplyControllerNixConfig(ctx context.Context, req *ctrlpb.ApplyNixConfigRequest) (*ctrlpb.ApplyNixConfigResponse, error) {
	return s.legacy.ApplyNixConfig(ctx, req)
}

func (s *Service) ApplyNodeNixConfig(context.Context, *cppb.ApplyNodeNixConfigRequest) (*ctrlpb.ApplyNixConfigResponse, error) {
	return nil, status.Error(codes.Unimplemented, "ApplyNodeNixConfig is not implemented yet")
}

func (s *Service) CreateEnrollmentToken(_ context.Context, req *cppb.CreateEnrollmentTokenRequest) (*cppb.CreateEnrollmentTokenResponse, error) {
	id := "tok-" + time.Now().UTC().Format("20060102150405.000000000")
	secret := "secret-" + id

	info := &cppb.EnrollmentTokenInfo{
		TokenId:       id,
		Scope:         req.Scope,
		AllowedLabels: req.AllowedLabels,
		ExpiresAt:     req.ExpiresAt,
		Description:   req.Description,
		Revoked:       false,
	}
	if info.ExpiresAt == nil {
		info.ExpiresAt = timestamppb.New(time.Now().UTC().Add(24 * time.Hour))
	}

	s.mu.Lock()
	s.tokens[id] = info
	s.mu.Unlock()

	return &cppb.CreateEnrollmentTokenResponse{
		TokenId:     id,
		TokenSecret: secret,
	}, nil
}

func (s *Service) RevokeEnrollmentToken(_ context.Context, req *cppb.RevokeEnrollmentTokenRequest) (*cppb.RevokeEnrollmentTokenResponse, error) {
	s.mu.Lock()
	defer s.mu.Unlock()

	token, ok := s.tokens[req.TokenId]
	if !ok {
		return nil, status.Errorf(codes.NotFound, "token not found: %s", req.TokenId)
	}
	token.Revoked = true
	return &cppb.RevokeEnrollmentTokenResponse{Success: true}, nil
}

func (s *Service) ListEnrollmentTokens(context.Context, *cppb.ListEnrollmentTokensRequest) (*cppb.ListEnrollmentTokensResponse, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()

	resp := &cppb.ListEnrollmentTokensResponse{
		Tokens: make([]*cppb.EnrollmentTokenInfo, 0, len(s.tokens)),
	}
	for _, tok := range s.tokens {
		resp.Tokens = append(resp.Tokens, tok)
	}
	return resp, nil
}

func (s *Service) GetBootstrapConfig(context.Context, *cppb.GetBootstrapConfigRequest) (*cppb.GetBootstrapConfigResponse, error) {
	return nil, status.Error(codes.Unimplemented, "GetBootstrapConfig is not implemented yet")
}

func (s *Service) EnrollNode(context.Context, *cppb.EnrollNodeRequest) (*cppb.EnrollNodeResponse, error) {
	return nil, status.Error(codes.Unimplemented, "EnrollNode is not implemented yet")
}

func (s *Service) RotateNodeCertificate(context.Context, *cppb.RotateNodeCertificateRequest) (*cppb.RotateNodeCertificateResponse, error) {
	return nil, status.Error(codes.Unimplemented, "RotateNodeCertificate is not implemented yet")
}

func (s *Service) ReportInstallStatus(context.Context, *cppb.ReportInstallStatusRequest) (*cppb.ReportInstallStatusResponse, error) {
	return nil, status.Error(codes.Unimplemented, "ReportInstallStatus is not implemented yet")
}

func (s *Service) GetInstallStatus(context.Context, *cppb.GetInstallStatusRequest) (*cppb.GetInstallStatusResponse, error) {
	return nil, status.Error(codes.Unimplemented, "GetInstallStatus is not implemented yet")
}

func (s *Service) ListInstallStatus(context.Context, *cppb.ListInstallStatusRequest) (*cppb.ListInstallStatusResponse, error) {
	return nil, status.Error(codes.Unimplemented, "ListInstallStatus is not implemented yet")
}
