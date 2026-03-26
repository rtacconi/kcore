use serde::Deserialize;

/// Subset of Cloud Hypervisor's `GET /api/v1/vm.info` response.
#[derive(Debug, Deserialize)]
pub struct VmInfo {
    pub state: String,
    pub config: VmConfig,
    #[serde(default)]
    #[allow(dead_code)]
    pub memory_actual_size: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct VmConfig {
    pub cpus: Option<CpuConfig>,
    pub memory: Option<MemoryConfig>,
    #[serde(default)]
    pub net: Vec<NetConfig>,
}

#[derive(Debug, Deserialize)]
pub struct CpuConfig {
    pub boot_vcpus: u32,
}

#[derive(Debug, Deserialize)]
pub struct MemoryConfig {
    /// Memory size in bytes.
    pub size: u64,
}

#[derive(Debug, Deserialize)]
pub struct NetConfig {
    #[serde(default)]
    pub mac: Option<String>,
}
