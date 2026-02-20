use rusb::{Context, UsbContext};
use std::time::Duration;

use crate::fuji::{FUJIFILM_VENDOR_ID, X100VI_PRODUCT_IDS};

pub fn run() {
    println!("Scanning for Fujifilm X100VI on USB...\n");

    let context = Context::new().expect("Failed to initialize libusb context");

    let devices = match context.devices() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Failed to enumerate USB devices: {e}");
            std::process::exit(1);
        }
    };

    let mut found_fuji = false;
    let mut found_x100vi = false;

    for device in devices.iter() {
        let desc = match device.device_descriptor() {
            Ok(d) => d,
            Err(_) => continue,
        };

        if desc.vendor_id() != FUJIFILM_VENDOR_ID {
            continue;
        }

        found_fuji = true;
        let product_id = desc.product_id();

        let product_name = device.open().ok().and_then(|handle| {
            handle
                .read_languages(Duration::from_secs(1))
                .ok()
                .and_then(|langs| langs.into_iter().next())
                .and_then(|lang| {
                    handle
                        .read_product_string(lang, &desc, Duration::from_secs(1))
                        .ok()
                })
        });

        let name_str = product_name.as_deref().unwrap_or("(unable to read name)");

        let is_x100vi = X100VI_PRODUCT_IDS.iter().any(|(pid, _)| *pid == product_id);
        let name_matches = name_str.to_uppercase().contains("X100VI");

        if is_x100vi || name_matches {
            found_x100vi = true;
            println!("*** Fujifilm X100VI DETECTED! ***");
            println!("  Product:    {name_str}");
            println!("  Vendor ID:  0x{:04X}", desc.vendor_id());
            println!("  Product ID: 0x{product_id:04X}");
            println!(
                "  Bus {:03} Device {:03}",
                device.bus_number(),
                device.address()
            );
        } else {
            println!("Found Fujifilm device (not X100VI):");
            println!("  Product:    {name_str}");
            println!("  Vendor ID:  0x{:04X}", desc.vendor_id());
            println!("  Product ID: 0x{product_id:04X}");
            println!(
                "  Bus {:03} Device {:03}",
                device.bus_number(),
                device.address()
            );
            println!("  TIP: If this IS your X100VI, note the Product ID above");
            println!("       and add it to X100VI_PRODUCT_IDS in fuji.rs.\n");
        }
    }

    if !found_fuji {
        println!("No Fujifilm USB devices found.");
        println!();
        println!("Troubleshooting:");
        println!("  1. Make sure the camera is turned ON");
        println!("  2. Check the USB cable is connected");
        println!("  3. On the camera, set CONNECTION MODE to USB (not Bluetooth)");
        println!("     Menu -> Connection Setting -> Connection Mode");
        println!("  4. Try setting USB mode to 'USB CARD READER' or 'USB TETHER SHOOTING AUTO'");
    } else if !found_x100vi {
        println!();
        println!("A Fujifilm device was found but not identified as X100VI.");
        println!("Check the Product ID printed above and add it to the");
        println!("X100VI_PRODUCT_IDS list in src/fuji.rs if it is your camera.");
    }
}
