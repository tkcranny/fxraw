# Research: Prior Attempts to Reverse-Engineer the Fuji USB Protocol and X Raw Studio JPEG Conversion

## 1. X Raw Studio – What It Is and How It Works

**X Raw Studio** (Fujifilm’s official name; often written “X RAW STUDIO”) is Fujifilm’s proprietary RAW development application. It converts RAF (Fujifilm RAW) files to JPEG using the **camera’s image processor over USB**, not the host computer’s CPU.

### User-visible flow

1. **Camera setup**
   - **USB POWER SUPPLY/COMM SETTING**: either **AUTO** or **POWER SUPPLY OFF/COMM ON**
   - **CONNECTION MODE**: **USB RAW CONV./BACKUP RESTORE**
   - Camera connected via USB and powered on

2. **Conversion**
   - User selects RAF(s) in X Raw Studio and applies Film Simulations / settings.
   - The host sends RAF data to the camera over USB.
   - The camera runs its own RAW pipeline (same as in-camera conversion) and returns the result (e.g. JPEG).
   - Conversion time is roughly “one shot per image” and does not depend on PC performance.

3. **Output**
   - JPEG files (and optionally .FP1/.FP2/.FP3 sidecar-style files from the app).

### Technical summary

- **Protocol**: USB; device class is consistent with **PTP (Picture Transfer Protocol)** with Fujifilm **vendor extensions** (0x9xxx operation codes).
- **Data flow**: Host sends RAF (and likely metadata/parameters) to camera; camera processes and returns image data (e.g. JPEG).
- **Model matching**: RAW files are converted by the **same or compatible camera model**; the camera’s firmware defines the supported RAW pipeline and Film Simulations.

Fujifilm does **not** publish the vendor PTP opcodes or payload formats; the exact sequence (which opcode sends RAF, which requests JPEG, what payloads look like) has been inferred only by reverse engineering and traffic observation.

---

## 2. Prior Reverse-Engineering Efforts

### 2.1 FujiHack (firmware + USB/PTP)

- **Repo**: [fujihack/fujihack](https://github.com/fujihack/fujihack)
- **Focus**: Firmware reverse engineering, code execution on camera, and **PTP/USB** for debugger and file upload.

**USB/PTP usage in FujiHack (from `usb/ptp.c`):**

| Opcode   | FujiHack name      | FujiHack use                                                                                           |
| -------- | ------------------ | ------------------------------------------------------------------------------------------------------ |
| `0x900C` | `FUJI_CREATE_FILE` | Send ObjectInfo-like metadata (storage, type, size, filename); used to “create” a file before writing. |
| `0x900D` | `FUJI_UNKNOWN1`    | Comment: “mostly similar to 901d”. Not used for RAW in their code.                                     |
| `0x901D` | `FUJI_WRITE_FILE`  | Send file payload (e.g. script binary) after 0x900C.                                                   |

So in FujiHack’s world, **0x900C = create/metadata**, **0x901D = write payload**. That matches a “SendObjectInfo + SendObject” style flow for **uploading** data to the camera (e.g. scripts), not necessarily the same semantic as “send RAF, get JPEG” in X Raw Studio. X Raw Studio may use the same opcodes with different payloads and a different sequence (e.g. one opcode for “get result” with data-in).

**Other:**

- **0x9805** = `FUJI_HIJACK`: custom debugger protocol (read/write/exec) used with patched firmware.
- FujiHack also has **minptp** (minimal PTP stack) and **usb** (libusb + PTP) for talking to the camera.

FujiHack does **not** document or implement the X Raw Studio RAW→JPEG flow; their 0x900C/0x900D/0x901D usage is for file upload and debugger, not for RAW conversion.

### 2.2 fuji-cam-wifi-tool (WiFi, not USB)

- **Repo**: [hkr/fuji-cam-wifi-tool](https://github.com/hkr/fuji-cam-wifi-tool)
- **Focus**: Reverse engineering of the **WiFi** remote control protocol (e.g. X-T10).
- Covers: shutter, ISO, aperture, white balance, live view.
- **Not** related to USB or RAW conversion.

### 2.3 Fudge / libpict (C – implements RAF→JPEG over USB)

**Repos:** [petabyt/fudge](https://github.com/petabyt/fudge), [petabyt/libpict](https://github.com/petabyt/libpict). Fudge implements **full USB RAF→JPEG conversion** in RAW CONV. mode (same as X Raw Studio).

**Protocol:** 0x900C (SendObjectInfo: ObjectInfo with format **0xf802**, filename `"FUP_FILE.dat"`) → 0x900D (SendObject: full RAF) → **Get** then **Set** property **0xD185** (RawConvProfile) → **Set** property **0xD183** (StartRawConversion) to **0** → poll **GetObjectHandles** → **GetObject**(handle) for JPEG → **DeleteObject**(handle). The result is **not** 0x901D; it is standard PTP GetObject. See **[docs/PROTOCOL.md](PROTOCOL.md)** for the full step-by-step.

### 2.4 This project (fjx)

- **Goal**: Detect Fuji X100VI over USB and perform on-camera RAF→JPEG conversion like X Raw Studio.
- **Approach**: Implement PTP session + Fuji vendor ops; probe device (operations, properties, formats); try sequences of 0x900C / 0x900D / 0x901D to match X Raw Studio behavior.

**Current interpretation of vendor ops (see `src/fuji.rs`):**

| Opcode   | Role (hypothesis)                                      |
| -------- | ------------------------------------------------------ |
| `0x900C` | SendObjectInfo / RAW send info (metadata or full RAF). |
| `0x900D` | SendObject / RAW send data (trigger or send RAF).      |
| `0x901D` | GetObject / get processed result (JPEG).               |

**Correct flow (from Fudge):** JPEG is **not** received via 0x901D. Use **0x900C** (ObjectInfo, format 0xf802) → **0x900D** (full RAF) → **0xD185** (RawConvProfile) → **Set 0xD183 = 0** (StartRawConversion) → **GetObjectHandles** (poll) → **GetObject** → **DeleteObject**. See **[PROTOCOL.md](PROTOCOL.md)** for the full step-by-step.

**Relevant device properties (from probe):**

- `0xD34D` – RawDevelopMode
- `0xD36A` – RawConvVersion
- `0xD36B` – RawConvParam
- `0xD21C` – USB mode
- **0xD185** – RawConvProfile (get/set development settings; Fudge)
- **0xD183** – StartRawConversion (set to 0 to trigger; Fudge)
- **0xD16E** – USBMode (6 = RAW CONV; Fudge)

**Object format:** Fuji RAF = `0xB103`; for upload in RAW CONV mode Fudge uses **0xf802** and filename `"FUP_FILE.dat"`.

---

## 3. How the X100VI USB RAW conversion is done

The sequence below is **reverse‑engineered from Fudge** (same flow as X Raw Studio). Full detail: **[PROTOCOL.md](PROTOCOL.md)**.

1. **Mode**
   - Camera is set to **USB RAW CONV./BACKUP RESTORE** so the device exposes the RAW conversion service over USB (likely different PTP vendor behavior than “normal” USB storage/MTP).

2. **Send RAF** – **0x900C** (params 0,0,0; data = ObjectInfo with format **0xf802**, size, filename "FUP_FILE.dat") then **0x900D** (data = full RAF).

3. **Profile and trigger** – **Get** **0xD185** (RawConvProfile); **Set** 0xD185 (camera default or merged); **Set** **0xD183** (StartRawConversion) to **0**.

4. **Get result** – **Poll** **GetObjectHandles** until a new object appears; **GetObject**(handle) for the JPEG; **DeleteObject**(handle). Result is **not** via 0x901D.

5. **Constraints** – RAF must be from a compatible camera model.

---

## 4. Protocol and Format References

- **Full conversion sequence:** [PROTOCOL.md](PROTOCOL.md) (X100VI / Fudge step-by-step).
- **PTP**: Standard Still Image Device class; vendor extensions in the **0x9xxx** range.
- **Fujifilm RAF**:
  - [Fujifilm RAF (libopenraw)](https://libopenraw.freedesktop.org/formats/raf/)
  - Big-endian; structure with embedded JPEG and raw sensor data; newer X-series may use compression.
- **USB capture**:
  - **Linux**: usbmon + Wireshark (PTP dissector); see [Wireshark USB PTP](https://wiki.wireshark.org/USB-PTP) and [Sniffing from USB ports](https://wiki.wireshark.org/USB).
  - Filter for bulk transfers and PTP command/response/data phases; look for **Operation Code** in the **0x9xxx** range to see Fuji vendor ops used by X Raw Studio.

---

## 5. Summary Table

| Source                 | Focus              | USB/PTP vendor ops                                      | RAW→JPEG conversion                  |
| ---------------------- | ------------------ | ------------------------------------------------------- | ------------------------------------ |
| **X Raw Studio**       | Official app       | Not documented                                          | Yes (design goal)                    |
| **FujiHack**           | Firmware + USB     | 0x900C, 0x900D, 0x901D (upload/debugger); 0x9805 hijack | No                                   |
| **Fudge / libpict**    | C PTP + Fuji       | 0x900C, 0x900D; 0xD185, 0xD183; GetObject for result    | **Yes** ([PROTOCOL.md](PROTOCOL.md)) |
| **fuji-cam-wifi-tool** | WiFi remote        | N/A                                                     | No                                   |
| **fjx**                | USB PTP + Fuji ops | Aligning with Fudge sequence (see PROTOCOL.md)          | In progress                          |

**Conclusion:** The **Fudge** project reverse‑engineered the full RAF→JPEG-over-USB sequence: 0x900C (ObjectInfo) → 0x900D (RAF) → 0xD185 (profile) → 0xD183 = 0 (start) → GetObjectHandles → GetObject → DeleteObject. The JPEG is retrieved via **standard PTP GetObject**, not vendor 0x901D. See **[PROTOCOL.md](PROTOCOL.md)** for the step-by-step; fjx should implement that sequence (format 0xf802, filename "FUP_FILE.dat", properties 0xD185 and 0xD183).
