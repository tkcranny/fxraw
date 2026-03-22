# Fujifilm X100VI USB RAW Conversion Protocol

This document describes **how RAF→JPEG conversion over USB is done** for the Fujifilm X100VI (and compatible X/GFX cameras) when the camera is in **USB RAW CONV./BACKUP RESTORE** mode. The sequence is reverse‑engineered from the [Fudge](https://github.com/petabyt/fudge) project (C, `lib/fuji_usb.c`, `lib/fujiptp.h`), which implements the same flow as Fuji X Raw Studio.

---

## 1. Overview

- **Transport:** USB with **PTP (Picture Transfer Protocol)**. Fujifilm uses **vendor operation codes** (0x9xxx) and **vendor device properties** (0xDxxx) for RAW conversion.
- **Roles:** The computer is the PTP **initiator**; the camera is the PTP **responder**. The camera must be set to connection mode **USB RAW CONV./BACKUP RESTORE** (USB mode value **6**, property `0xD16E`).
- **Flow in short:** Send the RAF to the camera with two vendor ops (0x900C then 0x900D), set the development profile (0xD185) and trigger conversion (0xD183), then **retrieve the result with standard PTP** (GetObjectHandles → GetObject → DeleteObject). The result is **not** read via vendor 0x901D.

---

## 2. Step-by-step sequence

### 2.1 Prerequisites

- Camera powered on, connected via USB.
- **Connection mode:** **USB RAW CONV./BACKUP RESTORE** (menu: Connection Setting → Connection Mode).
- **USB mode property** `0xD16E` = 6 (RAW CONV). You can read it to confirm.
- PTP session already **open** (OpenSession).

### 2.2 Send the RAF file to the camera

The host sends the **entire RAF file** in two steps: first an **ObjectInfo** (metadata), then the **raw bytes**.

**Step A – Fuji SendObjectInfo (0x900C)**

- **Operation code:** `0x900C` (Fuji vendor; Fudge name: `PTP_OC_FUJI_SendObjectInfo`).
- **Parameters:** `(storage_id, handle, 0)`. Fudge uses `storage_id = 0`, `handle = 0`, third param `0`.
- **Data phase (out):** A **PTP ObjectInfo** structure sent to the device. Fudge builds it with:
  - **ObjectFormat:** `0xf802` (Fuji-specific; not the standard RAF code 0xB103).
  - **CompressedSize:** RAF file size in bytes.
  - **Filename:** `"FUP_FILE.dat"` (Fudge’s choice; the camera may accept other names).
  - Other ObjectInfo fields (StorageID, ProtectionStatus, thumb fields, image dimensions, etc.) as in standard PTP; many can be zero for this use.

So: **0x900C** with three params and a **data-out** payload = full ObjectInfo describing the RAF (format 0xf802, size, filename).

**Step B – Fuji SendObject (0x900D)**

- **Operation code:** `0x900D` (Fudge name: `PTP_OC_FUJI_SendObject2`).
- **Parameters:** Fudge uses **none** (param_length = 0).
- **Data phase (out):** The **entire RAF file** (raw bytes).

Fudge increases the PTP/USB **max packet size** (e.g. to 261632) for this transfer to speed up the upload. After the transfer, it restores the previous packet size.

So: **0x900D** with **data-out** = send the RAF payload. After this, the camera has the RAF in memory for conversion.

### 2.3 Set development parameters and trigger conversion

**Step C – Get current profile (optional but recommended)**

- **Get** device property **0xD185** (`PTP_DPC_FUJI_RawConvProfile`).
- The camera returns a **binary profile** (“d185” format) that describes Film Simulation, tone, WB, etc. You can use this as-is for “camera default” conversion.

**Step D – Optionally modify and set profile**

- Parse the profile (Fudge: `fp_parse_d185`), optionally merge with user settings from an FP1 (XML) file (`fp_parse_fp1`, `fp_apply_profile`), then re-encode (`fp_create_d185`).
- **Set** device property **0xD185** with the (possibly merged) profile data. This tells the camera how to develop the RAW (Film Sim, exposure, etc.).

If you skip merge, you can **Set 0xD185** with the same bytes you got from **Get 0xD185** to keep camera defaults.

**Step E – Start conversion**

- **Set** device property **0xD183** (`PTP_DPC_FUJI_StartRawConversion`) to value **0**.
- This **triggers** the camera to run its internal RAW pipeline on the RAF you sent and produce the output image (e.g. JPEG).

### 2.4 Retrieve the result (JPEG)

The camera does **not** return the JPEG via a vendor opcode. It creates a **new object** in its (virtual) store and the host retrieves it with **standard PTP**:

**Step F – Poll until the object appears**

- Call **GetObjectHandles** with:
  - StorageID = **0xFFFFFFFF** (all stores) or the appropriate storage;
  - ObjectFormat = 0 (any);
  - Association = 0 (no association).
- Repeat until the returned handle list has **length > 0**. The camera adds the processed image as an object when conversion is done (may take several seconds).

**Step G – Download the object**

- **GetObject**(handle) with the first (or only) handle from the list. The **data phase (in)** is the image file (e.g. JPEG).
- Save or process that payload as the converted image.

**Step H – Clean up (optional)**

- **DeleteObject**(handle) so the camera doesn’t keep the object in its store.

---

## 3. Summary: opcodes and properties

| Code                 | Type         | Name (Fudge)       | Role                                                                                     |
| -------------------- | ------------ | ------------------ | ---------------------------------------------------------------------------------------- |
| **0x900C**           | Vendor op    | SendObjectInfo     | Send ObjectInfo for RAF (format 0xf802, size, filename); params (storage_id, handle, 0). |
| **0x900D**           | Vendor op    | SendObject2        | Send RAF file bytes (data-out).                                                          |
| **0x901D**           | Vendor op    | SendObject         | **Not** used for receiving the JPEG; in Fudge/FujiHack used for “write file” (upload).   |
| **0xD185**           | Prop         | RawConvProfile     | Get = read current profile; Set = set profile used for conversion (Film Sim, etc.).      |
| **0xD183**           | Prop         | StartRawConversion | Set to **0** = start conversion after RAF + profile are set.                             |
| **0xD16E**           | Prop         | USBMode            | Read to confirm mode; **6** = RAW CONV.                                                  |
| **GetObjectHandles** | Standard PTP | —                  | Poll until a new object appears (converted image).                                       |
| **GetObject**        | Standard PTP | —                  | Download the image (JPEG).                                                               |
| **DeleteObject**     | Standard PTP | —                  | Remove the object from the camera.                                                       |

---

## 4. ObjectInfo format for 0x900C

Fudge sends a **full PTP ObjectInfo** in the 0x900C data phase. Layout (same as ISO 15740 PTP):

- StorageID (4), ObjectFormat (2), ProtectionStatus (2), ObjectCompressedSize (4)
- ThumbFormat, ThumbCompressedSize, ThumbPixWidth, ThumbPixHeight (2+4+4+4)
- ImagePixWidth, ImagePixHeight, ImageBitDepth (4+4+4)
- ParentObject, AssociationType, AssociationDesc, SequenceNumber (4+2+4+4)
- Filename (PTP string), CaptureDate, ModificationDate, Keywords (PTP strings)

For RAW conversion, Fudge sets:

- **ObjectFormat = 0xf802** (Fuji RAF upload format).
- **ObjectCompressedSize =** RAF file size.
- **Filename = "FUP_FILE.dat"**.

Other fields can be zero or minimal. Our project’s `ptp::build_object_info()` can be used with **format 0xf802** and filename `"FUP_FILE.dat"` to match Fudge; the exact layout must follow PTP (e.g. our existing builder with the right format code and size).

---

## 5. Profile (0xD185)

The **RawConvProfile** is a binary blob that controls development settings (Film Simulation, highlight/shadow, WB, etc.). Fudge:

- **Gets** it from the camera to read current/camera default.
- Can **merge** with user FP1 (XML) and **Set** it back so conversion uses custom settings.

For a first implementation you can **Get 0xD185** and then **Set 0xD185** with the same bytes (no merge) to use camera defaults. Full FP1 parsing/merge is optional.

---

## 6. Implementation checklist for fxraw

1. **Camera in RAW CONV mode** – user sets menu; optionally read 0xD16E and assert value 6.
2. **0x900C** – Params `(0, 0, 0)`, data = ObjectInfo with **format 0xf802**, size = RAF length, filename **"FUP_FILE.dat"** (and full PTP ObjectInfo layout).
3. **0x900D** – Params empty, data = **full RAF file**; consider increasing max packet size for bulk send.
4. **Get 0xD185** – read profile; optionally **Set 0xD185** (same or merged) before starting.
5. **Set 0xD183 = 0** – trigger conversion.
6. **Poll GetObjectHandles** (storage 0xFFFFFFFF or 0, format 0, assoc 0) until at least one handle.
7. **GetObject**(handle) – receive JPEG (or processed image).
8. **DeleteObject**(handle) – optional cleanup.

The **JPEG is not** read via 0x901D; it is read via standard **GetObject** after the camera creates the object. This matches Fudge and is the correct flow for X100VI (and compatible models) USB RAW conversion.
