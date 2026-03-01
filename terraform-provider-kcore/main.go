package main

import (
	"flag"

	"github.com/hashicorp/terraform-plugin-sdk/v2/plugin"
	"github.com/kcore/terraform-provider-kcore/internal/provider"
)

func main() {
	var debugMode bool

	flag.BoolVar(&debugMode, "debug", false, "set to true to run the provider with support for debuggers like delve")
	flag.Parse()

	opts := &plugin.ServeOpts{
		ProviderFunc: provider.New,
		Debug:        debugMode,
		ProviderAddr: "registry.terraform.io/kcore/kcore",
	}

	plugin.Serve(opts)
}
