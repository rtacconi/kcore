use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::config::{StorageBackendKind, StorageConfig};
use crate::proto;

#[derive(Debug, Clone, Copy)]
pub enum ErrorKind {
    InvalidArgument,
    NotFound,
    AlreadyExists,
    FailedPrecondition,
    Unimplemented,
    Internal,
}

#[derive(Debug)]
pub struct StorageError {
    kind: ErrorKind,
    message: String,
}

impl StorageError {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub fn kind(&self) -> ErrorKind {
        self.kind
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl std::fmt::Display for StorageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for StorageError {}

impl From<StorageError> for tonic::Status {
    fn from(value: StorageError) -> Self {
        match value.kind {
            ErrorKind::InvalidArgument => tonic::Status::invalid_argument(value.message),
            ErrorKind::NotFound => tonic::Status::not_found(value.message),
            ErrorKind::AlreadyExists => tonic::Status::already_exists(value.message),
            ErrorKind::FailedPrecondition => tonic::Status::failed_precondition(value.message),
            ErrorKind::Unimplemented => tonic::Status::unimplemented(value.message),
            ErrorKind::Internal => tonic::Status::internal(value.message),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CreateVolumeRequest {
    pub volume_id: String,
    pub storage_class: String,
    pub size_bytes: i64,
    pub parameters: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct AttachVolumeRequest {
    pub backend_handle: String,
    pub vm_id: String,
    pub target_device: String,
    pub bus: String,
}

#[derive(Debug, Clone)]
pub struct DetachVolumeRequest {
    pub backend_handle: String,
    pub vm_id: String,
}

#[derive(Debug, Clone)]
pub struct EnsureImageRequest {
    pub image_url: String,
    pub image_sha256: String,
    pub destination_path: String,
}

#[derive(Debug, Clone)]
pub struct EnsureImageResult {
    pub path: String,
    pub size_bytes: i64,
    pub cached: bool,
    pub downloaded: bool,
}

#[derive(Debug, Clone)]
pub struct UploadImageRequest {
    pub image_bytes: Vec<u8>,
    pub source_name: String,
    pub destination_name: String,
    pub image_format: String,
    pub image_sha256: String,
}

#[derive(Debug, Clone)]
pub struct UploadImageResult {
    pub path: String,
    pub size_bytes: i64,
    pub image_format: String,
    pub image_sha256: String,
}

#[derive(Debug, Clone)]
pub struct UploadImageFromPathRequest {
    pub source_file_path: String,
    pub source_name: String,
    pub destination_name: String,
    pub image_format: String,
    pub image_sha256: String,
}

pub trait StorageAdapter: Send + Sync {
    fn create_volume(&self, req: CreateVolumeRequest) -> Result<String, StorageError>;
    fn delete_volume(&self, backend_handle: &str) -> Result<(), StorageError>;
    fn attach_volume(&self, req: AttachVolumeRequest) -> Result<(), StorageError>;
    fn detach_volume(&self, req: DetachVolumeRequest) -> Result<(), StorageError>;
    fn ensure_image(&self, req: EnsureImageRequest) -> Result<EnsureImageResult, StorageError>;
    fn upload_image(&self, req: UploadImageRequest) -> Result<UploadImageResult, StorageError>;
    fn upload_image_from_path(
        &self,
        req: UploadImageFromPathRequest,
    ) -> Result<UploadImageResult, StorageError>;
}

pub fn from_config(cfg: &StorageConfig) -> Result<Arc<dyn StorageAdapter>, StorageError> {
    let adapter: Arc<dyn StorageAdapter> = match cfg.backend {
        StorageBackendKind::Filesystem => Arc::new(FilesystemAdapter::new(
            cfg.filesystem_volume_dir.clone(),
            cfg.image_cache_dir.clone(),
        )),
        StorageBackendKind::Lvm => Arc::new(LvmAdapter::new(cfg)?),
        StorageBackendKind::Zfs => Arc::new(ZfsAdapter::new(cfg)?),
    };
    Ok(adapter)
}

pub fn default_adapter() -> Arc<dyn StorageAdapter> {
    Arc::new(FilesystemAdapter::new(
        "/var/lib/kcore/volumes".to_string(),
        "/var/lib/kcore/images".to_string(),
    ))
}

#[derive(Debug, Clone)]
struct FilesystemAdapter {
    volume_dir: PathBuf,
    image_cache_dir: PathBuf,
}

impl FilesystemAdapter {
    fn new(volume_dir: String, image_cache_dir: String) -> Self {
        Self {
            volume_dir: PathBuf::from(volume_dir),
            image_cache_dir: PathBuf::from(image_cache_dir),
        }
    }
}

impl StorageAdapter for FilesystemAdapter {
    fn create_volume(&self, req: CreateVolumeRequest) -> Result<String, StorageError> {
        validate_volume_create_inputs(&req)?;
        std::fs::create_dir_all(&self.volume_dir).map_err(|e| {
            StorageError::new(
                ErrorKind::Internal,
                format!("creating {}: {e}", self.volume_dir.display()),
            )
        })?;
        let volume_name = sanitize_volume_name(&req.volume_id);
        let backend_handle = self.volume_dir.join(format!("{volume_name}.raw"));
        let file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&backend_handle)
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::AlreadyExists {
                    StorageError::new(
                        ErrorKind::AlreadyExists,
                        format!("volume {} already exists", backend_handle.display()),
                    )
                } else {
                    StorageError::new(
                        ErrorKind::Internal,
                        format!("creating {}: {e}", backend_handle.display()),
                    )
                }
            })?;
        file.set_len(req.size_bytes as u64).map_err(|e| {
            StorageError::new(
                ErrorKind::Internal,
                format!("sizing {}: {e}", backend_handle.display()),
            )
        })?;
        Ok(backend_handle.display().to_string())
    }

    fn delete_volume(&self, backend_handle: &str) -> Result<(), StorageError> {
        let path = PathBuf::from(backend_handle.trim());
        if !path.starts_with(&self.volume_dir) {
            return Err(StorageError::new(
                ErrorKind::InvalidArgument,
                format!("backend_handle must be under {}", self.volume_dir.display()),
            ));
        }
        std::fs::remove_file(&path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                StorageError::new(
                    ErrorKind::NotFound,
                    format!("volume {} not found", path.display()),
                )
            } else {
                StorageError::new(
                    ErrorKind::Internal,
                    format!("deleting {}: {e}", path.display()),
                )
            }
        })?;
        Ok(())
    }

    fn attach_volume(&self, req: AttachVolumeRequest) -> Result<(), StorageError> {
        validate_attach_request(&req)?;
        Ok(())
    }

    fn detach_volume(&self, req: DetachVolumeRequest) -> Result<(), StorageError> {
        validate_detach_request(&req)?;
        Ok(())
    }

    fn ensure_image(&self, req: EnsureImageRequest) -> Result<EnsureImageResult, StorageError> {
        ensure_image_cached(&self.image_cache_dir, req)
    }

    fn upload_image(&self, req: UploadImageRequest) -> Result<UploadImageResult, StorageError> {
        upload_image_to_cache(&self.image_cache_dir, req)
    }

    fn upload_image_from_path(
        &self,
        req: UploadImageFromPathRequest,
    ) -> Result<UploadImageResult, StorageError> {
        upload_image_file_to_cache(&self.image_cache_dir, req)
    }
}

#[derive(Debug, Clone)]
struct LvmAdapter {
    vg_name: String,
    lv_prefix: String,
    image_cache_dir: PathBuf,
}

impl LvmAdapter {
    fn new(cfg: &StorageConfig) -> Result<Self, StorageError> {
        let lvm = cfg.lvm.as_ref().ok_or_else(|| {
            StorageError::new(
                ErrorKind::InvalidArgument,
                "storage.lvm config is required when storage.backend=lvm",
            )
        })?;
        Ok(Self {
            vg_name: lvm.vg_name.clone(),
            lv_prefix: lvm.lv_prefix.clone(),
            image_cache_dir: PathBuf::from(cfg.image_cache_dir.clone()),
        })
    }
}

impl StorageAdapter for LvmAdapter {
    fn create_volume(&self, req: CreateVolumeRequest) -> Result<String, StorageError> {
        validate_volume_create_inputs(&req)?;
        let lv_name = format!("{}{}", self.lv_prefix, sanitize_volume_name(&req.volume_id));
        run_cmd(
            "lvcreate",
            &[
                "-L",
                &format!("{}B", req.size_bytes),
                "-n",
                &lv_name,
                &self.vg_name,
            ],
            ErrorKind::Internal,
        )?;
        Ok(format!("/dev/{}/{}", self.vg_name, lv_name))
    }

    fn delete_volume(&self, backend_handle: &str) -> Result<(), StorageError> {
        if backend_handle.trim().is_empty() {
            return Err(StorageError::new(
                ErrorKind::InvalidArgument,
                "backend_handle is required",
            ));
        }
        run_cmd("lvremove", &["-f", backend_handle], ErrorKind::Internal)?;
        Ok(())
    }

    fn attach_volume(&self, req: AttachVolumeRequest) -> Result<(), StorageError> {
        validate_attach_request(&req)?;
        Ok(())
    }

    fn detach_volume(&self, req: DetachVolumeRequest) -> Result<(), StorageError> {
        validate_detach_request(&req)?;
        Ok(())
    }

    fn ensure_image(&self, req: EnsureImageRequest) -> Result<EnsureImageResult, StorageError> {
        ensure_image_cached(&self.image_cache_dir, req)
    }

    fn upload_image(&self, req: UploadImageRequest) -> Result<UploadImageResult, StorageError> {
        upload_image_to_cache(&self.image_cache_dir, req)
    }

    fn upload_image_from_path(
        &self,
        req: UploadImageFromPathRequest,
    ) -> Result<UploadImageResult, StorageError> {
        upload_image_file_to_cache(&self.image_cache_dir, req)
    }
}

#[derive(Debug, Clone)]
struct ZfsAdapter {
    pool_name: String,
    dataset_prefix: String,
    image_cache_dir: PathBuf,
}

impl ZfsAdapter {
    fn new(cfg: &StorageConfig) -> Result<Self, StorageError> {
        let zfs = cfg.zfs.as_ref().ok_or_else(|| {
            StorageError::new(
                ErrorKind::InvalidArgument,
                "storage.zfs config is required when storage.backend=zfs",
            )
        })?;
        Ok(Self {
            pool_name: zfs.pool_name.clone(),
            dataset_prefix: zfs.dataset_prefix.clone(),
            image_cache_dir: PathBuf::from(cfg.image_cache_dir.clone()),
        })
    }
}

impl StorageAdapter for ZfsAdapter {
    fn create_volume(&self, req: CreateVolumeRequest) -> Result<String, StorageError> {
        validate_volume_create_inputs(&req)?;
        let zvol = format!(
            "{}/{}{}",
            self.pool_name,
            self.dataset_prefix,
            sanitize_volume_name(&req.volume_id)
        );
        run_cmd(
            "zfs",
            &[
                "create",
                "-V",
                &req.size_bytes.to_string(),
                "-o",
                "volmode=dev",
                &zvol,
            ],
            ErrorKind::Internal,
        )?;
        Ok(format!("/dev/zvol/{zvol}"))
    }

    fn delete_volume(&self, backend_handle: &str) -> Result<(), StorageError> {
        let target = backend_handle.trim();
        if target.is_empty() {
            return Err(StorageError::new(
                ErrorKind::InvalidArgument,
                "backend_handle is required",
            ));
        }
        let dataset = target.strip_prefix("/dev/zvol/").unwrap_or(target);
        run_cmd("zfs", &["destroy", "-f", dataset], ErrorKind::Internal)?;
        Ok(())
    }

    fn attach_volume(&self, req: AttachVolumeRequest) -> Result<(), StorageError> {
        validate_attach_request(&req)?;
        Ok(())
    }

    fn detach_volume(&self, req: DetachVolumeRequest) -> Result<(), StorageError> {
        validate_detach_request(&req)?;
        Ok(())
    }

    fn ensure_image(&self, req: EnsureImageRequest) -> Result<EnsureImageResult, StorageError> {
        ensure_image_cached(&self.image_cache_dir, req)
    }

    fn upload_image(&self, req: UploadImageRequest) -> Result<UploadImageResult, StorageError> {
        upload_image_to_cache(&self.image_cache_dir, req)
    }

    fn upload_image_from_path(
        &self,
        req: UploadImageFromPathRequest,
    ) -> Result<UploadImageResult, StorageError> {
        upload_image_file_to_cache(&self.image_cache_dir, req)
    }
}

fn run_cmd(program: &str, args: &[&str], err_kind: ErrorKind) -> Result<(), StorageError> {
    let out = Command::new(program)
        .args(args)
        .output()
        .map_err(|e| StorageError::new(err_kind, format!("running {program}: {e}")))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(StorageError::new(
            err_kind,
            format!("{program} failed: {}", stderr.trim()),
        ));
    }
    Ok(())
}

fn validate_volume_create_inputs(req: &CreateVolumeRequest) -> Result<(), StorageError> {
    if req.volume_id.trim().is_empty() {
        return Err(StorageError::new(
            ErrorKind::InvalidArgument,
            "volume_id is required",
        ));
    }
    if req.size_bytes <= 0 {
        return Err(StorageError::new(
            ErrorKind::InvalidArgument,
            "size_bytes must be > 0",
        ));
    }
    // Keep parity with current API shape even before these fields
    // are fully interpreted by all adapters.
    let _storage_class = req.storage_class.trim();
    let _parameter_count = req.parameters.len();
    Ok(())
}

fn validate_attach_request(req: &AttachVolumeRequest) -> Result<(), StorageError> {
    if req.backend_handle.trim().is_empty() {
        return Err(StorageError::new(
            ErrorKind::InvalidArgument,
            "backend_handle is required",
        ));
    }
    if req.vm_id.trim().is_empty() {
        return Err(StorageError::new(
            ErrorKind::InvalidArgument,
            "vm_id is required",
        ));
    }
    if req.target_device.trim().is_empty() {
        return Err(StorageError::new(
            ErrorKind::InvalidArgument,
            "target_device is required",
        ));
    }
    if req.bus.trim().is_empty() {
        return Err(StorageError::new(
            ErrorKind::InvalidArgument,
            "bus is required",
        ));
    }
    Ok(())
}

fn validate_detach_request(req: &DetachVolumeRequest) -> Result<(), StorageError> {
    if req.backend_handle.trim().is_empty() {
        return Err(StorageError::new(
            ErrorKind::InvalidArgument,
            "backend_handle is required",
        ));
    }
    if req.vm_id.trim().is_empty() {
        return Err(StorageError::new(
            ErrorKind::InvalidArgument,
            "vm_id is required",
        ));
    }
    Ok(())
}

fn validate_image_sha256(sha: &str) -> Result<String, StorageError> {
    let normalized = sha.trim().to_ascii_lowercase();
    if normalized.len() != 64 || !normalized.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(StorageError::new(
            ErrorKind::InvalidArgument,
            "image_sha256 must be exactly 64 hexadecimal characters",
        ));
    }
    Ok(normalized)
}

fn validate_image_url(url: &str) -> Result<String, StorageError> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Err(StorageError::new(
            ErrorKind::InvalidArgument,
            "image_url is required",
        ));
    }
    if !trimmed.starts_with("https://") {
        return Err(StorageError::new(
            ErrorKind::InvalidArgument,
            "image_url must use https:// scheme",
        ));
    }
    Ok(trimmed.to_string())
}

fn validate_destination_path(path: &str, image_cache_dir: &Path) -> Result<PathBuf, StorageError> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err(StorageError::new(
            ErrorKind::InvalidArgument,
            "destination_path is required",
        ));
    }
    let p = PathBuf::from(trimmed);
    if !p.is_absolute() {
        return Err(StorageError::new(
            ErrorKind::InvalidArgument,
            "destination_path must be an absolute path",
        ));
    }
    if !p.starts_with(image_cache_dir) {
        return Err(StorageError::new(
            ErrorKind::InvalidArgument,
            format!(
                "destination_path must be under {}",
                image_cache_dir.display()
            ),
        ));
    }
    if trimmed.contains("..") {
        return Err(StorageError::new(
            ErrorKind::InvalidArgument,
            "destination_path must not contain path traversal",
        ));
    }
    Ok(p)
}

fn sha256sum_file(path: &Path) -> Result<String, StorageError> {
    let out = Command::new("sha256sum").arg(path).output().map_err(|e| {
        StorageError::new(
            ErrorKind::Internal,
            format!("running sha256sum on {}: {e}", path.display()),
        )
    })?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(StorageError::new(
            ErrorKind::Internal,
            format!("sha256sum failed for {}: {}", path.display(), stderr.trim()),
        ));
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    let digest = stdout
        .split_whitespace()
        .next()
        .ok_or_else(|| StorageError::new(ErrorKind::Internal, "invalid sha256sum output"))?;
    Ok(digest.to_ascii_lowercase())
}

fn ensure_image_cached(
    image_cache_dir: &Path,
    req: EnsureImageRequest,
) -> Result<EnsureImageResult, StorageError> {
    let image_url = validate_image_url(&req.image_url)?;
    let image_sha256 = validate_image_sha256(&req.image_sha256)?;
    let destination = validate_destination_path(&req.destination_path, image_cache_dir)?;

    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            StorageError::new(
                ErrorKind::Internal,
                format!("creating {}: {e}", parent.display()),
            )
        })?;
    }

    if destination.exists() {
        let existing_sha = sha256sum_file(&destination)?;
        if existing_sha == image_sha256 {
            let size_bytes = std::fs::metadata(&destination)
                .map_err(|e| {
                    StorageError::new(
                        ErrorKind::Internal,
                        format!("stat {}: {e}", destination.display()),
                    )
                })?
                .len() as i64;
            return Ok(EnsureImageResult {
                path: destination.display().to_string(),
                size_bytes,
                cached: true,
                downloaded: false,
            });
        }
        std::fs::remove_file(&destination).map_err(|e| {
            StorageError::new(
                ErrorKind::Internal,
                format!("removing {}: {e}", destination.display()),
            )
        })?;
    }

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| StorageError::new(ErrorKind::Internal, format!("system clock error: {e}")))?
        .as_millis();
    let tmp_path = PathBuf::from(format!("{}.part-{timestamp}", destination.display()));

    let status = Command::new("curl")
        .args([
            "--fail",
            "--location",
            "--silent",
            "--show-error",
            "--output",
            tmp_path.to_string_lossy().as_ref(),
            image_url.as_str(),
        ])
        .status()
        .map_err(|e| StorageError::new(ErrorKind::Internal, format!("starting curl: {e}")))?;
    if !status.success() {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(StorageError::new(
            ErrorKind::Internal,
            format!("curl download failed for {image_url}"),
        ));
    }

    let downloaded_sha = sha256sum_file(&tmp_path)?;
    if downloaded_sha != image_sha256 {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(StorageError::new(
            ErrorKind::FailedPrecondition,
            format!(
                "sha256 mismatch for {} (expected {}, got {})",
                image_url, image_sha256, downloaded_sha
            ),
        ));
    }

    std::fs::rename(&tmp_path, &destination).map_err(|e| {
        StorageError::new(
            ErrorKind::Internal,
            format!(
                "moving {} to {}: {e}",
                tmp_path.display(),
                destination.display()
            ),
        )
    })?;

    let size_bytes = std::fs::metadata(&destination)
        .map_err(|e| {
            StorageError::new(
                ErrorKind::Internal,
                format!("stat {}: {e}", destination.display()),
            )
        })?
        .len() as i64;
    Ok(EnsureImageResult {
        path: destination.display().to_string(),
        size_bytes,
        cached: false,
        downloaded: true,
    })
}

fn sanitize_volume_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    if cleaned.is_empty() {
        "volume".to_string()
    } else {
        cleaned
    }
}

pub fn ensure_image_response(result: EnsureImageResult) -> proto::EnsureImageResponse {
    proto::EnsureImageResponse {
        path: result.path,
        size_bytes: result.size_bytes,
        cached: result.cached,
        downloaded: result.downloaded,
    }
}

pub fn upload_image_response(result: UploadImageResult) -> proto::UploadImageResponse {
    proto::UploadImageResponse {
        path: result.path,
        size_bytes: result.size_bytes,
        image_format: result.image_format,
        image_sha256: result.image_sha256,
    }
}

fn validate_image_format(format: &str) -> Result<String, StorageError> {
    let normalized = format.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "qcow2" | "raw" => Ok(normalized),
        _ => Err(StorageError::new(
            ErrorKind::InvalidArgument,
            "image_format must be 'qcow2' or 'raw'",
        )),
    }
}

fn validate_expected_sha256(sha: &str) -> Result<Option<String>, StorageError> {
    let trimmed = sha.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let normalized = validate_image_sha256(trimmed)?;
    Ok(Some(normalized))
}

fn sanitize_image_name(name: &str, format: &str) -> String {
    let src = name.trim();
    let raw_name = if src.is_empty() {
        format!("uploaded-image.{format}")
    } else {
        src.to_string()
    };
    let mut cleaned: String = raw_name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    if cleaned.is_empty() {
        cleaned = format!("uploaded-image.{format}");
    }
    if !cleaned
        .to_ascii_lowercase()
        .ends_with(&format!(".{format}"))
    {
        cleaned.push('.');
        cleaned.push_str(format);
    }
    cleaned
}

fn detect_qcow2_magic(data: &[u8]) -> bool {
    data.len() >= 4 && data[0] == b'Q' && data[1] == b'F' && data[2] == b'I' && data[3] == 0xfb
}

fn detect_qcow2_magic_from_file(path: &Path) -> Result<bool, StorageError> {
    let mut f = std::fs::File::open(path).map_err(|e| {
        StorageError::new(
            ErrorKind::Internal,
            format!("opening {}: {e}", path.display()),
        )
    })?;
    let mut magic = [0u8; 4];
    let read = f.read(&mut magic).map_err(|e| {
        StorageError::new(
            ErrorKind::Internal,
            format!("reading {}: {e}", path.display()),
        )
    })?;
    Ok(read == 4 && detect_qcow2_magic(&magic))
}

fn upload_image_to_cache(
    image_cache_dir: &Path,
    req: UploadImageRequest,
) -> Result<UploadImageResult, StorageError> {
    let format = validate_image_format(&req.image_format)?;
    let expected_sha = validate_expected_sha256(&req.image_sha256)?;
    if req.image_bytes.is_empty() {
        return Err(StorageError::new(
            ErrorKind::InvalidArgument,
            "image_bytes is required",
        ));
    }

    // qcow2 has a stable magic header, raw does not.
    if format == "qcow2" && !detect_qcow2_magic(&req.image_bytes) {
        return Err(StorageError::new(
            ErrorKind::FailedPrecondition,
            "uploaded image does not look like qcow2 (missing QFI magic)",
        ));
    }

    std::fs::create_dir_all(image_cache_dir).map_err(|e| {
        StorageError::new(
            ErrorKind::Internal,
            format!("creating {}: {e}", image_cache_dir.display()),
        )
    })?;

    let base_name = if req.destination_name.trim().is_empty() {
        sanitize_image_name(&req.source_name, &format)
    } else {
        sanitize_image_name(&req.destination_name, &format)
    };
    let destination = image_cache_dir.join(base_name);
    if destination.exists() {
        return Err(StorageError::new(
            ErrorKind::AlreadyExists,
            format!("destination already exists: {}", destination.display()),
        ));
    }

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| StorageError::new(ErrorKind::Internal, format!("system clock error: {e}")))?
        .as_millis();
    let tmp_path = PathBuf::from(format!("{}.upload-{timestamp}.part", destination.display()));
    std::fs::write(&tmp_path, &req.image_bytes).map_err(|e| {
        StorageError::new(
            ErrorKind::Internal,
            format!("writing {}: {e}", tmp_path.display()),
        )
    })?;

    let computed_sha = sha256sum_file(&tmp_path)?;
    if let Some(expected) = expected_sha {
        if expected != computed_sha {
            let _ = std::fs::remove_file(&tmp_path);
            return Err(StorageError::new(
                ErrorKind::FailedPrecondition,
                format!(
                    "sha256 mismatch for upload (expected {}, got {})",
                    expected, computed_sha
                ),
            ));
        }
    }

    std::fs::rename(&tmp_path, &destination).map_err(|e| {
        StorageError::new(
            ErrorKind::Internal,
            format!(
                "moving {} to {}: {e}",
                tmp_path.display(),
                destination.display()
            ),
        )
    })?;

    let size_bytes = std::fs::metadata(&destination)
        .map_err(|e| {
            StorageError::new(
                ErrorKind::Internal,
                format!("stat {}: {e}", destination.display()),
            )
        })?
        .len() as i64;

    Ok(UploadImageResult {
        path: destination.display().to_string(),
        size_bytes,
        image_format: format,
        image_sha256: computed_sha,
    })
}

fn upload_image_file_to_cache(
    image_cache_dir: &Path,
    req: UploadImageFromPathRequest,
) -> Result<UploadImageResult, StorageError> {
    let format = validate_image_format(&req.image_format)?;
    let expected_sha = validate_expected_sha256(&req.image_sha256)?;
    let source = PathBuf::from(req.source_file_path.trim());
    if !source.is_absolute() {
        return Err(StorageError::new(
            ErrorKind::InvalidArgument,
            "source_file_path must be absolute",
        ));
    }
    if !source.exists() {
        return Err(StorageError::new(
            ErrorKind::NotFound,
            format!("source file not found: {}", source.display()),
        ));
    }
    let source_meta = std::fs::metadata(&source).map_err(|e| {
        StorageError::new(
            ErrorKind::Internal,
            format!("stat {}: {e}", source.display()),
        )
    })?;
    if source_meta.len() == 0 {
        return Err(StorageError::new(
            ErrorKind::InvalidArgument,
            "source file is empty",
        ));
    }
    if format == "qcow2" && !detect_qcow2_magic_from_file(&source)? {
        return Err(StorageError::new(
            ErrorKind::FailedPrecondition,
            "uploaded image does not look like qcow2 (missing QFI magic)",
        ));
    }

    std::fs::create_dir_all(image_cache_dir).map_err(|e| {
        StorageError::new(
            ErrorKind::Internal,
            format!("creating {}: {e}", image_cache_dir.display()),
        )
    })?;
    let base_name = if req.destination_name.trim().is_empty() {
        sanitize_image_name(&req.source_name, &format)
    } else {
        sanitize_image_name(&req.destination_name, &format)
    };
    let destination = image_cache_dir.join(base_name);
    if destination.exists() {
        return Err(StorageError::new(
            ErrorKind::AlreadyExists,
            format!("destination already exists: {}", destination.display()),
        ));
    }
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| StorageError::new(ErrorKind::Internal, format!("system clock error: {e}")))?
        .as_millis();
    let tmp_path = PathBuf::from(format!("{}.upload-{timestamp}.part", destination.display()));
    std::fs::copy(&source, &tmp_path).map_err(|e| {
        StorageError::new(
            ErrorKind::Internal,
            format!(
                "copying {} to {}: {e}",
                source.display(),
                tmp_path.display()
            ),
        )
    })?;
    let computed_sha = sha256sum_file(&tmp_path)?;
    if let Some(expected) = expected_sha {
        if expected != computed_sha {
            let _ = std::fs::remove_file(&tmp_path);
            return Err(StorageError::new(
                ErrorKind::FailedPrecondition,
                format!(
                    "sha256 mismatch for upload (expected {}, got {})",
                    expected, computed_sha
                ),
            ));
        }
    }
    std::fs::rename(&tmp_path, &destination).map_err(|e| {
        StorageError::new(
            ErrorKind::Internal,
            format!(
                "moving {} to {}: {e}",
                tmp_path.display(),
                destination.display()
            ),
        )
    })?;
    let size_bytes = std::fs::metadata(&destination)
        .map_err(|e| {
            StorageError::new(
                ErrorKind::Internal,
                format!("stat {}: {e}", destination.display()),
            )
        })?
        .len() as i64;
    Ok(UploadImageResult {
        path: destination.display().to_string(),
        size_bytes,
        image_format: format,
        image_sha256: computed_sha,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_filesystem_adapter_creates_volume_file() {
        let temp = tempfile::tempdir().expect("tempdir");
        let image_dir = temp.path().join("images");
        let volume_dir = temp.path().join("volumes");
        let adapter = FilesystemAdapter::new(
            volume_dir.display().to_string(),
            image_dir.display().to_string(),
        );
        let handle = adapter
            .create_volume(CreateVolumeRequest {
                volume_id: "vm-1-root".to_string(),
                storage_class: "filesystem".to_string(),
                size_bytes: 1024 * 1024,
                parameters: HashMap::new(),
            })
            .expect("create volume");
        let meta = std::fs::metadata(&handle).expect("volume exists");
        assert_eq!(meta.len(), 1024 * 1024);
    }

    #[test]
    fn destination_path_must_stay_under_cache_dir() {
        let cache = PathBuf::from("/var/lib/kcore/images");
        let err = validate_destination_path("/tmp/bad.raw", &cache).expect_err("must fail");
        assert!(matches!(err.kind, ErrorKind::InvalidArgument));
    }

    #[test]
    fn upload_rejects_unsupported_format() {
        let temp = tempfile::tempdir().expect("tempdir");
        let err = upload_image_to_cache(
            temp.path(),
            UploadImageRequest {
                image_bytes: vec![1, 2, 3],
                source_name: "disk.iso".to_string(),
                destination_name: String::new(),
                image_format: "iso".to_string(),
                image_sha256: String::new(),
            },
        )
        .expect_err("unsupported format should fail");
        assert!(matches!(err.kind, ErrorKind::InvalidArgument));
    }

    #[test]
    fn upload_rejects_invalid_qcow2_magic() {
        let temp = tempfile::tempdir().expect("tempdir");
        let err = upload_image_to_cache(
            temp.path(),
            UploadImageRequest {
                image_bytes: vec![0, 1, 2, 3, 4],
                source_name: "disk.qcow2".to_string(),
                destination_name: String::new(),
                image_format: "qcow2".to_string(),
                image_sha256: String::new(),
            },
        )
        .expect_err("invalid qcow2 should fail");
        assert!(matches!(err.kind, ErrorKind::FailedPrecondition));
    }

    #[test]
    fn upload_sanitizes_destination_name_under_cache_dir() {
        let temp = tempfile::tempdir().expect("tempdir");
        let result = upload_image_to_cache(
            temp.path(),
            UploadImageRequest {
                image_bytes: vec![9, 8, 7, 6],
                source_name: "disk.raw".to_string(),
                destination_name: "../escape.raw".to_string(),
                image_format: "raw".to_string(),
                image_sha256: String::new(),
            },
        )
        .expect("upload should succeed");
        let uploaded = PathBuf::from(&result.path);
        assert!(uploaded.starts_with(temp.path()));
        assert!(uploaded.exists());
        assert!(!result.path.contains("/../"));
    }

    #[test]
    fn upload_from_path_handles_qcow2_magic_and_sha() {
        let temp = tempfile::tempdir().expect("tempdir");
        let src = temp.path().join("ubuntu.qcow2");
        let cache_dir = temp.path().join("cache");
        let mut content = Vec::from([b'Q', b'F', b'I', 0xfb]);
        content.extend_from_slice(&[1, 2, 3, 4]);
        std::fs::write(&src, &content).expect("write src");
        let expected = sha256sum_file(&src).expect("sha");
        let result = upload_image_file_to_cache(
            &cache_dir,
            UploadImageFromPathRequest {
                source_file_path: src.display().to_string(),
                source_name: "ubuntu.qcow2".to_string(),
                destination_name: String::new(),
                image_format: "qcow2".to_string(),
                image_sha256: expected.clone(),
            },
        )
        .expect("upload from path");
        assert_eq!(result.image_format, "qcow2");
        assert_eq!(result.image_sha256, expected);
        assert!(PathBuf::from(result.path).exists());
    }
}
