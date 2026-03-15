#![allow(dead_code)]

use clap::ValueEnum;

// ---------------------------------------------------------------------------
// D185 binary profile layout
// ---------------------------------------------------------------------------
// Bytes 0..2:    n_props (u16 LE) — number of property slots (typically 0x1d)
// Bytes 2..0x201: IOP code as a PTP string + padding
// Bytes 0x201..:  n_props × u32 LE property values
//
// Property indices (from Fudge / petabyt/fp d185.c + X100VI empirical):
//   0  ShootingCondition    7  FilmSimulation    15 HighlightTone  (×10)
//   1  FileType             8  GrainEffect*      16 ShadowTone     (×10)
//   2  ImageSize            9  SmoothSkinEffect  17 Color          (×10)
//   3  ImageQuality        10  WBShootCond       18 Sharpness      (×10)
//   4  ExposureBias        11  WhiteBalance      19 NoiseReduction (table)
//   5  DynamicRange        12  WBShiftR (raw)    20 (unused on X100VI)
//   6  WideDRange          13  WBShiftB (raw)    21 ColorSpace
//                          14  WBColorTemp
//
// X100VI-specific extended indices (empirically verified):
//   23 ColorChromeEffect     (Off=1, Weak=2, Strong=3)
//   24 ColorChromeFXBlue     (Off=1, Weak=2, Strong=3)
//   26 Clarity               (×10)
//
// * GrainEffect (index 8) encodes BOTH roughness and size as a combined ordinal:
//     1=Off, 2=Weak+Small, 3=Strong+Small, 4=Weak+Large, 5=Strong+Large
//
// Encoding notes (from D185 profile dump analysis on X100VI fw 1.31):
//   - Indices 15-18, 26 use ×10 encoding (e.g. Shadow=-2 → -20)
//   - WB shifts (12-13) use RAW integer values (NOT ×10)
//   - WBShootCond (10) must be set to 0 (OFF) for the camera to honour
//     WhiteBalance (11). Default value 1 (ON) = use shooting condition's WB.

const PROPS_OFFSET: usize = 0x201;
const PROP_EXPOSURE_BIAS: usize = 4;
const PROP_DYNAMIC_RANGE: usize = 5;
const PROP_FILM_SIM: usize = 7;
const PROP_GRAIN: usize = 8;
const PROP_WB_SHOOT_COND: usize = 10;
const PROP_WB: usize = 11;
const PROP_WB_SHIFT_R: usize = 12;
const PROP_WB_SHIFT_B: usize = 13;
const PROP_WB_TEMP: usize = 14;
const PROP_HIGHLIGHT: usize = 15;
const PROP_SHADOW: usize = 16;
const PROP_COLOR: usize = 17;
const PROP_SHARPNESS: usize = 18;
const PROP_NOISE_REDUCTION: usize = 19;
// Index 22 is NOT GrainSize — tested values 0-3 and 0x20, none changed EXIF.
// Grain size is encoded in PROP_GRAIN as a combined ordinal (see GrainEffect).
const PROP_CHROME_EFFECT: usize = 23;
const PROP_CHROME_BLUE: usize = 24;
const PROP_CLARITY: usize = 26;

// ---------------------------------------------------------------------------
// Film simulation
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, ValueEnum)]
pub enum FilmSimulation {
    Provia,
    Velvia,
    Astia,
    ClassicChrome,
    ClassicNeg,
    ProNegHi,
    ProNegStd,
    Eterna,
    EternaBleachBypass,
    Acros,
    AcrosYe,
    AcrosR,
    AcrosG,
    Monochrome,
    MonochromeYe,
    MonochromeR,
    MonochromeG,
    Sepia,
    NostalgicNeg,
    RealaAce,
}

impl FilmSimulation {
    fn to_d185(self) -> u32 {
        match self {
            Self::Provia => 0x01,
            Self::Velvia => 0x02,
            Self::Astia => 0x03,
            Self::ProNegHi => 0x04,
            Self::ProNegStd => 0x05,
            Self::Monochrome => 0x06,
            Self::MonochromeYe => 0x07,
            Self::MonochromeR => 0x08,
            Self::MonochromeG => 0x09,
            Self::Sepia => 0x0A,
            Self::ClassicChrome => 0x0B,
            Self::Acros => 0x0C,
            Self::AcrosYe => 0x0D,
            Self::AcrosR => 0x0E,
            Self::AcrosG => 0x0F,
            Self::Eterna => 0x10,
            Self::ClassicNeg => 0x11,
            Self::EternaBleachBypass => 0x12,
            Self::NostalgicNeg => 0x13,
            Self::RealaAce => 0x14,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Provia => "Provia/Standard",
            Self::Velvia => "Velvia/Vivid",
            Self::Astia => "Astia/Soft",
            Self::ProNegHi => "PRO Neg Hi",
            Self::ProNegStd => "PRO Neg Std",
            Self::Monochrome => "Monochrome",
            Self::MonochromeYe => "Monochrome+Ye",
            Self::MonochromeR => "Monochrome+R",
            Self::MonochromeG => "Monochrome+G",
            Self::Sepia => "Sepia",
            Self::ClassicChrome => "Classic Chrome",
            Self::Acros => "Acros",
            Self::AcrosYe => "Acros+Ye",
            Self::AcrosR => "Acros+R",
            Self::AcrosG => "Acros+G",
            Self::Eterna => "Eterna/Cinema",
            Self::ClassicNeg => "Classic Neg",
            Self::EternaBleachBypass => "Eterna Bleach Bypass",
            Self::NostalgicNeg => "Nostalgic Neg",
            Self::RealaAce => "REALA ACE",
        }
    }
}

// ---------------------------------------------------------------------------
// Grain effect
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, ValueEnum)]
pub enum GrainEffect {
    Off,
    Weak,
    Strong,
}

impl GrainEffect {
    fn to_d185(self, size: Option<GrainSize>) -> u32 {
        let large = matches!(size, Some(GrainSize::Large));
        match (self, large) {
            (Self::Off, _) => 1,
            (Self::Weak, false) => 2,
            (Self::Weak, true) => 4,
            (Self::Strong, false) => 3,
            (Self::Strong, true) => 5,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, ValueEnum)]
pub enum GrainSize {
    Small,
    Large,
}

impl GrainSize {
    fn to_d185(self) -> u32 {
        match self {
            Self::Small => 0x10,
            Self::Large => 0x20,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, ValueEnum)]
pub enum ChromeLevel {
    Off,
    Weak,
    Strong,
}

impl ChromeLevel {
    fn to_d185(self) -> u32 {
        match self {
            Self::Off => 1,
            Self::Weak => 2,
            Self::Strong => 3,
        }
    }
}

// ---------------------------------------------------------------------------
// D185 value encoding helpers
// ---------------------------------------------------------------------------

/// Encode highlight/shadow tone for X-Processor 5 (half-stop steps).
/// Input: floating-point value like -1.5, 0, +2.5, etc.
fn encode_tone(val: f32) -> u32 {
    let scaled = (val * 10.0).round() as i32;
    scaled as u32
}

/// Encode color / sharpness (integer steps, ×10 internally).
fn encode_range(val: i32) -> u32 {
    (val * 10) as u32
}

/// Encode noise reduction to Fuji's non-linear encoding.
fn encode_noise_reduction(val: i32) -> u32 {
    match val {
        4 => 0x5000,
        3 => 0x6000,
        2 => 0x0000,
        1 => 0x1000,
        0 => 0x2000,
        -1 => 0x3000,
        -2 => 0x4000,
        -3 => 0x7000,
        -4 => 0x8000,
        _ => 0x2000, // default to 0
    }
}

/// Encode clarity (×10 internally, same as color / sharpness).
fn encode_clarity(val: i32) -> u32 {
    encode_range(val)
}

/// Encode white balance shift as raw signed integer.
/// D185 dump shows WBShiftR=1, WBShiftB=-6 matching recipe units directly.
fn encode_wb_shift(val: i32) -> u32 {
    val as u32
}

/// Parse exposure compensation from multiple notations:
///   Decimal shorthand:  0, +1, -2, +0.3, -0.7, +1.3, -2.7
///   Fraction notation:  +1/3, -2/3, +1 1/3, -2 2/3
///
/// The .3/.7 shorthand maps to 1/3 and 2/3 stops respectively,
/// matching the notation used on Fujifilm dials and menus.
pub fn parse_exposure_comp(s: &str) -> Result<i32, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("empty string".into());
    }

    let (neg, body) = if let Some(rest) = s.strip_prefix('-') {
        (true, rest.trim())
    } else if let Some(rest) = s.strip_prefix('+') {
        (false, rest.trim())
    } else {
        (false, s)
    };

    let millis = if body.contains('.') {
        // Decimal shorthand: "1.3" → 1333, "0.7" → 667
        let dot = body.find('.').unwrap();
        let whole: i32 = if dot == 0 {
            0
        } else {
            body[..dot].parse().map_err(|_| format!("invalid number: {body}"))?
        };
        let frac_digit = &body[dot + 1..];
        let frac = match frac_digit {
            "0" => 0,
            "3" => 333,
            "7" => 667,
            _ => return Err(format!("use .3 for 1/3 or .7 for 2/3 (got .{frac_digit})")),
        };
        whole * 1000 + frac
    } else if body.contains('/') {
        // Fraction notation: "1/3", "2/3", or "1 1/3", "2 2/3"
        let parts: Vec<&str> = body.split_whitespace().collect();
        let (whole, frac_str) = match parts.len() {
            1 => (0i32, parts[0]),
            2 => {
                let w: i32 = parts[0].parse().map_err(|_| format!("invalid number: {}", parts[0]))?;
                (w, parts[1])
            }
            _ => return Err(format!("cannot parse '{s}'")),
        };
        let frac = match frac_str {
            "1/3" => 333,
            "2/3" => 667,
            other => return Err(format!("unsupported fraction: {other} (use 1/3 or 2/3)")),
        };
        whole * 1000 + frac
    } else {
        // Whole stops: "0", "1", "2", "3"
        let w: i32 = body.parse().map_err(|_| format!("invalid number: {body}"))?;
        w * 1000
    };

    if millis.abs() > 3000 {
        return Err(format!("out of range: ±3 EV max"));
    }

    Ok(if neg { -millis } else { millis })
}

fn encode_exposure_comp(millis: i32) -> u32 {
    millis as u32
}

fn format_ev(millis: i32) -> String {
    let sign = if millis < 0 { "-" } else if millis > 0 { "+" } else { "" };
    let abs = millis.abs();
    let whole = abs / 1000;
    let rem = abs % 1000;
    match (whole, rem) {
        (0, 0) => "0".into(),
        (0, 333) => format!("{sign}1/3"),
        (0, 667) => format!("{sign}2/3"),
        (w, 0) => format!("{sign}{w}"),
        (w, 333) => format!("{sign}{w} 1/3"),
        (w, 667) => format!("{sign}{w} 2/3"),
        _ => format!("{sign}{}.{:03}", whole, rem),
    }
}

/// Encode white balance mode. Normalizes casing so recipe strings (e.g. "Auto") match.
fn encode_white_balance(mode: &str, _temp: Option<u32>) -> Option<u32> {
    let m = mode.trim().to_lowercase();
    match m.as_str() {
        "auto" | "auto-white" | "auto-ambience" => Some(0x0002),
        "daylight" => Some(0x0004),
        "shade" => Some(0x8006),
        "incandescent" => Some(0x0006),
        "fluorescent-1" => Some(0x8001),
        "fluorescent-2" => Some(0x8002),
        "fluorescent-3" => Some(0x8003),
        "temperature" => Some(0x8007), // Fuji vendor WB mode for Temperature/Kelvin
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Recipe settings — all supported overrides
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Clone, PartialEq)]
pub struct RecipeSettings {
    pub film_sim: Option<FilmSimulation>,
    pub grain: Option<GrainEffect>,
    pub grain_size: Option<GrainSize>,
    pub highlight: Option<f32>,
    pub shadow: Option<f32>,
    pub color: Option<i32>,
    pub sharpness: Option<i32>,
    pub noise_reduction: Option<i32>,
    pub clarity: Option<i32>,
    pub white_balance: Option<String>,
    pub wb_temp: Option<u32>,
    pub wb_shift_r: Option<i32>,
    pub wb_shift_b: Option<i32>,
    pub chrome_effect: Option<ChromeLevel>,
    pub chrome_blue: Option<ChromeLevel>,
    pub dynamic_range: Option<u32>,
    /// Exposure compensation in millis (EV × 1000), e.g. 333 = +1/3 EV
    pub exposure_comp: Option<i32>,
}

impl RecipeSettings {
    pub fn is_empty(&self) -> bool {
        self.film_sim.is_none()
            && self.grain.is_none()
            && self.grain_size.is_none()
            && self.highlight.is_none()
            && self.shadow.is_none()
            && self.color.is_none()
            && self.sharpness.is_none()
            && self.noise_reduction.is_none()
            && self.clarity.is_none()
            && self.white_balance.is_none()
            && self.wb_shift_r.is_none()
            && self.wb_shift_b.is_none()
            && self.chrome_effect.is_none()
            && self.chrome_blue.is_none()
            && self.dynamic_range.is_none()
            && self.exposure_comp.is_none()
    }

    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        if let Some(sim) = self.film_sim {
            parts.push(format!("Film: {}", sim.label()));
        }
        if let Some(grain) = self.grain {
            let size_str = match self.grain_size {
                Some(GrainSize::Small) => "/small",
                Some(GrainSize::Large) => "/large",
                None => "",
            };
            parts.push(format!("Grain: {grain:?}{size_str}"));
        }
        if let Some(v) = self.highlight {
            parts.push(format!("Highlight: {v:+}"));
        }
        if let Some(v) = self.shadow {
            parts.push(format!("Shadow: {v:+}"));
        }
        if let Some(v) = self.color {
            parts.push(format!("Color: {v:+}"));
        }
        if let Some(v) = self.sharpness {
            parts.push(format!("Sharp: {v:+}"));
        }
        if let Some(v) = self.noise_reduction {
            parts.push(format!("NR: {v:+}"));
        }
        if let Some(v) = self.clarity {
            parts.push(format!("Clarity: {v:+}"));
        }
        if let Some(ref wb) = self.white_balance {
            if let Some(t) = self.wb_temp {
                parts.push(format!("WB: {t}K"));
            } else {
                parts.push(format!("WB: {wb}"));
            }
        }
        if let Some(r) = self.wb_shift_r {
            parts.push(format!("WB-R: {r:+}"));
        }
        if let Some(b) = self.wb_shift_b {
            parts.push(format!("WB-B: {b:+}"));
        }
        if let Some(ce) = self.chrome_effect {
            parts.push(format!("Chrome: {ce:?}"));
        }
        if let Some(cb) = self.chrome_blue {
            parts.push(format!("ChromeBlue: {cb:?}"));
        }
        if let Some(dr) = self.dynamic_range {
            parts.push(format!("DR{dr}"));
        }
        if let Some(millis) = self.exposure_comp {
            parts.push(format!("EV: {}", format_ev(millis)));
        }
        if parts.is_empty() {
            "camera defaults".to_string()
        } else {
            parts.join(", ")
        }
    }

    /// Merge CLI overrides on top of recipe settings. CLI flags take priority.
    pub fn merge_cli(
        &mut self,
        film_sim: Option<FilmSimulation>,
        grain: Option<GrainEffect>,
        grain_size: Option<GrainSize>,
        exposure_comp: Option<i32>,
    ) {
        if let Some(fs) = film_sim {
            self.film_sim = Some(fs);
        }
        if let Some(g) = grain {
            self.grain = Some(g);
        }
        if let Some(gs) = grain_size {
            self.grain_size = Some(gs);
        }
        if let Some(ev) = exposure_comp {
            self.exposure_comp = Some(ev);
        }
    }
}

// ---------------------------------------------------------------------------
// D185 profile read/write
// ---------------------------------------------------------------------------

fn read_prop(profile: &[u8], index: usize) -> Option<u32> {
    let offset = PROPS_OFFSET + index * 4;
    if offset + 4 > profile.len() {
        return None;
    }
    Some(u32::from_le_bytes([
        profile[offset],
        profile[offset + 1],
        profile[offset + 2],
        profile[offset + 3],
    ]))
}

fn write_prop(profile: &mut [u8], index: usize, value: u32) {
    let offset = PROPS_OFFSET + index * 4;
    if offset + 4 <= profile.len() {
        profile[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
}

/// Apply recipe settings to a D185 profile blob (in-place).
/// Returns a human-readable log of changes.
pub fn apply_recipe(profile: &mut Vec<u8>, settings: &RecipeSettings) -> String {
    if profile.len() < PROPS_OFFSET + 4 {
        return "profile too short to modify".to_string();
    }

    let mut changes: Vec<String> = Vec::new();

    macro_rules! set_prop {
        ($name:expr, $idx:expr, $val:expr) => {{
            let old = read_prop(profile, $idx).unwrap_or(0);
            let new = $val;
            write_prop(profile, $idx, new);
            changes.push(format!("  {} 0x{:X} -> 0x{:X}", $name, old, new));
        }};
    }

    if let Some(dr) = settings.dynamic_range {
        set_prop!("DynamicRange", PROP_DYNAMIC_RANGE, dr);
    }
    if let Some(ev) = settings.exposure_comp {
        // The camera limits negative EV based on DynamicRange:
        //   DR100 → -3 EV min, DR200 → -3 EV min, DR400 → -2 EV min
        // Values beyond the limit are silently ignored by the firmware.
        let dr = read_prop(profile, PROP_DYNAMIC_RANGE).unwrap_or(100);
        let min_ev = if dr >= 400 { -2000 } else { -3000 };
        let clamped = ev.max(min_ev);
        if clamped != ev {
            changes.push(format!(
                "  ExposureBias clamped {} -> {} (DR{} limits negative EV to {})",
                format_ev(ev), format_ev(clamped), dr, format_ev(min_ev)
            ));
        }
        set_prop!("ExposureBias", PROP_EXPOSURE_BIAS, encode_exposure_comp(clamped));
    }
    if let Some(sim) = settings.film_sim {
        set_prop!("FilmSim", PROP_FILM_SIM, sim.to_d185());
    }
    if let Some(grain) = settings.grain {
        set_prop!("Grain", PROP_GRAIN, grain.to_d185(settings.grain_size));
    }
    if let Some(v) = settings.highlight {
        set_prop!("Highlight", PROP_HIGHLIGHT, encode_tone(v));
    }
    if let Some(v) = settings.shadow {
        set_prop!("Shadow", PROP_SHADOW, encode_tone(v));
    }
    if let Some(v) = settings.color {
        set_prop!("Color", PROP_COLOR, encode_range(v));
    }
    if let Some(v) = settings.sharpness {
        set_prop!("Sharpness", PROP_SHARPNESS, encode_range(v));
    }
    if let Some(v) = settings.noise_reduction {
        set_prop!("NR", PROP_NOISE_REDUCTION, encode_noise_reduction(v));
    }
    if let Some(v) = settings.clarity {
        set_prop!("Clarity", PROP_CLARITY, encode_clarity(v));
    }
    if let Some(ce) = settings.chrome_effect {
        set_prop!("ChromeEffect", PROP_CHROME_EFFECT, ce.to_d185());
    }
    if let Some(cb) = settings.chrome_blue {
        set_prop!("ChromeFXBlue", PROP_CHROME_BLUE, cb.to_d185());
    }
    if let Some(ref wb) = settings.white_balance {
        if let Some(wb_val) = encode_white_balance(wb, settings.wb_temp) {
            // WBShootCond=0 (OFF) tells the camera to use our WB instead of the
            // shooting condition's WB. Without this, the camera ignores index 11.
            set_prop!("WBShootCond", PROP_WB_SHOOT_COND, 0);
            set_prop!("WB", PROP_WB, wb_val);
        }
        if let Some(temp) = settings.wb_temp {
            set_prop!("WBTemp", PROP_WB_TEMP, temp);
        }
    }
    if let Some(v) = settings.wb_shift_r {
        set_prop!("WB-R", PROP_WB_SHIFT_R, encode_wb_shift(v));
    }
    if let Some(v) = settings.wb_shift_b {
        set_prop!("WB-B", PROP_WB_SHIFT_B, encode_wb_shift(v));
    }

    if changes.is_empty() {
        "no changes".to_string()
    } else {
        changes.join("\n")
    }
}

/// Dump all property values from a D185 profile for diagnostic purposes.
pub fn dump_profile(profile: &[u8]) {
    if profile.len() < PROPS_OFFSET + 4 {
        println!("  Profile too short ({} bytes)", profile.len());
        return;
    }
    let n_props = u16::from_le_bytes([profile[0], profile[1]]) as usize;
    let max_props = (profile.len() - PROPS_OFFSET) / 4;
    let count = n_props.min(max_props);
    println!("  n_props={n_props} (max from size: {max_props})");
    for i in 0..count {
        let val = read_prop(profile, i).unwrap_or(0);
        let signed = val as i32;
        let label = match i {
            0 => "ShootingCondition",
            1 => "FileType",
            2 => "ImageSize",
            3 => "ImageQuality",
            4 => "ExposureBias",
            5 => "DynamicRange",
            6 => "WideDRange",
            7 => "FilmSimulation",
            8 => "GrainEffect",
            9 => "SmoothSkinEffect",
            10 => "WBShootCond",
            11 => "WhiteBalance",
            12 => "WBShiftR",
            13 => "WBShiftB",
            14 => "WBColorTemp",
            15 => "HighlightTone",
            16 => "ShadowTone",
            17 => "Color",
            18 => "Sharpness",
            19 => "NoiseReduction",
            20 => "(legacy)",
            21 => "ColorSpace",
            22 => "GrainEffectSize",
            23 => "ColorChromeEffect",
            24 => "ColorChromeFXBlue",
            25 => "?",
            26 => "Clarity",
            27 => "?",
            _ => "?",
        };
        if signed < 0 && signed > -10000 {
            println!("  [{i:2}] {label:<20} = {signed} (0x{val:08X})");
        } else {
            println!("  [{i:2}] {label:<20} = {val} (0x{val:08X})");
        }
    }
}

/// Decode the current film simulation from a profile for display.
pub fn current_film_sim(profile: &[u8]) -> String {
    match read_prop(profile, PROP_FILM_SIM) {
        Some(v) => {
            let name = match v {
                0x01 => "Provia/Standard",
                0x02 => "Velvia/Vivid",
                0x03 => "Astia/Soft",
                0x04 => "PRO Neg Hi",
                0x05 => "PRO Neg Std",
                0x06 => "Monochrome",
                0x07 => "Monochrome+Ye",
                0x08 => "Monochrome+R",
                0x09 => "Monochrome+G",
                0x0A => "Sepia",
                0x0B => "Classic Chrome",
                0x0C => "Acros",
                0x0D => "Acros+Ye",
                0x0E => "Acros+R",
                0x0F => "Acros+G",
                0x10 => "Eterna/Cinema",
                0x11 => "Classic Neg",
                0x12 => "Eterna Bleach Bypass",
                0x13 => "Nostalgic Neg",
                0x14 => "REALA ACE",
                _ => "Unknown",
            };
            format!("{name} (0x{v:X})")
        }
        None => "unreadable".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_exposure_comp_fractions() {
        assert_eq!(parse_exposure_comp("0").unwrap(), 0);
        assert_eq!(parse_exposure_comp("+1/3").unwrap(), 333);
        assert_eq!(parse_exposure_comp("-1/3").unwrap(), -333);
        assert_eq!(parse_exposure_comp("+2/3").unwrap(), 667);
        assert_eq!(parse_exposure_comp("-2/3").unwrap(), -667);
        assert_eq!(parse_exposure_comp("+1").unwrap(), 1000);
        assert_eq!(parse_exposure_comp("-1").unwrap(), -1000);
        assert_eq!(parse_exposure_comp("+1 1/3").unwrap(), 1333);
        assert_eq!(parse_exposure_comp("-1 2/3").unwrap(), -1667);
        assert_eq!(parse_exposure_comp("+2").unwrap(), 2000);
        assert_eq!(parse_exposure_comp("-3").unwrap(), -3000);
        assert_eq!(parse_exposure_comp("+2 2/3").unwrap(), 2667);
        assert_eq!(parse_exposure_comp("1/3").unwrap(), 333);
        assert_eq!(parse_exposure_comp("2").unwrap(), 2000);
    }

    #[test]
    fn test_parse_exposure_comp_decimal_shorthand() {
        assert_eq!(parse_exposure_comp("+0.3").unwrap(), 333);
        assert_eq!(parse_exposure_comp("-0.3").unwrap(), -333);
        assert_eq!(parse_exposure_comp("+0.7").unwrap(), 667);
        assert_eq!(parse_exposure_comp("-0.7").unwrap(), -667);
        assert_eq!(parse_exposure_comp("+1.3").unwrap(), 1333);
        assert_eq!(parse_exposure_comp("-1.7").unwrap(), -1667);
        assert_eq!(parse_exposure_comp("+2.3").unwrap(), 2333);
        assert_eq!(parse_exposure_comp("-2.7").unwrap(), -2667);
        assert_eq!(parse_exposure_comp("1.0").unwrap(), 1000);
        assert_eq!(parse_exposure_comp(".3").unwrap(), 333);
        assert_eq!(parse_exposure_comp("-.7").unwrap(), -667);
    }

    #[test]
    fn test_parse_exposure_comp_errors() {
        assert!(parse_exposure_comp("+4").is_err());
        assert!(parse_exposure_comp("abc").is_err());
        assert!(parse_exposure_comp("+1 3/4").is_err());
        assert!(parse_exposure_comp("+1.5").is_err());
        assert!(parse_exposure_comp("").is_err());
    }

    #[test]
    fn test_format_ev() {
        assert_eq!(format_ev(0), "0");
        assert_eq!(format_ev(333), "+1/3");
        assert_eq!(format_ev(-333), "-1/3");
        assert_eq!(format_ev(667), "+2/3");
        assert_eq!(format_ev(-667), "-2/3");
        assert_eq!(format_ev(1000), "+1");
        assert_eq!(format_ev(-1000), "-1");
        assert_eq!(format_ev(1333), "+1 1/3");
        assert_eq!(format_ev(-2667), "-2 2/3");
        assert_eq!(format_ev(3000), "+3");
    }

    #[test]
    fn test_encode_exposure_comp_twos_complement() {
        assert_eq!(encode_exposure_comp(0), 0);
        assert_eq!(encode_exposure_comp(333), 333);
        assert_eq!(encode_exposure_comp(-333), 0xfffffeb3);
        assert_eq!(encode_exposure_comp(-1000), 0xfffffc18);
        assert_eq!(encode_exposure_comp(3000), 3000);
    }
}
