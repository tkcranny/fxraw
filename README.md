# fuji-usb-test

Detect and interact with a Fujifilm X100VI camera over USB. The end goal is
on-camera RAF-to-JPEG conversion, like Fuji X RAW Studio.

## Prerequisites

- [Rust](https://rustup.rs/) (1.85+)
- macOS
- USB cable connected to a Fujifilm X100VI

## Commands

### detect

Check whether the camera is plugged in and powered on.

```
cargo run -- detect
```

### probe

Open a PTP session and dump everything the camera reports: supported
operations, device properties, image formats, and vendor extensions.
This is the key step for reverse-engineering the RAW conversion protocol.

```
cargo run -- probe
```

### convert

Convert a RAF file to JPEG using the camera's image processor.

```
cargo run -- convert photo.raf
cargo run -- convert photo.raf -o output.jpg
cargo run -- convert photo.raf -f acros -g weak -o acros.jpg
```

#### Recipe options

| Flag | Values |
|------|--------|
| `-f, --film-sim` | provia, velvia, astia, classic-chrome, classic-neg, pro-neg-hi, pro-neg-std, eterna, eterna-bleach-bypass, acros, acros-ye, acros-r, acros-g, monochrome, monochrome-ye, monochrome-r, monochrome-g, sepia, nostalgic-neg, reala-ace |
| `-g, --grain` | off, weak, strong |

Without recipe flags the camera's current settings are used (same as
X RAW Studio's default behaviour).

## Camera setup

1. Connect the camera to your Mac via USB.
2. Turn the camera on.
3. Go to **Menu > Connection Setting > Connection Mode** and set it to **USB**.

## Reverse-engineering the protocol

The `probe` command dumps all PTP operations and properties the camera
supports. The next step is to capture USB traffic while Fuji X RAW Studio
processes a RAF file:

1. Install [Wireshark](https://www.wireshark.org/).
2. Start a capture on the **XHC20** (USB) interface.
3. Open Fuji X RAW Studio and convert a single RAF.
4. Stop the capture and filter for USB bulk transfers.
5. Look for PTP vendor operation codes (0x9XXX) in the command containers.
6. Update `src/fuji.rs` with the discovered sequence.

## Project structure

```
src/
  main.rs     CLI entry point (detect / probe / convert)
  detect.rs   USB device scanning
  ptp.rs      PTP-over-USB protocol layer
  fuji.rs     Fujifilm-specific PTP operations
  profile.rs  D185 profile parsing and recipe settings
```
