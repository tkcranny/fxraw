#![allow(dead_code)]

use crate::profile::{self, RecipeSettings};
use crate::ptp::{self, DeviceInfo, PtpCamera, PtpResponse, RC_OK};
use crate::ui::ConvertProgress;
use std::fs;
use std::path::Path;
use std::process::Command;

// ---------------------------------------------------------------------------
// RAF metadata (ISO, DR) for pre-flight validation
// ---------------------------------------------------------------------------

pub struct RafMeta {
    pub iso: Option<u32>,
    pub dynamic_range: Option<u32>,
}

/// Minimum ISO required for each DR level on X-Trans 5 (X100VI).
/// DR200 uses 1-stop underexposure → needs 2× base ISO (400).
/// DR400 uses 2-stop underexposure → needs 4× base ISO (800).
pub fn min_iso_for_dr(dr: u32) -> u32 {
    match dr {
        400 => 800,
        200 => 400,
        _ => 0,
    }
}

/// Highest DR the camera can apply given the shooting ISO.
pub fn max_dr_for_iso(iso: u32) -> u32 {
    if iso >= 800 {
        400
    } else if iso >= 400 {
        200
    } else {
        100
    }
}

/// Result of DR clamping: the final DR value and any warnings generated.
pub struct DrClampResult {
    pub dr: u32,
    pub warnings: Vec<String>,
}

/// Clamp recipe DR to what the RAF actually supports.
///
/// Two hard constraints (violating either produces green/corrupt output):
///
///   1. ISO floor: DR200 needs ISO 400+, DR400 needs ISO 800+.
///
///   2. Max 1-stop increase from shooting DR. The raw data only has headroom
///      for one additional stop of DR expansion beyond what was captured.
///      DR100 shots support up to DR200; DR200 shots support up to DR400.
///      (Matches the limits enforced by Fuji X RAW Studio.)
pub fn clamp_dr(recipe_dr: u32, iso: Option<u32>, shot_dr: Option<u32>) -> DrClampResult {
    let mut dr = recipe_dr;
    let mut warnings = Vec::new();

    if let Some(iso) = iso {
        let iso_max = max_dr_for_iso(iso);
        if dr > iso_max {
            warnings.push(format!(
                "DR: Recipe requests DR{dr} but ISO {iso} supports up to DR{iso_max} \
                 (DR{dr} needs ISO {}). Clamping.",
                min_iso_for_dr(dr)
            ));
            dr = iso_max;
        }
    }

    if let Some(shot) = shot_dr {
        let ceiling = (shot * 2).min(400);
        if dr > ceiling {
            warnings.push(format!(
                "DR: Recipe requests DR{dr} but this RAF was shot at DR{shot} \
                 (max 1-stop increase → DR{ceiling}). Clamping."
            ));
            dr = ceiling;
        }
    }

    DrClampResult { dr, warnings }
}

pub fn read_raf_meta(path: &str) -> RafMeta {
    let output = Command::new("exiftool")
        .args(["-s", "-s", "-s", "-ISO", "-FujiFilm:DynamicRange"])
        .arg(path)
        .output();

    let (iso, dr) = match output {
        Ok(o) if o.status.success() => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            let mut iso = None;
            let mut dr = None;
            for line in stdout.lines() {
                let line = line.trim();
                if iso.is_none() {
                    if let Ok(v) = line.parse::<u32>() {
                        iso = Some(v);
                        continue;
                    }
                }
                if dr.is_none() {
                    let dr_val = match line {
                        "Standard" => Some(100),
                        "Wide1" | "Wide 1" => Some(200),
                        "Wide2" | "Wide 2" => Some(400),
                        s => s.trim_end_matches('%').parse::<u32>().ok(),
                    };
                    if let Some(v) = dr_val {
                        dr = Some(v);
                    }
                }
            }
            (iso, dr)
        }
        _ => (None, None),
    };

    RafMeta { iso, dynamic_range: dr }
}

// ---------------------------------------------------------------------------
// Fujifilm USB identifiers
// ---------------------------------------------------------------------------

pub const FUJIFILM_VENDOR_ID: u16 = 0x04CB;

/// Known X100VI product IDs. Add new ones here if the camera shows up with
/// a different PID depending on USB mode.
pub const X100VI_PRODUCT_IDS: &[(u16, &str)] = &[(0x0305, "X100VI (PTP)")];

/// Default product ID used for PTP connections.
const DEFAULT_PRODUCT_ID: u16 = 0x0305;

// ---------------------------------------------------------------------------
// Fuji vendor PTP operation codes (from reverse engineering / probe)
// ---------------------------------------------------------------------------

// Vendor PTP operation codes (from Fudge / PROTOCOL.md)
/// Vendor SendObjectInfo – send ObjectInfo for RAF upload
const FUJI_OC_SEND_OBJECT_INFO: u16 = 0x900C;
/// Vendor SendObject – send RAF file bytes
const FUJI_OC_SEND_OBJECT: u16 = 0x900D;

// Fuji vendor device properties
const FUJI_PROP_RAW_DEVELOP_MODE: u16 = 0xD34D;
const FUJI_PROP_RAW_CONV_VERSION: u16 = 0xD36A;
const FUJI_PROP_RAW_CONV_PARAM: u16 = 0xD36B;
const FUJI_PROP_USB_MODE: u16 = 0xD21C;
/// Development profile (get = camera defaults, set = apply before conversion)
const FUJI_PROP_RAW_CONV_PROFILE: u16 = 0xD185;
/// Set to 0 to trigger RAW conversion on the camera
const FUJI_PROP_START_RAW_CONV: u16 = 0xD183;

/// Fuji-specific object format for RAF upload (NOT 0xB103)
const PTP_OFC_FUJI_RAW_UPLOAD: u16 = 0xF802;

/// Standard RAF format code (for reference in probe)
const PTP_OFC_FUJI_RAF: u16 = 0xB103;

// ---------------------------------------------------------------------------
// probe – dump camera PTP capabilities
// ---------------------------------------------------------------------------

pub fn probe() {
    println!("Connecting to Fujifilm X100VI...\n");

    let mut camera = open_camera();

    println!("Connected. Opening PTP session...");
    camera.open_session().unwrap_or_else(|e| {
        eprintln!("Error: {e}");
        std::process::exit(1);
    });

    println!("Session open. Querying device info...\n");

    let info = camera.get_device_info().unwrap_or_else(|e| {
        eprintln!("Failed to get device info: {e}");
        std::process::exit(1);
    });
    print_device_info(&info);

    // Try to read vendor-specific device info if supported
    if info
        .operations_supported
        .contains(&FUJI_OC_SEND_OBJECT_INFO)
    {
        println!("\n--- Fuji Vendor DeviceInfo (0x900C) ---");
        match camera.vendor_receive(FUJI_OC_SEND_OBJECT_INFO, &[]) {
            Ok((data, resp)) => {
                println!(
                    "  Response: 0x{:04X} ({})",
                    resp.code,
                    ptp::response_name(resp.code)
                );
                println!("  Data: {} bytes", data.len() as usize);
                if !data.is_empty() {
                    print_hex_dump(&data, 256);
                }
            }
            Err(e) => {
                println!("  Skipped (timed out or unsupported in this mode)");
                println!("  Detail: {e}");
            }
        }
    }

    // Dump all vendor device properties with labels
    let vendor_props: Vec<u16> = info
        .device_properties_supported
        .iter()
        .copied()
        .filter(|&c| c >= 0xD000)
        .collect();

    if !vendor_props.is_empty() {
        println!(
            "\n--- Fuji Vendor Properties ({} found) ---",
            vendor_props.len()
        );
        for prop in &vendor_props {
            let label = ptp::property_name(*prop);
            match camera.get_device_prop_value(*prop) {
                Ok(data) => {
                    print!("  0x{prop:04X} {label:<30} ");
                    match data.len() {
                        2 => {
                            let val = u16::from_le_bytes([data[0], data[1]]);
                            let signed = val as i16;
                            if signed < 0 {
                                println!("{signed} (0x{val:04X})");
                            } else {
                                println!("{val} (0x{val:04X})");
                            }
                        }
                        4 => {
                            let val = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
                            println!("{val} (0x{val:08X})");
                        }
                        n if n <= 8 => {
                            for b in &data {
                                print!("{b:02X} ");
                            }
                            println!();
                        }
                        n => println!("{n} bytes"),
                    }
                }
                Err(_) => println!("  0x{prop:04X} {label:<30} (read error)"),
            }
        }
    }

    println!("\nClosing session...");
    let _ = camera.close_session();
    println!("Done.");
}

// ---------------------------------------------------------------------------
// convert – process a RAF file through the camera
// ---------------------------------------------------------------------------

/// Run conversions. When `manage_session` is true, opens and closes the PTP session
/// (use for a single batch, e.g. CLI). When false, the session must already be open
/// (use when running multiple batches so the session is opened once and closed once).
pub fn convert(
    camera: &mut dyn ptp::FujiCamera,
    jobs: &[(String, String)],
    recipe: &RecipeSettings,
    ui: &ConvertProgress,
    manage_session: bool,
) {
    let mut validated: Vec<(&str, &str, Vec<u8>, RecipeSettings)> = Vec::with_capacity(jobs.len());
    for (input, output) in jobs {
        let input_path = Path::new(input);
        if !input_path.exists() {
            eprintln!("Error: file not found: {input}");
            std::process::exit(1);
        }
        let raf_data = fs::read(input).unwrap_or_else(|e| {
            eprintln!("Error reading {input}: {e}");
            std::process::exit(1);
        });
        if raf_data.len() < 16 || &raf_data[..15] != b"FUJIFILMCCD-RAW" {
            eprintln!("Error: {input} does not appear to be a valid Fujifilm RAF file.");
            std::process::exit(1);
        }

        let meta = read_raf_meta(input);
        let mut file_recipe = recipe.clone();

        if meta.iso.is_some() || meta.dynamic_range.is_some() {
            ui.meta_info(input, meta.iso, meta.dynamic_range);
        }

        if let Some(recipe_dr) = file_recipe.dynamic_range {
            let result = clamp_dr(recipe_dr, meta.iso, meta.dynamic_range);
            for w in &result.warnings {
                ui.dr_clamped(w);
            }
            if result.dr != recipe_dr {
                file_recipe.dynamic_range = Some(result.dr);
            }
        }

        validated.push((input, output, raf_data, file_recipe));
    }

    let total = validated.len();
    ui.batch_header(total);

    if manage_session {
        ui.step(0, "connecting…");
        camera.open_session().unwrap_or_else(|e| {
            eprintln!("  Failed to open session: {e}");
            std::process::exit(1);
        });

        let info = camera.get_device_info().unwrap_or_else(|e| {
            eprintln!("  Failed to get device info: {e}");
            std::process::exit(1);
        });
        ui.camera_info(&info.manufacturer, &info.model, &info.device_version);

        match camera.get_device_prop_value(0xD16E) {
            Ok(data) if data.len() >= 2 => {
                let mode = u16::from_le_bytes([data[0], data[1]]);
                ui.usb_mode(mode);
            }
            _ => ui.usb_mode_unreadable(),
        }
    }

    // ------------------------------------------------------------------
    // Process each file
    // ------------------------------------------------------------------
    let mut succeeded = 0u32;
    let mut failed = 0u32;

    for (idx, (input, output, raf_data, file_recipe)) in validated.iter().enumerate() {
        let raf_size_mb = raf_data.len() as f64 / (1024.0 * 1024.0);
        ui.file_start(idx, input, output, raf_size_mb);

        match convert_one(camera, raf_data, raf_size_mb, output, file_recipe, ui) {
            Ok(jpeg_size_mb) => {
                succeeded += 1;
                ui.file_done(output, jpeg_size_mb);
            }
            Err(e) => {
                failed += 1;
                ui.file_failed(input, &e);
            }
        }
    }

    if manage_session {
        let _ = camera.close_session();
    }
    ui.summary(succeeded, failed);

    if failed > 0 {
        std::process::exit(1);
    }
}

/// Process a single RAF through the camera. Returns the JPEG size in MB on
/// success, or an error message so the caller can continue with the next file.
fn convert_one(
    camera: &mut dyn ptp::FujiCamera,
    raf_data: &[u8],
    raf_size_mb: f64,
    output_path: &str,
    recipe: &RecipeSettings,
    ui: &ConvertProgress,
) -> Result<f64, String> {
    // Step A: Send ObjectInfo via 0x900C
    ui.step(1, "sending object info…");
    let obj_info = ptp::build_object_info(
        PTP_OFC_FUJI_RAW_UPLOAD,
        raf_data.len() as u32,
        "FUP_FILE.dat",
    );
    let resp = camera.vendor_send(FUJI_OC_SEND_OBJECT_INFO, &[0, 0, 0], &obj_info)?;
    ui.step_detail(&format!(
        "-> 0x{:04X} ({})",
        resp.code,
        ptp::response_name(resp.code)
    ));
    if resp.code != RC_OK {
        return Err("0x900C failed. Is the camera in USB RAW CONV mode?".into());
    }

    // Step B: Send RAF file data via 0x900D
    ui.step(2, &format!("uploading RAF ({raf_size_mb:.1} MB)…"));
    let resp = camera.vendor_send(FUJI_OC_SEND_OBJECT, &[], raf_data)?;
    ui.step_detail(&format!(
        "-> 0x{:04X} ({})",
        resp.code,
        ptp::response_name(resp.code)
    ));
    if resp.code != RC_OK {
        return Err("0x900D (SendObject) failed.".into());
    }

    // Step C/D: Get development profile, apply recipe, set it back
    ui.step(3, "reading development profile…");
    let mut profile_data = match camera.get_device_prop_value(FUJI_PROP_RAW_CONV_PROFILE) {
        Ok(data) => {
            ui.step_detail(&format!("Got {} bytes of profile data", data.len()));
            ui.step_detail(&format!(
                "Current film sim: {}",
                profile::current_film_sim(&data)
            ));
            data
        }
        Err(e) => {
            ui.step_detail(&format!("Warning: could not read profile: {e}"));
            ui.step_detail("Proceeding without profile (camera will use defaults)");
            Vec::new()
        }
    };

    if !profile_data.is_empty() {
        if !recipe.is_empty() {
            ui.step_detail(&format!("Applying recipe: {}", recipe.summary()));
            let changes = profile::apply_recipe(&mut profile_data, recipe);
            ui.step_detail(&changes);
        }

        ui.step(4, "setting development profile…");
        match camera.set_device_prop_value(FUJI_PROP_RAW_CONV_PROFILE, &profile_data) {
            Ok(()) => ui.step_detail("-> OK"),
            Err(e) => ui.step_detail(&format!("Warning: could not set profile: {e}")),
        }
    } else {
        ui.step(4, "skipping profile (no data)…");
    }

    // Step E: Trigger conversion
    ui.step(5, "triggering RAW conversion…");
    match camera.set_device_prop_value(FUJI_PROP_START_RAW_CONV, &0u32.to_le_bytes()) {
        Ok(()) => ui.step_detail("-> OK"),
        Err(e) => {
            ui.step_detail(&format!("u32 failed ({e}), trying u16..."));
            camera
                .set_device_prop_value(FUJI_PROP_START_RAW_CONV, &0u16.to_le_bytes())
                .map_err(|e2| format!("Failed to trigger conversion: {e2}"))?;
            ui.step_detail("-> OK (u16)");
        }
    }

    // Step F/G: Poll GetObjectHandles until the JPEG appears, then download
    ui.step(6, "waiting for camera…");
    ui.poll_start();

    let poll_interval = std::time::Duration::from_secs(2);
    let max_polls = 45;

    for attempt in 1..=max_polls {
        std::thread::sleep(poll_interval);
        ui.poll_tick(attempt, max_polls);

        match camera.get_object_handles(0xFFFFFFFF, 0, 0) {
            Ok(handles) if !handles.is_empty() => {
                ui.poll_result(&format!(
                    "found {} object(s): {:08X?}",
                    handles.len(),
                    handles
                ));
                ui.poll_done();

                let handle = handles[0];
                ui.step_detail(&format!("Downloading object 0x{handle:08X}..."));
                match camera.get_object(handle) {
                    Ok(data) => {
                        let size_mb = data.len() as f64 / (1024.0 * 1024.0);
                        ui.step_detail(&format!("Got {size_mb:.1} MB"));

                        if data.len() >= 2 && data[0] == 0xFF && data[1] == 0xD8 {
                            ui.step_detail("JPEG signature verified!");
                        } else {
                            ui.step_detail(
                                "Warning: data doesn't start with JPEG magic (FF D8)",
                            );
                        }

                        fs::write(output_path, &data).unwrap_or_else(|e| {
                            eprintln!("  Error writing {output_path}: {e}");
                            std::process::exit(1);
                        });
                        chown_to_sudo_user(output_path);

                        ui.step_detail(&format!("Cleaning up (DeleteObject)..."));
                        match camera.delete_object(handle) {
                            Ok(_) => ui.step_detail("Object deleted."),
                            Err(e) => {
                                ui.step_detail(&format!("Could not delete object: {e}"))
                            }
                        }

                        return Ok(size_mb);
                    }
                    Err(e) => {
                        ui.poll_result(&format!("GetObject failed: {e}"));
                    }
                }
            }
            Ok(_) => ui.poll_result("no objects yet"),
            Err(e) => ui.poll_result(&format!("error: {e}")),
        }
    }

    ui.poll_done();
    Err("Timed out waiting for the camera to produce the converted image.".into())
}

// ---------------------------------------------------------------------------
// Stub camera for tests (no USB device)
// ---------------------------------------------------------------------------

/// Minimal valid JPEG (SOI + minimal segment + EOI) for stub conversion output.
pub const STUB_JPEG: &[u8] = &[0xFF, 0xD8, 0xFF, 0xDB, 0x00, 0x43, 0x00, 0x08, 0x06, 0x06, 0x07, 0x06, 0x05, 0x08, 0x07, 0x07, 0x07, 0x09, 0x09, 0x08, 0x0A, 0x0C, 0x14, 0x0D, 0x0C, 0x0B, 0x0B, 0x0C, 0x19, 0x12, 0x13, 0x0F, 0x14, 0x1D, 0x1A, 0x1F, 0x1E, 0x1D, 0x1A, 0x1C, 0x1C, 0x20, 0x24, 0x2E, 0x27, 0x20, 0x22, 0x2C, 0x23, 0x1C, 0x1C, 0x28, 0x37, 0x29, 0x2C, 0x30, 0x31, 0x34, 0x34, 0x34, 0x1F, 0x27, 0x39, 0x3D, 0x38, 0x32, 0x3C, 0x2E, 0x33, 0x34, 0x32, 0xFF, 0xC0, 0x00, 0x0B, 0x08, 0x00, 0x01, 0x00, 0x01, 0x01, 0x01, 0x11, 0x00, 0xFF, 0xC4, 0x00, 0x1F, 0x00, 0x00, 0x01, 0x05, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0xFF, 0xDA, 0x00, 0x08, 0x01, 0x01, 0x00, 0x00, 0x3F, 0x00, 0x7B, 0x94, 0x32, 0x2B, 0xFF, 0xD9];

/// Mock camera used when FXRAW_STUB_CAMERA=1 (e.g. in tests). Returns STUB_JPEG from get_object(1).
pub struct StubCamera;

impl ptp::FujiCamera for StubCamera {
    fn open_session(&mut self) -> Result<(), String> {
        Ok(())
    }
    fn close_session(&mut self) -> Result<(), String> {
        Ok(())
    }
    fn get_device_info(&mut self) -> Result<DeviceInfo, String> {
        Ok(DeviceInfo {
            standard_version: 100,
            vendor_extension_id: 0,
            vendor_extension_version: 0,
            vendor_extension_desc: String::new(),
            functional_mode: 0,
            operations_supported: vec![],
            events_supported: vec![],
            device_properties_supported: vec![],
            capture_formats: vec![],
            image_formats: vec![],
            manufacturer: "FXRAW Stub".into(),
            model: "Test".into(),
            device_version: "0".into(),
            serial_number: String::new(),
        })
    }
    fn get_device_prop_value(&mut self, _prop: u16) -> Result<Vec<u8>, String> {
        Ok(Vec::new())
    }
    fn set_device_prop_value(&mut self, _prop: u16, _data: &[u8]) -> Result<(), String> {
        Ok(())
    }
    fn vendor_send(
        &mut self,
        _op: u16,
        _params: &[u32],
        _data: &[u8],
    ) -> Result<PtpResponse, String> {
        Ok(PtpResponse {
            code: RC_OK,
            params: vec![],
        })
    }
    fn get_object_handles(
        &mut self,
        _storage_id: u32,
        _format: u32,
        _parent: u32,
    ) -> Result<Vec<u32>, String> {
        Ok(vec![1])
    }
    fn get_object(&mut self, handle: u32) -> Result<Vec<u8>, String> {
        if handle == 1 {
            Ok(STUB_JPEG.to_vec())
        } else {
            Ok(Vec::new())
        }
    }
    fn delete_object(&mut self, _handle: u32) -> Result<PtpResponse, String> {
        Ok(PtpResponse {
            code: RC_OK,
            params: vec![],
        })
    }
    fn vendor_receive(
        &mut self,
        _op: u16,
        _params: &[u32],
    ) -> Result<(Vec<u8>, PtpResponse), String> {
        Ok((
            Vec::new(),
            PtpResponse {
                code: RC_OK,
                params: vec![],
            },
        ))
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// If we were invoked via sudo, change ownership of the path (file or directory)
/// to the real user so they don't end up with root-owned output files.
#[cfg(unix)]
pub fn chown_to_sudo_user(path: &str) {
    let uid = std::env::var("SUDO_UID").ok().and_then(|s| s.parse::<u32>().ok());
    let gid = std::env::var("SUDO_GID").ok().and_then(|s| s.parse::<u32>().ok());
    if let (Some(uid), Some(gid)) = (uid, gid) {
        if let Err(e) = std::os::unix::fs::chown(path, Some(uid), Some(gid)) {
            eprintln!("  Warning: could not chown output to invoking user: {e}");
        }
    }
}

#[cfg(not(unix))]
pub fn chown_to_sudo_user(_path: &str) {}

/// Open the camera (real USB or stub when FXRAW_STUB_CAMERA=1). Caller must pass
/// the returned box to `convert()`.
pub fn open_camera() -> Box<dyn ptp::FujiCamera> {
    if std::env::var("FXRAW_STUB_CAMERA").is_ok() {
        return Box::new(StubCamera);
    }
    Box::new(
        PtpCamera::open(FUJIFILM_VENDOR_ID, DEFAULT_PRODUCT_ID).unwrap_or_else(|e| {
            eprintln!("Error: {e}");
            eprintln!();
            eprintln!("Make sure:");
            eprintln!("  - The camera is connected via USB and turned on");
            eprintln!("  - No other app (Photos, Image Capture) is using it");
            eprintln!("  - USB mode is set correctly on the camera");
            eprintln!("  - You have run `sudo fxraw setup` (one-time macOS PTP daemon fix)");
            std::process::exit(1);
        }),
    )
}

fn print_device_info(info: &ptp::DeviceInfo) {
    println!("=== Device Info ===");
    println!("Manufacturer:       {}", info.manufacturer);
    println!("Model:              {}", info.model);
    println!("Device Version:     {}", info.device_version);
    println!("Serial Number:      {}", info.serial_number);
    println!(
        "PTP Version:        {}.{}",
        info.standard_version / 100,
        info.standard_version % 100
    );
    println!("Vendor Ext ID:      0x{:08X}", info.vendor_extension_id);
    println!("Vendor Ext Version: {}", info.vendor_extension_version);
    if !info.vendor_extension_desc.is_empty() {
        println!("Vendor Ext Desc:    {}", info.vendor_extension_desc);
    }
    println!("Functional Mode:    {}", info.functional_mode);

    println!(
        "\n--- Operations Supported ({}) ---",
        info.operations_supported.len()
    );
    for code in &info.operations_supported {
        println!("  0x{code:04X}  {}", ptp::operation_name(*code));
    }

    println!(
        "\n--- Events Supported ({}) ---",
        info.events_supported.len()
    );
    for code in &info.events_supported {
        println!("  0x{code:04X}");
    }

    println!(
        "\n--- Device Properties ({}) ---",
        info.device_properties_supported.len()
    );
    for code in &info.device_properties_supported {
        println!("  0x{code:04X}  {}", ptp::property_name(*code));
    }

    println!("\n--- Capture Formats ({}) ---", info.capture_formats.len());
    for code in &info.capture_formats {
        println!("  0x{code:04X}  {}", ptp::format_name(*code));
    }

    println!("\n--- Image Formats ({}) ---", info.image_formats.len());
    for code in &info.image_formats {
        println!("  0x{code:04X}  {}", ptp::format_name(*code));
    }
}

fn print_hex_dump(data: &[u8], max_bytes: usize) {
    let len = data.len().min(max_bytes);
    for (i, chunk) in data[..len].chunks(16).enumerate() {
        print!("  {:04X}  ", i * 16);
        for b in chunk {
            print!("{b:02X} ");
        }
        // Pad if short line
        for _ in chunk.len()..16 {
            print!("   ");
        }
        print!(" ");
        for b in chunk {
            if b.is_ascii_graphic() || *b == b' ' {
                print!("{}", *b as char);
            } else {
                print!(".");
            }
        }
        println!();
    }
    if data.len() > max_bytes {
        println!("  ... ({} more bytes)", data.len() - max_bytes);
    }
}
