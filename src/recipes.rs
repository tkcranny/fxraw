#![allow(dead_code)]

use crate::profile::{ChromeLevel, FilmSimulation, GrainEffect, GrainSize, RecipeSettings};
use serde::Deserialize;
use std::sync::LazyLock;

static RECIPES_JSON: &str = include_str!("../data/recipes.json");

static RECIPES: LazyLock<Vec<Recipe>> =
    LazyLock::new(|| serde_json::from_str(RECIPES_JSON).expect("invalid recipes.json"));

#[derive(Debug, Deserialize)]
pub struct Recipe {
    pub name: String,
    pub slug: String,
    pub film_sim: String,
    #[serde(default)]
    pub highlight: Option<f64>,
    #[serde(default)]
    pub shadow: Option<f64>,
    #[serde(default)]
    pub color: Option<f64>,
    #[serde(default)]
    pub noise_reduction: Option<f64>,
    #[serde(default)]
    pub sharpness: Option<f64>,
    #[serde(default)]
    pub clarity: Option<f64>,
    #[serde(default)]
    pub grain: Option<String>,
    #[serde(default)]
    pub grain_size: Option<String>,
    #[serde(default)]
    pub chrome_effect: Option<String>,
    #[serde(default)]
    pub chrome_blue: Option<String>,
    #[serde(default)]
    pub dynamic_range: Option<u32>,
    #[serde(default)]
    pub white_balance: Option<String>,
    #[serde(default)]
    pub wb_temp: Option<u32>,
    #[serde(default)]
    pub wb_shift_r: Option<f64>,
    #[serde(default)]
    pub wb_shift_b: Option<f64>,
    #[serde(default)]
    pub iso: Option<String>,
    #[serde(default)]
    pub exposure_comp: Option<String>,
}

impl Recipe {
    fn parse_film_sim(&self) -> Option<FilmSimulation> {
        match self.film_sim.as_str() {
            "provia" => Some(FilmSimulation::Provia),
            "velvia" => Some(FilmSimulation::Velvia),
            "astia" => Some(FilmSimulation::Astia),
            "classic-chrome" => Some(FilmSimulation::ClassicChrome),
            "classic-neg" => Some(FilmSimulation::ClassicNeg),
            "pro-neg-hi" => Some(FilmSimulation::ProNegHi),
            "pro-neg-std" => Some(FilmSimulation::ProNegStd),
            "eterna" => Some(FilmSimulation::Eterna),
            "eterna-bleach-bypass" => Some(FilmSimulation::EternaBleachBypass),
            "acros" => Some(FilmSimulation::Acros),
            "acros-ye" => Some(FilmSimulation::AcrosYe),
            "acros-r" => Some(FilmSimulation::AcrosR),
            "acros-g" => Some(FilmSimulation::AcrosG),
            "monochrome" => Some(FilmSimulation::Monochrome),
            "monochrome-ye" => Some(FilmSimulation::MonochromeYe),
            "monochrome-r" => Some(FilmSimulation::MonochromeR),
            "monochrome-g" => Some(FilmSimulation::MonochromeG),
            "sepia" => Some(FilmSimulation::Sepia),
            "nostalgic-neg" => Some(FilmSimulation::NostalgicNeg),
            "reala-ace" => Some(FilmSimulation::RealaAce),
            _ => None,
        }
    }

    fn parse_grain(&self) -> Option<GrainEffect> {
        match self.grain.as_deref() {
            Some("weak") => Some(GrainEffect::Weak),
            Some("strong") => Some(GrainEffect::Strong),
            _ => None,
        }
    }

    fn parse_grain_size(&self) -> Option<GrainSize> {
        match self.grain_size.as_deref() {
            Some("small") => Some(GrainSize::Small),
            Some("large") => Some(GrainSize::Large),
            _ => None,
        }
    }

    fn parse_chrome(val: &Option<String>) -> Option<ChromeLevel> {
        match val.as_deref() {
            Some("off") => Some(ChromeLevel::Off),
            Some("weak") => Some(ChromeLevel::Weak),
            Some("strong") => Some(ChromeLevel::Strong),
            _ => None,
        }
    }

    pub fn to_settings(&self) -> RecipeSettings {
        RecipeSettings {
            film_sim: self.parse_film_sim(),
            grain: self.parse_grain(),
            grain_size: self.parse_grain_size(),
            highlight: self.highlight.map(|v| v as f32),
            shadow: self.shadow.map(|v| v as f32),
            color: self.color.map(|v| v as i32),
            sharpness: self.sharpness.map(|v| v as i32),
            noise_reduction: self.noise_reduction.map(|v| v as i32),
            clarity: self.clarity.map(|v| v as i32),
            white_balance: self.white_balance.clone(),
            wb_temp: self.wb_temp,
            wb_shift_r: self.wb_shift_r.map(|v| v as i32),
            wb_shift_b: self.wb_shift_b.map(|v| v as i32),
            chrome_effect: Self::parse_chrome(&self.chrome_effect),
            chrome_blue: Self::parse_chrome(&self.chrome_blue),
            dynamic_range: self.dynamic_range,
            exposure_comp: None,
        }
    }
}

pub fn all() -> &'static [Recipe] {
    &RECIPES
}

pub fn find(query: &str) -> Option<&'static Recipe> {
    let q = query.to_lowercase();
    // Exact slug match first
    if let Some(r) = RECIPES.iter().find(|r| r.slug == q) {
        return Some(r);
    }
    // Substring match on slug or name
    RECIPES
        .iter()
        .find(|r| r.slug.contains(&q) || r.name.to_lowercase().contains(&q))
}

pub fn list_recipes() {
    println!("{} built-in recipes (X100VI compatible):\n", RECIPES.len());
    for r in RECIPES.iter() {
        let dr = match r.dynamic_range {
            Some(v) => format!("DR{v}"),
            None => "auto".into(),
        };
        let wb = match (r.white_balance.as_deref(), r.wb_temp) {
            (Some("temperature"), Some(t)) => format!("{t}K"),
            (Some(wb), _) => wb.to_string(),
            (None, _) => "auto".into(),
        };
        println!(
            "  {:<35} {:<22} {:<6} {}",
            r.slug, r.film_sim, dr, wb
        );
    }
}

pub fn show_recipe(r: &Recipe) {
    println!("{}", r.name);
    println!("{}", "─".repeat(r.name.len()));

    let row = |label: &str, val: String| {
        println!("  {:<18} {}", label, val);
    };

    row("Slug", r.slug.clone());
    row("Film Simulation", r.film_sim.clone());
    row("Dynamic Range", match r.dynamic_range {
        Some(dr) => format!("DR{dr}"),
        None => "auto".into(),
    });

    // Grain: "weak / small" or "off"
    let grain = match r.grain.as_deref() {
        Some(g) => {
            let size = r.grain_size.as_deref().unwrap_or("small");
            format!("{g} / {size}")
        }
        None => "off".into(),
    };
    row("Grain", grain);

    // Color Chrome + Blue on one line
    let chrome = r.chrome_effect.as_deref().unwrap_or("off");
    let chrome_b = r.chrome_blue.as_deref().unwrap_or("off");
    row("Color Effect", format!("chrome:{chrome}  blue:{chrome_b}"));

    // WB on one line: base + R/B shifts
    let wb_base = match (r.white_balance.as_deref(), r.wb_temp) {
        (Some("temperature"), Some(t)) => format!("{t}K"),
        (Some(wb), _) => wb.to_string(),
        (None, _) => "auto".into(),
    };
    let shift_r = r.wb_shift_r.map(|v| format!("{v:+}")).unwrap_or("0".into());
    let shift_b = r.wb_shift_b.map(|v| format!("{v:+}")).unwrap_or("0".into());
    row("White Balance", format!("{wb_base}  R:{shift_r} B:{shift_b}"));

    let fmt = |v: f64| -> String {
        if v == 0.0 { "0".into() } else { format!("{v:+}") }
    };

    // Tone: highlight & shadow on one line
    let hl = r.highlight.map(|v| fmt(v)).unwrap_or("0".into());
    let sh = r.shadow.map(|v| fmt(v)).unwrap_or("0".into());
    row("Tone", format!("highlight:{hl}  shadow:{sh}"));

    let fmti = |v: f64| -> String {
        if v == 0.0 { "0".into() } else { format!("{:+}", v as i32) }
    };
    row("Color", r.color.map(|v| fmti(v)).unwrap_or("0".into()));
    row("Sharpness", r.sharpness.map(|v| fmti(v)).unwrap_or("0".into()));
    row("NR", r.noise_reduction.map(|v| fmti(v)).unwrap_or("0".into()));
    row("Clarity", r.clarity.map(|v| fmti(v)).unwrap_or("0".into()));
    row("ISO", r.iso.clone().unwrap_or("auto".into()));
    row("Exposure", r.exposure_comp.clone().unwrap_or("0".into()));
}
