#![allow(dead_code)]

use clap::ValueEnum;

// ---------------------------------------------------------------------------
// D185 binary profile layout
// ---------------------------------------------------------------------------
// Bytes 0..2:    n_props (u16 LE) — number of property slots (typically 0x1d)
// Bytes 2..0x201: IOP code as a PTP string + padding
// Bytes 0x201..:  n_props × u32 LE property values
//
// Property indices (from Fudge / petabyt/fp):
//   0  ShootingCondition    7  FilmSimulation    15 HighlightTone
//   1  FileType             8  GrainEffect       16 ShadowTone
//   2  ImageSize            9  SmoothSkinEffect  17 Color
//   3  ImageQuality        10  WBShootCond       18 Sharpness
//   4  ExposureBias        11  WhiteBalance      19 NoiseReduction
//   5  DynamicRange        12  WBShiftR          20 Clarity
//   6  WideDRange          13  WBShiftB          21 ColorSpace
//                          14  WBColorTemp

const PROPS_OFFSET: usize = 0x201;
const PROP_FILM_SIM: usize = 7;
const PROP_GRAIN: usize = 8;
const PROP_WB: usize = 11;
const PROP_WB_SHIFT_R: usize = 12;
const PROP_WB_SHIFT_B: usize = 13;
const PROP_WB_TEMP: usize = 14;
const PROP_HIGHLIGHT: usize = 15;
const PROP_SHADOW: usize = 16;
const PROP_COLOR: usize = 17;
const PROP_SHARPNESS: usize = 18;
const PROP_NOISE_REDUCTION: usize = 19;

// ---------------------------------------------------------------------------
// Film simulation — CLI-selectable values
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, ValueEnum)]
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
    fn to_d185_value(self) -> u32 {
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

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum GrainEffect {
    Off,
    Weak,
    Strong,
}

impl GrainEffect {
    fn to_d185_value(self) -> u32 {
        match self {
            Self::Off => 1,
            Self::Weak => 2,
            Self::Strong => 3,
        }
    }
}

// ---------------------------------------------------------------------------
// Recipe settings — collected CLI overrides
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
pub struct RecipeSettings {
    pub film_sim: Option<FilmSimulation>,
    pub grain: Option<GrainEffect>,
}

impl RecipeSettings {
    pub fn is_empty(&self) -> bool {
        self.film_sim.is_none() && self.grain.is_none()
    }

    /// Human-readable summary of what will be changed.
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        if let Some(sim) = self.film_sim {
            parts.push(format!("Film Sim: {}", sim.label()));
        }
        if let Some(grain) = self.grain {
            parts.push(format!("Grain: {grain:?}"));
        }
        if parts.is_empty() {
            "camera defaults".to_string()
        } else {
            parts.join(", ")
        }
    }
}

// ---------------------------------------------------------------------------
// D185 profile modification
// ---------------------------------------------------------------------------

/// Read a u32 LE property from the profile blob at the given property index.
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

/// Write a u32 LE property into the profile blob at the given property index.
fn write_prop(profile: &mut [u8], index: usize, value: u32) {
    let offset = PROPS_OFFSET + index * 4;
    if offset + 4 <= profile.len() {
        profile[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
}

/// Apply recipe settings to a D185 profile blob (in-place).
/// Returns a human-readable summary of what was changed.
pub fn apply_recipe(profile: &mut Vec<u8>, settings: &RecipeSettings) -> String {
    if profile.len() < PROPS_OFFSET + 4 {
        return "profile too short to modify".to_string();
    }

    let mut changes = Vec::new();

    if let Some(sim) = settings.film_sim {
        let old = read_prop(profile, PROP_FILM_SIM);
        let new_val = sim.to_d185_value();
        write_prop(profile, PROP_FILM_SIM, new_val);
        changes.push(format!(
            "FilmSimulation: 0x{:X} -> 0x{:X} ({})",
            old.unwrap_or(0),
            new_val,
            sim.label()
        ));
    }

    if let Some(grain) = settings.grain {
        let old = read_prop(profile, PROP_GRAIN);
        let new_val = grain.to_d185_value();
        write_prop(profile, PROP_GRAIN, new_val);
        changes.push(format!(
            "GrainEffect: 0x{:X} -> 0x{:X} ({:?})",
            old.unwrap_or(0),
            new_val,
            grain
        ));
    }

    if changes.is_empty() {
        "no changes".to_string()
    } else {
        changes.join("; ")
    }
}

/// Decode the current film simulation from a profile for display purposes.
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
