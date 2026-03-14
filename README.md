# fjx

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

### recipes

List all 132 built-in recipe presets.

```
cargo run -- recipes
```

### convert

Convert one or more RAF files to JPEG using the camera's image processor.
When multiple files are given, the camera session is opened once and reused.

```
cargo run -- convert photo.raf
cargo run -- convert photo.raf -o output.jpg
cargo run -- convert photo.raf -r kodak-tri-x-400
cargo run -- convert photo.raf -r "kodak gold" -o gold.jpg
cargo run -- convert photo.raf -f acros -g weak -o acros.jpg
cargo run -- convert photo.raf -r portra-400 -g strong --grain-size large -o grainy.jpg
cargo run -- convert *.raf -r portra-400
cargo run -- convert *.raf -r classic-chrome -o jpegs/
```

#### Options

| Flag | Description |
|------|-------------|
| `-o, --output <PATH>` | Output JPEG path or directory (created if needed; defaults to `<input>-<recipe>.jpg`) |
| `-r, --recipe <NAME>` | Use a built-in recipe preset (exact slug or partial match on slug/name) |
| `-f, --film-sim <SIM>` | Film simulation override (takes priority over recipe) |
| `-g, --grain <LEVEL>` | Grain effect override: `off`, `weak`, `strong` |
| `--grain-size <SIZE>` | Grain size when grain is on: `small`, `large` |

#### Recipes

The `-r` flag accepts a recipe slug (e.g. `kodak-tri-x-400`) or a partial
match on the slug or name (e.g. `tri-x`, `portra`, `cinestill 800t`).
Run `recipes` to see the full list.

Each recipe sets some combination of: film simulation, grain, highlight/shadow
tone, color, sharpness, noise reduction, clarity, white balance, and WB shift.
The `-f` and `-g` flags override the recipe's film simulation and grain if
both are given.

#### Film simulations

provia, velvia, astia, classic-chrome, classic-neg, pro-neg-hi, pro-neg-std,
eterna, eterna-bleach-bypass, acros, acros-ye, acros-r, acros-g, monochrome,
monochrome-ye, monochrome-r, monochrome-g, sepia, nostalgic-neg, reala-ace

#### Grain

off, weak, strong

Without any recipe or override flags the camera's current settings are used
(same as X RAW Studio's default behaviour).

## Camera setup

1. Connect the camera to your Mac via USB.
2. Turn the camera on.
3. Go to **Menu > Connection Setting > Connection Mode** and set it to **USB**.

### One-time macOS setup (disabling ptpcamerad)

macOS automatically runs a system daemon (`ptpcamerad`) that claims every PTP
camera. This blocks `fjx` from communicating with the camera over USB.

Run this once to permanently disable the daemon:

```
sudo cargo run -- setup
```

After that, all `fjx` commands work without `sudo`. To undo this later:

```
sudo cargo run -- setup --undo
```

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
  main.rs     CLI entry point (detect / probe / recipes / convert)
  detect.rs   USB device scanning
  ptp.rs      PTP-over-USB protocol layer
  fuji.rs     Fujifilm-specific PTP operations
  profile.rs  D185 profile parsing and recipe settings
  recipes.rs  Built-in recipe presets (loaded from data/recipes.json)
data/
  recipes.json  132 film simulation recipes
```
