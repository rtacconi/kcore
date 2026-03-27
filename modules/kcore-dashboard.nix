# NixOS module: kcore Leptos dashboard (gRPC client to controller; no SQLite).
{
  config,
  lib,
  pkgs,
  ...
}: let
  cfg = config.services.kcore-dashboard;
in {
  options.services.kcore-dashboard = {
    enable = lib.mkEnableOption "kcore web dashboard (reads controller over gRPC)";

    package = lib.mkOption {
      type = lib.types.package;
      description = "The `kcore-dashboard` package (e.g. `inputs.kcore.packages.\${pkgs.system}.kcore-dashboard`).";
    };

    listenAddress = lib.mkOption {
      type = lib.types.str;
      default = "0.0.0.0";
      description = "Address for the HTTP server (Leptos / Axum).";
    };

    port = lib.mkOption {
      type = lib.types.port;
      default = 8080;
      description = "HTTP port for the dashboard.";
    };

    openFirewall = lib.mkOption {
      type = lib.types.bool;
      default = false;
      description = "Open the dashboard port in the host firewall.";
    };

    environmentFile = lib.mkOption {
      type = lib.types.nullOr lib.types.path;
      default = null;
      description = ''
        Optional systemd EnvironmentFile (e.g. /etc/kcore/dashboard.env) with:
        - KCORE_CONTROLLER or CONTROLLER_ADDR (host:9090)
        - KCORE_TLS_DOMAIN (optional): cert verification name when the controller cert is for a hostname but you dial an IP (e.g. 127.0.0.1)
        - KCORE_CA_FILE, KCORE_CERT_FILE, KCORE_KEY_FILE (client cert CN must be kcore-kctl)
        - or KCORE_INSECURE=1 for plaintext (dev only)
      '';
    };
  };

  config = lib.mkIf cfg.enable {
    systemd.services.kcore-dashboard = {
      description = "kcore web dashboard";
      after = ["network-online.target"];
      wants = ["network-online.target"];
      wantedBy = ["multi-user.target"];
      environment = {
        LEPTOS_SITE_ADDR = "${cfg.listenAddress}:${toString cfg.port}";
        LEPTOS_ENV = "PROD";
      };
      serviceConfig =
        {
          Type = "simple";
          User = "root";
          ExecStart = lib.getExe cfg.package;
          Restart = "on-failure";
          RestartSec = "5s";
          LimitNOFILE = 65536;
        }
        // lib.optionalAttrs (cfg.environmentFile != null) {
          EnvironmentFile = cfg.environmentFile;
        };
    };

    networking.firewall.allowedTCPPorts = lib.optionals cfg.openFirewall [cfg.port];
  };
}
