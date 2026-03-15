//! E2E test: temp dir, fjx.toml, _RAF with one RAF, stub camera, run conversion, assert JPEG produced.

use fjx::config;
use fjx::fuji;
use fjx::ui;
use std::fs;
use std::path::Path;

/// Minimal RAF magic (15 bytes) + 1 byte so len >= 16 for fuji's check.
const MINIMAL_RAF: &[u8] = b"FUJIFILMCCD-RAW\x00";

#[test]
fn e2e_project_convert_with_stub_camera_produces_jpeg() {
    unsafe { std::env::set_var("FJX_STUB_CAMERA", "1") };
    let guard = EnvGuard::new("FJX_STUB_CAMERA");

    let dir = tempfile::tempdir().unwrap();
    let project_root = dir.path().to_path_buf();
    let config_path = project_root.join(config::CONFIG_FILENAME);

    // fjx.toml
    let toml = r#"
raw_dir = "./_RAF"
[[output]]
recipe = "classic-chrome"
"#;
    fs::write(&config_path, toml).unwrap();

    // _RAF/one.raf
    let raw_dir = project_root.join("_RAF");
    fs::create_dir_all(&raw_dir).unwrap();
    fs::write(raw_dir.join("one.raf"), MINIMAL_RAF).unwrap();

    let cfg = config::load_config(&config_path).unwrap();
    config::validate_config(&project_root, &cfg).unwrap();
    let batches = config::expand_config(&project_root, &cfg).unwrap();
    assert!(!batches.is_empty());
    let output_batch = &batches[0];
    let (settings, jobs) = &output_batch.batches[0];
    assert!(!jobs.is_empty());

    fs::create_dir_all(&output_batch.output_dir).unwrap();
    let total_jobs: usize = batches.iter().flat_map(|b| b.batches.iter().map(|(_, j)| j.len())).sum();
    let ui = ui::ConvertProgress::new(false, total_jobs);

    let mut camera = fuji::open_camera();
    fuji::convert(&mut *camera, jobs, settings, &ui, true);

    let output_path = Path::new(&jobs[0].1);
    assert!(output_path.exists(), "output JPEG should exist: {}", output_path.display());
    let data = fs::read(output_path).unwrap();
    assert!(data.len() >= 2 && data[0] == 0xFF && data[1] == 0xD8, "output should be JPEG (FF D8)");
    drop(guard);
}

struct EnvGuard {
    key: String,
    saved: Option<String>,
}

impl EnvGuard {
    fn new(key: &str) -> Self {
        let saved = std::env::var(key).ok();
        Self {
            key: key.to_string(),
            saved,
        }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        if let Some(ref v) = self.saved {
            unsafe { std::env::set_var(&self.key, v) };
        } else {
            unsafe { std::env::remove_var(&self.key) };
        }
    }
}
