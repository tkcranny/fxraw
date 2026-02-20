#![allow(dead_code)]

use rusb::{Context, DeviceHandle, Direction, TransferType, UsbContext};
use std::time::Duration;

// ---------------------------------------------------------------------------
// PTP container types and header
// ---------------------------------------------------------------------------

const PTP_HEADER_LEN: usize = 12;
const CONTAINER_COMMAND: u16 = 1;
const CONTAINER_DATA: u16 = 2;
const CONTAINER_RESPONSE: u16 = 3;

// ---------------------------------------------------------------------------
// Standard PTP operation codes (ISO 15740)
// ---------------------------------------------------------------------------

const OC_GET_DEVICE_INFO: u16 = 0x1001;
const OC_OPEN_SESSION: u16 = 0x1002;
const OC_CLOSE_SESSION: u16 = 0x1003;
const OC_GET_STORAGE_IDS: u16 = 0x1004;
const OC_GET_STORAGE_INFO: u16 = 0x1005;
const OC_GET_NUM_OBJECTS: u16 = 0x1006;
const OC_GET_OBJECT_HANDLES: u16 = 0x1007;
const OC_GET_OBJECT_INFO: u16 = 0x1008;
const OC_GET_OBJECT: u16 = 0x1009;
const OC_GET_THUMB: u16 = 0x100A;
const OC_DELETE_OBJECT: u16 = 0x100B;
const OC_SEND_OBJECT_INFO: u16 = 0x100C;
const OC_SEND_OBJECT: u16 = 0x100D;
const OC_INITIATE_CAPTURE: u16 = 0x100E;
const OC_FORMAT_STORE: u16 = 0x100F;
const OC_RESET_DEVICE: u16 = 0x1010;
const OC_SELF_TEST: u16 = 0x1011;
const OC_SET_OBJECT_PROTECTION: u16 = 0x1012;
const OC_POWER_DOWN: u16 = 0x1013;
const OC_GET_DEVICE_PROP_DESC: u16 = 0x1014;
const OC_GET_DEVICE_PROP_VALUE: u16 = 0x1015;
const OC_SET_DEVICE_PROP_VALUE: u16 = 0x1016;
const OC_RESET_DEVICE_PROP_VALUE: u16 = 0x1017;
const OC_TERMINATE_CAPTURE: u16 = 0x1018;
const OC_INITIATE_OPEN_CAPTURE: u16 = 0x101B;

// ---------------------------------------------------------------------------
// PTP response codes
// ---------------------------------------------------------------------------

pub const RC_OK: u16 = 0x2001;
const RC_GENERAL_ERROR: u16 = 0x2002;
const RC_SESSION_NOT_OPEN: u16 = 0x2003;
const RC_INVALID_TRANSACTION_ID: u16 = 0x2004;
const RC_OPERATION_NOT_SUPPORTED: u16 = 0x2005;
const RC_PARAMETER_NOT_SUPPORTED: u16 = 0x2006;
const RC_INCOMPLETE_TRANSFER: u16 = 0x2007;
const RC_INVALID_STORAGE_ID: u16 = 0x2008;
const RC_INVALID_OBJECT_HANDLE: u16 = 0x2009;
const RC_DEVICE_PROP_NOT_SUPPORTED: u16 = 0x200A;
const RC_INVALID_OBJECT_FORMAT_CODE: u16 = 0x200B;
const RC_STORE_FULL: u16 = 0x200C;
const RC_OBJECT_WRITE_PROTECTED: u16 = 0x200D;
const RC_STORE_READ_ONLY: u16 = 0x200E;
const RC_ACCESS_DENIED: u16 = 0x200F;
const RC_NO_THUMBNAIL_PRESENT: u16 = 0x2010;
const RC_SELF_TEST_FAILED: u16 = 0x2011;
const RC_PARTIAL_DELETION: u16 = 0x2012;
const RC_STORE_NOT_AVAILABLE: u16 = 0x2013;
const RC_SPEC_BY_FORMAT_UNSUPPORTED: u16 = 0x2014;
const RC_NO_VALID_OBJECT_INFO: u16 = 0x2015;
const RC_DEVICE_BUSY: u16 = 0x2019;
const RC_INVALID_DEVICE_PROP_FORMAT: u16 = 0x201A;
const RC_INVALID_DEVICE_PROP_VALUE: u16 = 0x201C;
const RC_SESSION_ALREADY_OPEN: u16 = 0x201E;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

pub struct PtpResponse {
    pub code: u16,
    pub params: Vec<u32>,
}

pub struct DeviceInfo {
    pub standard_version: u16,
    pub vendor_extension_id: u32,
    pub vendor_extension_version: u16,
    pub vendor_extension_desc: String,
    pub functional_mode: u16,
    pub operations_supported: Vec<u16>,
    pub events_supported: Vec<u16>,
    pub device_properties_supported: Vec<u16>,
    pub capture_formats: Vec<u16>,
    pub image_formats: Vec<u16>,
    pub manufacturer: String,
    pub model: String,
    pub device_version: String,
    pub serial_number: String,
}

// ---------------------------------------------------------------------------
// PtpCamera – PTP-over-USB transport
// ---------------------------------------------------------------------------

pub struct PtpCamera {
    handle: DeviceHandle<Context>,
    ep_in: u8,
    ep_out: u8,
    iface: u8,
    transaction_id: u32,
    timeout: Duration,
}

impl PtpCamera {
    /// Find and open a USB device by vendor/product ID, claim the first
    /// interface with bulk IN + OUT endpoints (typically the PTP/Still Image
    /// interface).
    pub fn open(vendor_id: u16, product_id: u16) -> Result<Self, String> {
        let ctx = Context::new().map_err(|e| format!("USB context init failed: {e}"))?;
        let devices = ctx.devices().map_err(|e| format!("Cannot enumerate USB devices: {e}"))?;

        for device in devices.iter() {
            let desc = match device.device_descriptor() {
                Ok(d) => d,
                Err(_) => continue,
            };

            if desc.vendor_id() != vendor_id || desc.product_id() != product_id {
                continue;
            }

            let config = device
                .active_config_descriptor()
                .map_err(|e| format!("Cannot read config descriptor: {e}"))?;

            let mut ep_in = None;
            let mut ep_out = None;
            let mut iface_num = 0u8;

            'outer: for interface in config.interfaces() {
                for if_desc in interface.descriptors() {
                    let mut found_in = None;
                    let mut found_out = None;

                    for ep in if_desc.endpoint_descriptors() {
                        if ep.transfer_type() != TransferType::Bulk {
                            continue;
                        }
                        match ep.direction() {
                            Direction::In => found_in = Some(ep.address()),
                            Direction::Out => found_out = Some(ep.address()),
                        }
                    }

                    if let (Some(i), Some(o)) = (found_in, found_out) {
                        ep_in = Some(i);
                        ep_out = Some(o);
                        iface_num = if_desc.interface_number();
                        break 'outer;
                    }
                }
            }

            let (ep_in, ep_out) = match (ep_in, ep_out) {
                (Some(i), Some(o)) => (i, o),
                _ => return Err("No bulk endpoints found on device".into()),
            };

            let handle = device
                .open()
                .map_err(|e| format!("Cannot open USB device: {e}"))?;

            let _ = handle.set_auto_detach_kernel_driver(true);

            handle.claim_interface(iface_num).map_err(|e| {
                format!(
                    "Cannot claim interface {iface_num}: {e}. \
                     Is another app (Photos, Image Capture) using the camera?"
                )
            })?;

            return Ok(PtpCamera {
                handle,
                ep_in,
                ep_out,
                iface: iface_num,
                transaction_id: 0,
                timeout: Duration::from_secs(5),
            });
        }

        Err(format!(
            "Device {:04X}:{:04X} not found on USB",
            vendor_id, product_id
        ))
    }

    // -- low-level transport ------------------------------------------------

    fn next_tid(&mut self) -> u32 {
        self.transaction_id += 1;
        self.transaction_id
    }

    fn send_command(&mut self, code: u16, params: &[u32]) -> Result<u32, String> {
        let tid = self.next_tid();
        let length = (PTP_HEADER_LEN + params.len() * 4) as u32;

        let mut buf = Vec::with_capacity(length as usize);
        buf.extend_from_slice(&length.to_le_bytes());
        buf.extend_from_slice(&CONTAINER_COMMAND.to_le_bytes());
        buf.extend_from_slice(&code.to_le_bytes());
        buf.extend_from_slice(&tid.to_le_bytes());
        for p in params {
            buf.extend_from_slice(&p.to_le_bytes());
        }

        self.handle
            .write_bulk(self.ep_out, &buf, self.timeout)
            .map_err(|e| format!("USB write (command) error: {e}"))?;
        Ok(tid)
    }

    fn send_data(&self, code: u16, tid: u32, data: &[u8]) -> Result<(), String> {
        let length = (PTP_HEADER_LEN + data.len()) as u32;

        let mut buf = Vec::with_capacity(length as usize);
        buf.extend_from_slice(&length.to_le_bytes());
        buf.extend_from_slice(&CONTAINER_DATA.to_le_bytes());
        buf.extend_from_slice(&code.to_le_bytes());
        buf.extend_from_slice(&tid.to_le_bytes());
        buf.extend_from_slice(data);

        let timeout = if data.len() > 1_000_000 {
            Duration::from_secs(120)
        } else {
            self.timeout
        };

        self.handle
            .write_bulk(self.ep_out, &buf, timeout)
            .map_err(|e| format!("USB write (data) error: {e}"))?;
        Ok(())
    }

    /// Read a complete PTP container from bulk IN.  Returns
    /// (container_type, code, transaction_id, payload).
    fn read_container(&self, timeout: Duration) -> Result<(u16, u16, u32, Vec<u8>), String> {
        let mut tmp = vec![0u8; 524_288]; // 512 KB scratch buffer
        let n = self
            .handle
            .read_bulk(self.ep_in, &mut tmp, timeout)
            .map_err(|e| format!("USB read error: {e}"))?;

        if n < PTP_HEADER_LEN {
            return Err(format!(
                "Short PTP read: got {n} bytes, need at least {PTP_HEADER_LEN}"
            ));
        }

        let total_len = u32::from_le_bytes([tmp[0], tmp[1], tmp[2], tmp[3]]) as usize;
        let ctype = u16::from_le_bytes([tmp[4], tmp[5]]);
        let code = u16::from_le_bytes([tmp[6], tmp[7]]);
        let tid = u32::from_le_bytes([tmp[8], tmp[9], tmp[10], tmp[11]]);

        let payload_len = total_len.saturating_sub(PTP_HEADER_LEN);
        let mut payload = Vec::with_capacity(payload_len);

        if n > PTP_HEADER_LEN {
            let avail = (n - PTP_HEADER_LEN).min(payload_len);
            payload.extend_from_slice(&tmp[PTP_HEADER_LEN..PTP_HEADER_LEN + avail]);
        }

        while payload.len() < payload_len {
            let n = self
                .handle
                .read_bulk(self.ep_in, &mut tmp, timeout)
                .map_err(|e| format!("USB read (continuation) error: {e}"))?;
            if n == 0 {
                break;
            }
            payload.extend_from_slice(&tmp[..n]);
        }

        payload.truncate(payload_len);
        Ok((ctype, code, tid, payload))
    }

    fn parse_response_payload(payload: &[u8]) -> Vec<u32> {
        payload
            .chunks_exact(4)
            .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect()
    }

    fn read_response(&self, timeout: Duration) -> Result<PtpResponse, String> {
        let (ctype, code, _tid, payload) = self.read_container(timeout)?;
        if ctype != CONTAINER_RESPONSE {
            return Err(format!(
                "Expected response container (type 3), got type {ctype}"
            ));
        }
        Ok(PtpResponse {
            code,
            params: Self::parse_response_payload(&payload),
        })
    }

    // -- PTP transaction helpers --------------------------------------------

    /// No-data transaction: Command → Response
    fn transact(&mut self, code: u16, params: &[u32]) -> Result<PtpResponse, String> {
        self.send_command(code, params)?;
        self.read_response(self.timeout)
    }

    /// Data-in transaction: Command → Data(in) → Response.
    /// Returns the received data payload and the response.
    fn transact_data_in(
        &mut self,
        code: u16,
        params: &[u32],
        timeout: Duration,
    ) -> Result<(Vec<u8>, PtpResponse), String> {
        self.send_command(code, params)?;

        let (ctype, rcode, _tid, payload) = self.read_container(timeout)?;

        if ctype == CONTAINER_RESPONSE {
            return Ok((
                Vec::new(),
                PtpResponse {
                    code: rcode,
                    params: Self::parse_response_payload(&payload),
                },
            ));
        }
        if ctype != CONTAINER_DATA {
            return Err(format!(
                "Expected data container (type 2), got type {ctype}"
            ));
        }

        let data = payload;
        let response = self.read_response(timeout)?;
        Ok((data, response))
    }

    /// Data-out transaction: Command → Data(out) → Response.
    fn transact_data_out(
        &mut self,
        code: u16,
        params: &[u32],
        data: &[u8],
    ) -> Result<PtpResponse, String> {
        let tid = self.send_command(code, params)?;
        self.send_data(code, tid, data)?;

        let resp_timeout = if data.len() > 1_000_000 {
            Duration::from_secs(120)
        } else {
            self.timeout
        };
        self.read_response(resp_timeout)
    }

    // -- public PTP operations ----------------------------------------------

    pub fn open_session(&mut self) -> Result<(), String> {
        let resp = self.transact(OC_OPEN_SESSION, &[1])?;
        if resp.code != RC_OK && resp.code != RC_SESSION_ALREADY_OPEN {
            return Err(format!(
                "OpenSession failed: 0x{:04X} ({})",
                resp.code,
                response_name(resp.code)
            ));
        }
        Ok(())
    }

    pub fn close_session(&mut self) -> Result<(), String> {
        let resp = self.transact(OC_CLOSE_SESSION, &[])?;
        if resp.code != RC_OK {
            return Err(format!(
                "CloseSession failed: 0x{:04X} ({})",
                resp.code,
                response_name(resp.code)
            ));
        }
        Ok(())
    }

    pub fn get_device_info(&mut self) -> Result<DeviceInfo, String> {
        let (data, resp) =
            self.transact_data_in(OC_GET_DEVICE_INFO, &[], Duration::from_secs(10))?;
        if resp.code != RC_OK {
            return Err(format!(
                "GetDeviceInfo failed: 0x{:04X} ({})",
                resp.code,
                response_name(resp.code)
            ));
        }
        parse_device_info(&data)
    }

    pub fn get_device_prop_value(&mut self, prop: u16) -> Result<Vec<u8>, String> {
        let (data, resp) = self.transact_data_in(
            OC_GET_DEVICE_PROP_VALUE,
            &[prop as u32],
            Duration::from_secs(10),
        )?;
        if resp.code != RC_OK {
            return Err(format!(
                "GetDevicePropValue 0x{prop:04X} failed: 0x{:04X} ({})",
                resp.code,
                response_name(resp.code)
            ));
        }
        Ok(data)
    }

    pub fn set_device_prop_value(&mut self, prop: u16, data: &[u8]) -> Result<(), String> {
        let resp =
            self.transact_data_out(OC_SET_DEVICE_PROP_VALUE, &[prop as u32], data)?;
        if resp.code != RC_OK {
            return Err(format!(
                "SetDevicePropValue 0x{prop:04X} failed: 0x{:04X} ({})",
                resp.code,
                response_name(resp.code)
            ));
        }
        Ok(())
    }

    pub fn get_object_handles(
        &mut self,
        storage_id: u32,
        format: u32,
        parent: u32,
    ) -> Result<Vec<u32>, String> {
        let (data, resp) = self.transact_data_in(
            OC_GET_OBJECT_HANDLES,
            &[storage_id, format, parent],
            Duration::from_secs(10),
        )?;
        if resp.code != RC_OK {
            return Err(format!(
                "GetObjectHandles failed: 0x{:04X} ({})",
                resp.code,
                response_name(resp.code)
            ));
        }
        Ok(parse_u32_array(&data))
    }

    pub fn get_object(&mut self, handle: u32) -> Result<Vec<u8>, String> {
        let (data, resp) = self.transact_data_in(
            OC_GET_OBJECT,
            &[handle],
            Duration::from_secs(60),
        )?;
        if resp.code != RC_OK {
            return Err(format!(
                "GetObject 0x{handle:08X} failed: 0x{:04X} ({})",
                resp.code,
                response_name(resp.code)
            ));
        }
        Ok(data)
    }

    pub fn delete_object(&mut self, handle: u32) -> Result<PtpResponse, String> {
        self.transact(OC_DELETE_OBJECT, &[handle])
    }

    /// Send a vendor operation with a data-out payload (e.g. 0x900C / 0x900D).
    pub fn vendor_send(
        &mut self,
        op: u16,
        params: &[u32],
        data: &[u8],
    ) -> Result<PtpResponse, String> {
        self.transact_data_out(op, params, data)
    }

    /// Send a vendor operation and receive data back.
    pub fn vendor_receive(
        &mut self,
        op: u16,
        params: &[u32],
    ) -> Result<(Vec<u8>, PtpResponse), String> {
        self.transact_data_in(op, params, Duration::from_secs(10))
    }
}

// ---------------------------------------------------------------------------
// PTP dataset parsing helpers
// ---------------------------------------------------------------------------

struct PtpReader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> PtpReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn read_u8(&mut self) -> Result<u8, String> {
        if self.pos >= self.data.len() {
            return Err("Unexpected end of PTP data (u8)".into());
        }
        let v = self.data[self.pos];
        self.pos += 1;
        Ok(v)
    }

    fn read_u16(&mut self) -> Result<u16, String> {
        if self.pos + 2 > self.data.len() {
            return Err("Unexpected end of PTP data (u16)".into());
        }
        let v = u16::from_le_bytes([self.data[self.pos], self.data[self.pos + 1]]);
        self.pos += 2;
        Ok(v)
    }

    fn read_u32(&mut self) -> Result<u32, String> {
        if self.pos + 4 > self.data.len() {
            return Err("Unexpected end of PTP data (u32)".into());
        }
        let v = u32::from_le_bytes([
            self.data[self.pos],
            self.data[self.pos + 1],
            self.data[self.pos + 2],
            self.data[self.pos + 3],
        ]);
        self.pos += 4;
        Ok(v)
    }

    fn read_ptp_string(&mut self) -> Result<String, String> {
        let num_chars = self.read_u8()? as usize;
        if num_chars == 0 {
            return Ok(String::new());
        }
        if self.pos + num_chars * 2 > self.data.len() {
            return Err("PTP string extends beyond data".into());
        }
        let mut u16s = Vec::with_capacity(num_chars);
        for _ in 0..num_chars {
            u16s.push(self.read_u16()?);
        }
        if u16s.last() == Some(&0) {
            u16s.pop();
        }
        Ok(String::from_utf16_lossy(&u16s))
    }

    fn read_u16_array(&mut self) -> Result<Vec<u16>, String> {
        let count = self.read_u32()? as usize;
        let mut arr = Vec::with_capacity(count);
        for _ in 0..count {
            arr.push(self.read_u16()?);
        }
        Ok(arr)
    }
}

fn parse_device_info(data: &[u8]) -> Result<DeviceInfo, String> {
    let mut r = PtpReader::new(data);

    let standard_version = r.read_u16()?;
    let vendor_extension_id = r.read_u32()?;
    let vendor_extension_version = r.read_u16()?;
    let vendor_extension_desc = r.read_ptp_string()?;
    let functional_mode = r.read_u16()?;
    let operations_supported = r.read_u16_array()?;
    let events_supported = r.read_u16_array()?;
    let device_properties_supported = r.read_u16_array()?;
    let capture_formats = r.read_u16_array()?;
    let image_formats = r.read_u16_array()?;
    let manufacturer = r.read_ptp_string()?;
    let model = r.read_ptp_string()?;
    let device_version = r.read_ptp_string()?;
    let serial_number = r.read_ptp_string()?;

    Ok(DeviceInfo {
        standard_version,
        vendor_extension_id,
        vendor_extension_version,
        vendor_extension_desc,
        functional_mode,
        operations_supported,
        events_supported,
        device_properties_supported,
        capture_formats,
        image_formats,
        manufacturer,
        model,
        device_version,
        serial_number,
    })
}

fn parse_u32_array(data: &[u8]) -> Vec<u32> {
    if data.len() < 4 {
        return Vec::new();
    }
    let count = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
    let mut arr = Vec::with_capacity(count);
    let mut off = 4;
    for _ in 0..count {
        if off + 4 > data.len() {
            break;
        }
        arr.push(u32::from_le_bytes([
            data[off],
            data[off + 1],
            data[off + 2],
            data[off + 3],
        ]));
        off += 4;
    }
    arr
}

// ---------------------------------------------------------------------------
// ObjectInfo builder (for 0x900C data-out payload)
// ---------------------------------------------------------------------------

/// Build a PTP ObjectInfo dataset suitable for Fuji vendor SendObjectInfo
/// (0x900C).  Layout follows ISO 15740 §5.5.2.
pub fn build_object_info(format: u16, compressed_size: u32, filename: &str) -> Vec<u8> {
    let mut buf = Vec::with_capacity(128);

    buf.extend_from_slice(&0u32.to_le_bytes()); // StorageID
    buf.extend_from_slice(&format.to_le_bytes()); // ObjectFormat
    buf.extend_from_slice(&0u16.to_le_bytes()); // ProtectionStatus
    buf.extend_from_slice(&compressed_size.to_le_bytes()); // ObjectCompressedSize
    buf.extend_from_slice(&0u16.to_le_bytes()); // ThumbFormat
    buf.extend_from_slice(&0u32.to_le_bytes()); // ThumbCompressedSize
    buf.extend_from_slice(&0u32.to_le_bytes()); // ThumbPixWidth
    buf.extend_from_slice(&0u32.to_le_bytes()); // ThumbPixHeight
    buf.extend_from_slice(&0u32.to_le_bytes()); // ImagePixWidth
    buf.extend_from_slice(&0u32.to_le_bytes()); // ImagePixHeight
    buf.extend_from_slice(&0u32.to_le_bytes()); // ImageBitDepth
    buf.extend_from_slice(&0u32.to_le_bytes()); // ParentObject
    buf.extend_from_slice(&0u16.to_le_bytes()); // AssociationType
    buf.extend_from_slice(&0u32.to_le_bytes()); // AssociationDesc
    buf.extend_from_slice(&0u32.to_le_bytes()); // SequenceNumber
    encode_ptp_string(&mut buf, filename); // Filename
    buf.push(0); // CaptureDate (empty)
    buf.push(0); // ModificationDate (empty)
    buf.push(0); // Keywords (empty)

    buf
}

fn encode_ptp_string(buf: &mut Vec<u8>, s: &str) {
    if s.is_empty() {
        buf.push(0);
        return;
    }
    let utf16: Vec<u16> = s.encode_utf16().collect();
    let num_chars = (utf16.len() + 1) as u8; // +1 for null terminator
    buf.push(num_chars);
    for c in &utf16 {
        buf.extend_from_slice(&c.to_le_bytes());
    }
    buf.extend_from_slice(&0u16.to_le_bytes()); // null terminator
}

// ---------------------------------------------------------------------------
// Human-readable name lookups
// ---------------------------------------------------------------------------

pub fn operation_name(code: u16) -> &'static str {
    match code {
        0x1001 => "GetDeviceInfo",
        0x1002 => "OpenSession",
        0x1003 => "CloseSession",
        0x1004 => "GetStorageIDs",
        0x1005 => "GetStorageInfo",
        0x1006 => "GetNumObjects",
        0x1007 => "GetObjectHandles",
        0x1008 => "GetObjectInfo",
        0x1009 => "GetObject",
        0x100A => "GetThumb",
        0x100B => "DeleteObject",
        0x100C => "SendObjectInfo",
        0x100D => "SendObject",
        0x100E => "InitiateCapture",
        0x100F => "FormatStore",
        0x1010 => "ResetDevice",
        0x1011 => "SelfTest",
        0x1012 => "SetObjectProtection",
        0x1013 => "PowerDown",
        0x1014 => "GetDevicePropDesc",
        0x1015 => "GetDevicePropValue",
        0x1016 => "SetDevicePropValue",
        0x1017 => "ResetDevicePropValue",
        0x1018 => "TerminateOpenCapture",
        0x101B => "InitiateOpenCapture",
        // Fuji vendor operations
        0x900C => "Fuji SendObjectInfo",
        0x900D => "Fuji SendObject",
        0x901D => "Fuji WriteFile",
        0x9805 => "Fuji Hijack (debug)",
        _ if code >= 0x9000 => "Vendor Operation",
        _ => "(unknown)",
    }
}

pub fn response_name(code: u16) -> &'static str {
    match code {
        0x2001 => "OK",
        0x2002 => "General Error",
        0x2003 => "Session Not Open",
        0x2004 => "Invalid TransactionID",
        0x2005 => "Operation Not Supported",
        0x2006 => "Parameter Not Supported",
        0x2007 => "Incomplete Transfer",
        0x2008 => "Invalid StorageID",
        0x2009 => "Invalid ObjectHandle",
        0x200A => "DeviceProp Not Supported",
        0x200B => "Invalid ObjectFormatCode",
        0x200C => "Store Full",
        0x200D => "Object Write Protected",
        0x200E => "Store Read-Only",
        0x200F => "Access Denied",
        0x2010 => "No Thumbnail Present",
        0x2011 => "SelfTest Failed",
        0x2012 => "Partial Deletion",
        0x2013 => "Store Not Available",
        0x2014 => "Specification By Format Unsupported",
        0x2015 => "No Valid ObjectInfo",
        0x2019 => "Device Busy",
        0x201A => "Invalid DeviceProp Format",
        0x201C => "Invalid DeviceProp Value",
        0x201E => "Session Already Open",
        _ if code >= 0xA000 => "Vendor Response",
        _ => "(unknown response)",
    }
}

pub fn property_name(code: u16) -> &'static str {
    match code {
        // Standard PTP device properties
        0x5001 => "BatteryLevel",
        0x5003 => "ImageSize",
        0x5004 => "CompressionSetting",
        0x5005 => "WhiteBalance",
        0x5007 => "FNumber",
        0x5008 => "FocalLength",
        0x5009 => "FocusDistance",
        0x500A => "FocusMode",
        0x500B => "ExposureMeteringMode",
        0x500C => "FlashMode",
        0x500D => "ExposureTime",
        0x500E => "ExposureProgramMode",
        0x500F => "ExposureIndex (ISO)",
        0x5010 => "ExposureBiasCompensation",
        0x5011 => "DateTime",
        0x5012 => "CaptureDelay",
        0x5013 => "StillCaptureMode",
        // Fuji vendor device properties
        0xD100 => "Fuji FilmSimulation",
        0xD101 => "Fuji FilmSimulationTune",
        0xD102 => "Fuji DRangeMode",
        0xD103 => "Fuji ColorMode",
        0xD104 => "Fuji ColorSpace",
        0xD105 => "Fuji WhiteBalanceFineTune",
        0xD106 => "Fuji NoiseReduction",
        0xD108 => "Fuji HighISONoiseReduction",
        0xD10A => "Fuji ColorChromeEffect",
        0xD10B => "Fuji ColorChromeFXBlue",
        0xD153 => "Fuji ShadowTone",
        0xD154 => "Fuji HighlightTone",
        0xD155 => "Fuji Sharpness",
        0xD16E => "Fuji USBMode",
        0xD171 => "Fuji ExposureCompensation",
        0xD173 => "Fuji RecMode",
        0xD174 => "Fuji CommandDialMode",
        0xD183 => "Fuji StartRawConversion",
        0xD185 => "Fuji RawConvProfile",
        0xD200 => "Fuji FocusMeteringMode",
        0xD208 => "Fuji FocusPoint",
        0xD20E => "Fuji SelfTimer",
        0xD210 => "Fuji FlashMode",
        0xD211 => "Fuji FlashCompensation",
        0xD212 => "Fuji FlashSyncMode",
        0xD218 => "Fuji CropMode",
        0xD219 => "Fuji GrainEffect",
        0xD21C => "Fuji USBMode2",
        0xD310 => "Fuji ShutterType",
        0xD34D => "Fuji RawDevelopMode",
        0xD36A => "Fuji RawConvVersion",
        0xD36B => "Fuji RawConvParam",
        _ if code >= 0xD000 => "Fuji Vendor Property",
        _ => "(unknown property)",
    }
}

pub fn format_name(code: u16) -> &'static str {
    match code {
        0x3000 => "Undefined",
        0x3001 => "Association (folder)",
        0x3002 => "Script",
        0x3006 => "DPOF",
        0x3008 => "WAV",
        0x3009 => "MP3",
        0x3801 => "EXIF/JPEG",
        0x3802 => "TIFF/EP",
        0x3804 => "BMP",
        0x3807 => "GIF",
        0x3808 => "JFIF",
        0x380B => "PNG",
        0x380D => "TIFF",
        0xB101 => "WMV",
        0xB103 => "Fuji RAF",
        0xB982 => "MP4",
        0xF802 => "Fuji RAW Upload (vendor)",
        _ if code >= 0xB000 => "Vendor Format",
        _ => "(unknown format)",
    }
}
