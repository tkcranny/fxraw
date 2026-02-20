#![allow(dead_code)]

use crate::profile::{self, RecipeSettings};
use crate::ptp::{self, PtpCamera, RC_OK};
use std::fs;
use std::path::Path;

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
                println!("  Data: {} bytes", data.len());
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

pub fn convert(jobs: &[(String, String)], recipe: &RecipeSettings) {
    // Validate all inputs up-front before touching the camera
    let mut validated: Vec<(&str, &str, Vec<u8>)> = Vec::with_capacity(jobs.len());
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
        validated.push((input, output, raf_data));
    }

    let total = validated.len();
    if total > 1 {
        println!("Converting {total} files\n");
    }

    // ------------------------------------------------------------------
    // Connect & open session (once for the whole batch)
    // ------------------------------------------------------------------
    println!("[connect] Connecting to camera...");
    let mut camera = open_camera();
    camera.open_session().unwrap_or_else(|e| {
        eprintln!("  Failed to open session: {e}");
        std::process::exit(1);
    });

    let info = camera.get_device_info().unwrap_or_else(|e| {
        eprintln!("  Failed to get device info: {e}");
        std::process::exit(1);
    });
    println!(
        "  {} {} (fw {})",
        info.manufacturer, info.model, info.device_version
    );

    // Check USB mode (0xD16E should be 6 for RAW CONV)
    match camera.get_device_prop_value(0xD16E) {
        Ok(data) if data.len() >= 2 => {
            let mode = u16::from_le_bytes([data[0], data[1]]);
            println!("  USB mode = {} (expect 6 for RAW CONV)", mode);
            if mode != 6 {
                eprintln!("\n  WARNING: Camera may not be in RAW CONV mode.");
                eprintln!(
                    "  Set camera to: Connection Setting -> Connection Mode -> USB RAW CONV./BACKUP RESTORE"
                );
                eprintln!("  Then reconnect USB and retry.\n");
            }
        }
        _ => println!("  Could not read 0xD16E (connection mode)"),
    }

    // ------------------------------------------------------------------
    // Process each file
    // ------------------------------------------------------------------
    let mut succeeded = 0u32;
    let mut failed = 0u32;

    for (idx, (input, output, raf_data)) in validated.iter().enumerate() {
        let file_num = idx + 1;
        let raf_size_mb = raf_data.len() as f64 / (1024.0 * 1024.0);

        if total > 1 {
            println!("\n{}", "=".repeat(60));
            println!("[{file_num}/{total}] {input}");
            println!("{}", "=".repeat(60));
        }
        println!("Input:  {input}");
        println!("Output: {output}");
        println!("RAF:    {:.1} MB\n", raf_size_mb);

        match convert_one(&mut camera, raf_data, raf_size_mb, output, recipe) {
            Ok(()) => {
                succeeded += 1;
                println!("\n  Conversion complete!");
            }
            Err(e) => {
                failed += 1;
                eprintln!("\n  FAILED: {e}");
                if failed > 0 && file_num < total {
                    eprintln!("  Continuing with next file...");
                }
            }
        }
    }

    let _ = camera.close_session();

    if total > 1 {
        println!("\n{} succeeded, {} failed out of {} files.", succeeded, failed, total);
    }
    if failed > 0 {
        std::process::exit(1);
    }
}

/// Process a single RAF through the camera. Returns Err on failure so the
/// caller can continue with the next file instead of aborting.
fn convert_one(
    camera: &mut PtpCamera,
    raf_data: &[u8],
    raf_size_mb: f64,
    output_path: &str,
    recipe: &RecipeSettings,
) -> Result<(), String> {
    // Step A: Send ObjectInfo via 0x900C
    println!("[1/6] Sending ObjectInfo (0x900C)...");
    let obj_info = ptp::build_object_info(
        PTP_OFC_FUJI_RAW_UPLOAD,
        raf_data.len() as u32,
        "FUP_FILE.dat",
    );
    let resp = camera.vendor_send(FUJI_OC_SEND_OBJECT_INFO, &[0, 0, 0], &obj_info)?;
    println!(
        "  -> 0x{:04X} ({})",
        resp.code,
        ptp::response_name(resp.code)
    );
    if resp.code != RC_OK {
        return Err("0x900C failed. Is the camera in USB RAW CONV mode?".into());
    }

    // Step B: Send RAF file data via 0x900D
    println!("[2/6] Uploading RAF ({:.1} MB via 0x900D)...", raf_size_mb);
    let resp = camera.vendor_send(FUJI_OC_SEND_OBJECT, &[], raf_data)?;
    println!(
        "  -> 0x{:04X} ({})",
        resp.code,
        ptp::response_name(resp.code)
    );
    if resp.code != RC_OK {
        return Err("0x900D (SendObject) failed.".into());
    }

    // Step C/D: Get development profile, apply recipe, set it back
    println!("[3/6] Reading development profile (0xD185)...");
    let mut profile_data = match camera.get_device_prop_value(FUJI_PROP_RAW_CONV_PROFILE) {
        Ok(data) => {
            println!("  Got {} bytes of profile data", data.len());
            println!("  Current film sim: {}", profile::current_film_sim(&data));
            profile::dump_profile(&data);
            data
        }
        Err(e) => {
            eprintln!("  Warning: could not read profile: {e}");
            eprintln!("  Proceeding without profile (camera will use defaults)");
            Vec::new()
        }
    };

    if !profile_data.is_empty() {
        if !recipe.is_empty() {
            println!("\n  Applying recipe: {}", recipe.summary());
            let changes = profile::apply_recipe(&mut profile_data, recipe);
            println!("  {changes}");
        }

        println!("[4/6] Setting development profile (0xD185)...");
        match camera.set_device_prop_value(FUJI_PROP_RAW_CONV_PROFILE, &profile_data) {
            Ok(()) => println!("  -> OK"),
            Err(e) => eprintln!("  Warning: could not set profile: {e}"),
        }
    } else {
        println!("[4/6] Skipping profile set (no profile data)");
    }

    // Step E: Trigger conversion by setting 0xD183 = 0
    println!("[5/6] Triggering RAW conversion (Set 0xD183 = 0)...");
    match camera.set_device_prop_value(FUJI_PROP_START_RAW_CONV, &0u32.to_le_bytes()) {
        Ok(()) => println!("  -> OK"),
        Err(e) => {
            println!("  u32 failed ({e}), trying u16...");
            camera
                .set_device_prop_value(FUJI_PROP_START_RAW_CONV, &0u16.to_le_bytes())
                .map_err(|e2| format!("Failed to trigger conversion: {e2}"))?;
            println!("  -> OK (u16)");
        }
    }

    // Step F/G: Poll GetObjectHandles until the JPEG appears, then download
    println!("[6/6] Waiting for converted image...");
    let poll_interval = std::time::Duration::from_secs(2);
    let max_polls = 45;

    for attempt in 1..=max_polls {
        std::thread::sleep(poll_interval);
        print!("  Poll {attempt}/{max_polls}... ");

        match camera.get_object_handles(0xFFFFFFFF, 0, 0) {
            Ok(handles) if !handles.is_empty() => {
                println!("found {} object(s): {:08X?}", handles.len(), handles);

                let handle = handles[0];
                println!("  Downloading object 0x{handle:08X}...");
                match camera.get_object(handle) {
                    Ok(data) => {
                        let size_mb = data.len() as f64 / (1024.0 * 1024.0);
                        println!("  Got {:.1} MB", size_mb);

                        if data.len() >= 2 && data[0] == 0xFF && data[1] == 0xD8 {
                            println!("  JPEG signature verified!");
                        } else {
                            println!("  Warning: data doesn't start with JPEG magic (FF D8)");
                            print_hex_dump(&data, 64);
                        }

                        fs::write(output_path, &data).unwrap_or_else(|e| {
                            eprintln!("  Error writing {output_path}: {e}");
                            std::process::exit(1);
                        });
                        println!("\n  Saved: {output_path} ({:.1} MB)", size_mb);

                        println!("  Cleaning up (DeleteObject)...");
                        match camera.delete_object(handle) {
                            Ok(_) => println!("  Object deleted."),
                            Err(e) => println!("  Could not delete object: {e}"),
                        }

                        return Ok(());
                    }
                    Err(e) => {
                        println!("  GetObject failed: {e}");
                    }
                }
            }
            Ok(_) => println!("no objects yet"),
            Err(e) => println!("error: {e}"),
        }
    }

    Err("Timed out waiting for the camera to produce the converted image.".into())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn open_camera() -> PtpCamera {
    PtpCamera::open(FUJIFILM_VENDOR_ID, DEFAULT_PRODUCT_ID).unwrap_or_else(|e| {
        eprintln!("Error: {e}");
        eprintln!();
        eprintln!("Make sure:");
        eprintln!("  - The camera is connected via USB and turned on");
        eprintln!("  - No other app (Photos, Image Capture) is using it");
        eprintln!("  - USB mode is set correctly on the camera");
        eprintln!("  - You have run `sudo fjx setup` (one-time macOS PTP daemon fix)");
        std::process::exit(1);
    })
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
