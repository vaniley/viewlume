//! Best-effort startup diagnostics for native renderer failures.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use crate::config::ViewerConfig;
use crate::presentation::PresentationPolicy;

const MAX_DIAGNOSTIC_BYTES: usize = 256 * 1024;
const DIAGNOSTIC_FILE_NAME: &str = "diagnostics.log";
static OS_VERSION: OnceLock<String> = OnceLock::new();

/// Records the renderer selected by eframe after successful initialization.
///
/// Failure to inspect or write diagnostics is intentionally ignored: logging must never keep the
/// viewer from starting.
pub fn record_renderer_state(
    creation_context: &eframe::CreationContext<'_>,
    presentation: PresentationPolicy,
) {
    let mut entry = startup_header(presentation);

    if let Some(render_state) = creation_context.wgpu_render_state.as_ref() {
        let adapter = render_state.adapter.get_info();
        let limits = render_state.device.limits();
        entry.push_str("initialization_error: none\n");
        entry.push_str(&format!("selected_backend: {:?}\n", adapter.backend));
        entry.push_str(&format!("adapter_name: {}\n", sanitize(&adapter.name)));
        entry.push_str(&format!("adapter_vendor: {}\n", adapter.vendor));
        entry.push_str(&format!("adapter_device: {}\n", adapter.device));
        entry.push_str(&format!("adapter_type: {:?}\n", adapter.device_type));
        entry.push_str(&format!("adapter_driver: {}\n", sanitize(&adapter.driver)));
        entry.push_str(&format!(
            "adapter_driver_info: {}\n",
            sanitize(&adapter.driver_info)
        ));
        entry.push_str(&format!(
            "present_format: {:?}\n",
            render_state.target_format
        ));
        entry.push_str(&format!(
            "device_limits: max_texture_dimension_1d={}, max_texture_dimension_2d={}, max_texture_dimension_3d={}, max_texture_array_layers={}, max_bind_groups={}, max_uniform_buffer_binding_size={}, max_storage_buffer_binding_size={}\n",
            limits.max_texture_dimension_1d,
            limits.max_texture_dimension_2d,
            limits.max_texture_dimension_3d,
            limits.max_texture_array_layers,
            limits.max_bind_groups,
            limits.max_uniform_buffer_binding_size,
            limits.max_storage_buffer_binding_size,
        ));
    } else {
        entry.push_str("selected_backend: unavailable\n");
        entry.push_str("initialization_error: wgpu render state was unavailable\n");
    }

    record(entry);
}

/// Records an error returned by eframe while the native renderer was starting.
pub fn record_startup_failure(presentation: PresentationPolicy, error: &str) {
    let mut entry = startup_header(presentation);
    entry.push_str("selected_backend: unavailable\n");
    entry.push_str(&format!("initialization_error: {}\n", sanitize(error)));
    record(entry);
}

/// Location beside the existing platform configuration file.
pub fn diagnostic_path() -> PathBuf {
    let config_path = ViewerConfig::config_path();
    config_path.with_file_name(DIAGNOSTIC_FILE_NAME)
}

fn record(entry: String) {
    let _ = append_bounded(&diagnostic_path(), &entry);
}

fn startup_header(presentation: PresentationPolicy) -> String {
    format!(
        "\n=== Viewlume startup ===\napp_version: {}\nos_version: {}\nrequested_backend: {}\neffective_native_transparency: {}\nopaque_window_override: {}\n",
        env!("CARGO_PKG_VERSION"),
        os_version(),
        requested_backend(),
        presentation.transparent_window(),
        presentation.opaque_window(),
    )
}

fn requested_backend() -> String {
    std::env::var("WGPU_BACKEND").unwrap_or_else(|_| "automatic (eframe default)".to_owned())
}

fn os_version() -> &'static str {
    OS_VERSION.get_or_init(detect_os_version).as_str()
}

#[cfg(target_os = "windows")]
fn detect_os_version() -> String {
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mut command = Command::new("cmd");
    command
        .args(["/D", "/C", "ver"])
        .creation_flags(CREATE_NO_WINDOW);
    command_os_version(&mut command)
}

#[cfg(not(target_os = "windows"))]
fn detect_os_version() -> String {
    let mut command = Command::new("uname");
    command.args(["-s", "-r"]);
    command_os_version(&mut command)
}

fn command_os_version(command: &mut Command) -> String {
    command
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|version| sanitize(&version))
        .filter(|version| !version.is_empty())
        .unwrap_or_else(|| format!("{} {}", std::env::consts::OS, std::env::consts::ARCH))
}

fn sanitize(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn append_bounded(path: &Path, entry: &str) -> io::Result<()> {
    let entry = truncate_to_limit(entry.as_bytes());
    let current_size = fs::metadata(path)
        .map(|metadata| metadata.len() as usize)
        .unwrap_or(0);

    if current_size.saturating_add(entry.len()) > MAX_DIAGNOSTIC_BYTES {
        rotate(path)?;
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    file.write_all(entry)
}

fn rotate(path: &Path) -> io::Result<()> {
    if !path.exists() {
        return Ok(());
    }

    let previous = path.with_extension("log.1");
    let current = fs::read(path)?;
    fs::write(previous, tail_to_limit(&current))?;
    fs::remove_file(path)
}

fn truncate_to_limit(bytes: &[u8]) -> &[u8] {
    if bytes.len() <= MAX_DIAGNOSTIC_BYTES {
        bytes
    } else {
        &bytes[..MAX_DIAGNOSTIC_BYTES]
    }
}

fn tail_to_limit(bytes: &[u8]) -> &[u8] {
    if bytes.len() <= MAX_DIAGNOSTIC_BYTES {
        bytes
    } else {
        &bytes[bytes.len() - MAX_DIAGNOSTIC_BYTES..]
    }
}

#[cfg(test)]
mod tests {
    use super::{append_bounded, MAX_DIAGNOSTIC_BYTES};
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_directory(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock after Unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("viewlume-{name}-{}-{unique}", std::process::id()))
    }

    #[test]
    fn diagnostics_rotate_and_keep_one_bounded_previous_file() {
        let directory = test_directory("diagnostics-rotation");
        fs::create_dir_all(&directory).expect("create diagnostic test directory");
        let path = directory.join("diagnostics.log");
        let first = "a".repeat(MAX_DIAGNOSTIC_BYTES - 8);
        append_bounded(&path, &first).expect("write initial diagnostics");
        append_bounded(&path, "next startup\n").expect("rotate diagnostics");

        let previous = path.with_extension("log.1");
        assert_eq!(
            fs::metadata(&previous).expect("previous log exists").len() as usize,
            first.len()
        );
        assert_eq!(
            fs::read_to_string(&path).expect("current log"),
            "next startup\n"
        );
        assert!(
            fs::metadata(&path).expect("current log exists").len() as usize <= MAX_DIAGNOSTIC_BYTES
        );
        assert!(
            fs::metadata(&previous).expect("previous log exists").len() as usize
                <= MAX_DIAGNOSTIC_BYTES
        );

        fs::remove_dir_all(directory).expect("remove diagnostic test directory");
    }

    #[test]
    fn unavailable_diagnostic_destination_returns_an_error() {
        let directory = test_directory("diagnostics-unwritable");
        fs::create_dir_all(&directory).expect("create diagnostic test directory");
        let not_a_directory = directory.join("not-a-directory");
        fs::write(&not_a_directory, "file blocks diagnostics directory").expect("create blocker");

        assert!(append_bounded(&not_a_directory.join("diagnostics.log"), "entry\n").is_err());

        fs::remove_dir_all(directory).expect("remove diagnostic test directory");
    }
}
