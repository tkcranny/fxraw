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
    pub white_balance: Option<String>,
    #[serde(default)]
    pub wb_temp: Option<u32>,
    #[serde(default)]
    pub wb_shift_r: Option<f64>,
    #[serde(default)]
    pub wb_shift_b: Option<f64>,
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
        println!(
            "  {:<35} {:>20}   {}",
            r.slug,
            r.name,
            r.film_sim
        );
    }
}
