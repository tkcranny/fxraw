//! Project config (fxraw.toml): raw_dir, outputs, overrides; load, validate, expand to jobs.

use crate::profile::{parse_exposure_comp, FilmSimulation, GrainEffect, GrainSize, RecipeSettings};
use crate::recipes;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::fs;

pub const CONFIG_FILENAME: &str = "fxraw.toml";
pub const ALL_OUTPUTS_DIR: &str = "_ALL_OUTPUTS";

// ---------------------------------------------------------------------------
// Raw TOML structures (serde)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectConfig {
    #[serde(default = "default_raw_dir")]
    pub raw_dir: String,
    #[serde(default, rename = "output")]
    pub outputs: Vec<OutputEntry>,
    #[serde(default)]
    pub overrides: HashMap<String, PerImageOverride>,
}

fn default_raw_dir() -> String {
    "./_RAF".into()
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutputEntry {
    pub recipe: String,
    #[serde(default)]
    pub suffix: Option<String>,
    #[serde(default)]
    pub film_sim: Option<String>,
    #[serde(default)]
    pub grain: Option<String>,
    #[serde(default)]
    pub grain_size: Option<String>,
    #[serde(default)]
    pub exposure_comp: Option<String>,
    #[serde(default)]
    pub wb_mode: Option<String>,
    #[serde(default)]
    pub overrides: Vec<OutputPerImageOverride>,
}

#[derive(Debug)]
pub struct OutputPerImageOverride {
    pub match_pattern: String,
    pub film_sim: Option<String>,
    pub grain: Option<String>,
    pub grain_size: Option<String>,
    pub exposure_comp: Option<String>,
    pub wb_mode: Option<String>,
}

impl OutputPerImageOverride {
    fn pattern_key(&self) -> &str {
        &self.match_pattern
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PerImageOverride {
    #[serde(default)]
    pub exclude_outputs: Vec<String>,
    #[serde(default)]
    pub film_sim: Option<String>,
    #[serde(default)]
    pub grain: Option<String>,
    #[serde(default)]
    pub grain_size: Option<String>,
    #[serde(default)]
    pub exposure_comp: Option<String>,
    #[serde(default)]
    pub wb_mode: Option<String>,
}

// TOML uses "match" which is a Rust keyword; custom Deserialize.
impl<'de> Deserialize<'de> for OutputPerImageOverride {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Helper {
            #[serde(rename = "match")]
            match_pattern: String,
            #[serde(default)]
            film_sim: Option<String>,
            #[serde(default)]
            grain: Option<String>,
            #[serde(default)]
            grain_size: Option<String>,
            #[serde(default)]
            exposure_comp: Option<String>,
            #[serde(default)]
            wb_mode: Option<String>,
        }
        let h = Helper::deserialize(deserializer)?;
        Ok(OutputPerImageOverride {
            match_pattern: h.match_pattern,
            film_sim: h.film_sim,
            grain: h.grain,
            grain_size: h.grain_size,
            exposure_comp: h.exposure_comp,
            wb_mode: h.wb_mode,
        })
    }
}

// ---------------------------------------------------------------------------
// Discovery and load
// ---------------------------------------------------------------------------

/// Find fxraw.toml in current directory, then parent directories. Returns the path to the file.
/// When running under sudo, the process cwd is often root's home; use SUDO_PWD (the
/// invoking user's cwd) so we find the project the user is actually in.
pub fn find_project_config() -> Option<PathBuf> {
    let start = std::env::var("SUDO_PWD")
        .ok()
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
        .or_else(|| std::env::current_dir().ok());
    let cwd = start?;
    let mut dir = cwd.as_path();
    loop {
        let candidate = dir.join(CONFIG_FILENAME);
        if candidate.is_file() {
            return Some(candidate);
        }
        dir = dir.parent()?;
    }
}

pub fn load_config(path: &Path) -> Result<ProjectConfig, String> {
    let s = fs::read_to_string(path).map_err(|e| format!("Reading config: {e}"))?;
    toml::from_str(&s).map_err(|e| format!("Invalid TOML: {e}"))
}

// ---------------------------------------------------------------------------
// Overlay parsing (string -> enum / value) and merge into RecipeSettings
// ---------------------------------------------------------------------------

fn parse_film_sim(s: &str) -> Option<FilmSimulation> {
    match s.to_lowercase().as_str() {
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

fn parse_grain(s: &str) -> Option<GrainEffect> {
    match s.to_lowercase().as_str() {
        "weak" => Some(GrainEffect::Weak),
        "strong" => Some(GrainEffect::Strong),
        _ => None,
    }
}

fn parse_grain_size(s: &str) -> Option<GrainSize> {
    match s.to_lowercase().as_str() {
        "small" => Some(GrainSize::Small),
        "large" => Some(GrainSize::Large),
        _ => None,
    }
}

/// Parse wb_mode into profile (white_balance, wb_temp). Valid: auto, auto-ambience, daylight,
/// shade, incandescent, fluorescent-1/2/3, or NNNK (e.g. 5500K).
fn parse_wb_mode(s: &str) -> Result<(String, Option<u32>), String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("empty wb_mode".into());
    }
    let lower = s.to_lowercase();
    if lower.ends_with('k') {
        let num: String = lower.chars().take(lower.len().saturating_sub(1)).collect();
        let num = num.trim();
        if num.is_empty() {
            return Err(format!("invalid wb_mode \"{}\": expected e.g. 5500K", s));
        }
        let k: u32 = num.parse().map_err(|_| format!("invalid wb_mode \"{}\": Kelvin must be a number", s))?;
        if !(2000..=10000).contains(&k) {
            return Err(format!("invalid wb_mode \"{}\": Kelvin typically 2000–10000", s));
        }
        return Ok(("temperature".into(), Some(k)));
    }
    let mode = match lower.as_str() {
        "auto" | "auto-white" | "auto-ambience" => "auto",
        "daylight" => "daylight",
        "shade" => "shade",
        "incandescent" => "incandescent",
        "fluorescent-1" => "fluorescent-1",
        "fluorescent-2" => "fluorescent-2",
        "fluorescent-3" => "fluorescent-3",
        _ => return Err(format!("unknown wb_mode \"{}\"; use auto, auto-ambience, daylight, shade, incandescent, fluorescent-1/2/3, or e.g. 5500K", s)),
    };
    Ok((mode.into(), None))
}

/// Override white balance in settings. Only called when config has wb_mode; otherwise
/// the recipe's white_balance (from to_settings()) is used unchanged.
fn apply_wb_mode_to_settings(base: &mut RecipeSettings, wb_mode: &str) {
    if let Ok((mode, temp)) = parse_wb_mode(wb_mode) {
        base.white_balance = Some(mode);
        base.wb_temp = temp;
        base.wb_shift_r = None;
        base.wb_shift_b = None;
    }
}

fn apply_overlay_to_settings(base: &mut RecipeSettings, overlay: &PerImageOverride) {
    if let Some(ref s) = overlay.film_sim {
        if let Some(fs) = parse_film_sim(s) {
            base.film_sim = Some(fs);
        }
    }
    if let Some(ref s) = overlay.grain {
        if let Some(g) = parse_grain(s) {
            base.grain = Some(g);
        }
    }
    if let Some(ref s) = overlay.grain_size {
        if let Some(gs) = parse_grain_size(s) {
            base.grain_size = Some(gs);
        }
    }
    if let Some(ref s) = overlay.exposure_comp {
        if let Ok(millis) = parse_exposure_comp(s) {
            base.exposure_comp = Some(millis);
        }
    }
    if let Some(ref s) = overlay.wb_mode {
        apply_wb_mode_to_settings(base, s);
    }
}

fn apply_output_overlay_to_settings(base: &mut RecipeSettings, overlay: &OutputPerImageOverride) {
    if let Some(ref s) = overlay.film_sim {
        if let Some(fs) = parse_film_sim(s) {
            base.film_sim = Some(fs);
        }
    }
    if let Some(ref s) = overlay.grain {
        if let Some(g) = parse_grain(s) {
            base.grain = Some(g);
        }
    }
    if let Some(ref s) = overlay.grain_size {
        if let Some(gs) = parse_grain_size(s) {
            base.grain_size = Some(gs);
        }
    }
    if let Some(ref s) = overlay.exposure_comp {
        if let Ok(millis) = parse_exposure_comp(s) {
            base.exposure_comp = Some(millis);
        }
    }
    if let Some(ref s) = overlay.wb_mode {
        apply_wb_mode_to_settings(base, s);
    }
}

fn apply_output_entry_overlay_to_settings(
    base: &mut RecipeSettings,
    entry: &OutputEntry,
) {
    if let Some(ref s) = entry.film_sim {
        if let Some(fs) = parse_film_sim(s) {
            base.film_sim = Some(fs);
        }
    }
    if let Some(ref s) = entry.grain {
        if let Some(g) = parse_grain(s) {
            base.grain = Some(g);
        }
    }
    if let Some(ref s) = entry.grain_size {
        if let Some(gs) = parse_grain_size(s) {
            base.grain_size = Some(gs);
        }
    }
    if let Some(ref s) = entry.exposure_comp {
        if let Ok(millis) = parse_exposure_comp(s) {
            base.exposure_comp = Some(millis);
        }
    }
    if let Some(ref s) = entry.wb_mode {
        apply_wb_mode_to_settings(base, s);
    }
}

// ---------------------------------------------------------------------------
// Match override key (literal or glob) against a RAF filename
// ---------------------------------------------------------------------------

fn pattern_matches(pattern: &str, raf_filename: &str) -> bool {
    if pattern.contains('*') {
        glob_match(pattern, raf_filename)
    } else {
        pattern == raf_filename
    }
}

fn glob_match(pattern: &str, name: &str) -> bool {
    use glob::Pattern;
    match Pattern::new(pattern) {
        Ok(p) => p.matches(name),
        Err(_) => false,
    }
}

/// Validate a glob pattern; returns error message if invalid.
fn validate_glob_pattern(pattern: &str) -> Option<String> {
    if !pattern.contains('*') {
        return None;
    }
    use glob::Pattern;
    Pattern::new(pattern).err().map(|e| format!("invalid glob pattern \"{}\": {}", pattern, e))
}

/// Collect validation errors for overlay-style settings (film_sim, grain, grain_size, exposure_comp, wb_mode).
fn validate_overlay_settings(
    context: &str,
    film_sim: Option<&String>,
    grain: Option<&String>,
    grain_size: Option<&String>,
    exposure_comp: Option<&String>,
    wb_mode: Option<&String>,
) -> Vec<String> {
    let mut errors = Vec::new();
    if let Some(s) = film_sim {
        if parse_film_sim(s).is_none() {
            errors.push(format!("{}: unknown film_sim \"{}\"", context, s));
        }
    }
    if let Some(s) = grain {
        if parse_grain(s).is_none() {
            errors.push(format!("{}: unknown grain \"{}\" (use weak or strong)", context, s));
        }
    }
    if let Some(s) = grain_size {
        if parse_grain_size(s).is_none() {
            errors.push(format!("{}: unknown grain_size \"{}\" (use small or large)", context, s));
        }
    }
    if let Some(s) = exposure_comp {
        if let Err(e) = parse_exposure_comp(s) {
            errors.push(format!("{}: invalid exposure_comp \"{}\": {}", context, s, e));
        }
    }
    if let Some(s) = wb_mode {
        if let Err(e) = parse_wb_mode(s) {
            errors.push(format!("{}: {}", context, e));
        }
    }
    errors
}

// ---------------------------------------------------------------------------
// Resolve: get base RecipeSettings for an output entry (recipe + entry overlay)
// ---------------------------------------------------------------------------

pub fn resolve_output_entry_settings(entry: &OutputEntry) -> Result<RecipeSettings, String> {
    let recipe = recipes::find(entry.recipe.as_str())
        .ok_or_else(|| format!("Recipe '{}' not found", entry.recipe))?;
    let mut settings = recipe.to_settings();
    apply_output_entry_overlay_to_settings(&mut settings, entry);
    Ok(settings)
}

/// Output directory name and filename suffix for this entry (suffix or recipe slug).
pub fn output_dir_name(entry: &OutputEntry) -> &str {
    entry.suffix.as_deref().unwrap_or(entry.recipe.as_str())
}

/// Filename suffix and output dir name (same as output_dir_name).
pub fn output_suffix(entry: &OutputEntry) -> &str {
    entry.suffix.as_deref().unwrap_or(entry.recipe.as_str())
}

/// Human-readable description of overlay settings (for validate summary).
fn overlay_description(
    film_sim: Option<&String>,
    grain: Option<&String>,
    grain_size: Option<&String>,
    exposure_comp: Option<&String>,
    wb_mode: Option<&String>,
) -> Vec<String> {
    let mut parts = Vec::new();
    if let Some(s) = film_sim {
        parts.push(format!("film_sim={}", s));
    }
    if let Some(s) = grain {
        parts.push(format!("grain={}", s));
    }
    if let Some(s) = grain_size {
        parts.push(format!("grain_size={}", s));
    }
    if let Some(s) = exposure_comp {
        parts.push(format!("exposure_comp={}", s));
    }
    if let Some(s) = wb_mode {
        parts.push(format!("wb_mode={}", s));
    }
    parts
}

/// Describe a global override entry for the validate summary.
pub fn describe_global_override(key: &str, ov: &PerImageOverride) -> (String, Vec<String>) {
    let adjustments = overlay_description(
        ov.film_sim.as_ref(),
        ov.grain.as_ref(),
        ov.grain_size.as_ref(),
        ov.exposure_comp.as_ref(),
        ov.wb_mode.as_ref(),
    );
    (key.to_string(), adjustments)
}

/// Describe a per-output override entry for the validate summary.
pub fn describe_output_override(ov: &OutputPerImageOverride) -> (String, Vec<String>) {
    let key = ov.pattern_key().to_string();
    let adjustments = overlay_description(
        ov.film_sim.as_ref(),
        ov.grain.as_ref(),
        ov.grain_size.as_ref(),
        ov.exposure_comp.as_ref(),
        ov.wb_mode.as_ref(),
    );
    (key, adjustments)
}

/// Check if global override excludes this output (by recipe slug or output name).
fn is_excluded_by_override(override_entry: &PerImageOverride, output_dir_name: &str, recipe_slug: &str) -> bool {
    override_entry
        .exclude_outputs
        .iter()
        .any(|s| s == output_dir_name || s == recipe_slug)
}

// ---------------------------------------------------------------------------
// List RAFs in raw_dir (relative to project root = dir containing fxraw.toml)
// ---------------------------------------------------------------------------

pub fn list_rafs_in_raw_dir(project_root: &Path, raw_dir: &str) -> Result<Vec<PathBuf>, String> {
    // Normalize raw_dir: "./_RAF" or "_RAF" or "_RAF/" all refer to the same place
    let raw_dir_trimmed = raw_dir.trim().trim_end_matches(std::path::MAIN_SEPARATOR);
    let raw_path = project_root.join(raw_dir_trimmed);
    if !raw_path.is_dir() {
        return Err(format!("raw_dir '{}' is not a directory or does not exist", raw_dir));
    }
    let mut rafs = Vec::new();
    for e in fs::read_dir(&raw_path).map_err(|e| format!("Reading raw_dir: {e}"))? {
        let e = e.map_err(|e| format!("Reading raw_dir entry: {e}"))?;
        let p = e.path();
        if p.is_file() {
            // Match by filename ending .raf (case-insensitive); more reliable than extension() across platforms
            let name = p.file_name().and_then(|n| n.to_str());
            if name.map_or(false, |n| n.len() >= 4 && n.get(n.len().saturating_sub(4)..).map_or(false, |s| s.eq_ignore_ascii_case(".raf"))) {
                rafs.push(p);
            }
        }
    }
    rafs.sort();
    Ok(rafs)
}

// ---------------------------------------------------------------------------
// Validate: ensure raw_dir exists and every override key matches at least one RAF
// ---------------------------------------------------------------------------

pub fn validate_config(project_root: &Path, config: &ProjectConfig) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();
    let raw_path = project_root.join(&config.raw_dir);
    if !raw_path.exists() {
        errors.push(format!("raw_dir '{}' does not exist", config.raw_dir));
    } else if !raw_path.is_dir() {
        errors.push(format!("raw_dir '{}' is not a directory", config.raw_dir));
    }
    let rafs = list_rafs_in_raw_dir(project_root, &config.raw_dir).unwrap_or_default();
    let raf_names: Vec<String> = rafs
        .iter()
        .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .collect();

    for (key, ov) in &config.overrides {
        if let Some(e) = validate_glob_pattern(key) {
            errors.push(format!("Global override key: {}", e));
        }
        let matches = raf_names.iter().any(|n| pattern_matches(key, n));
        if !matches {
            errors.push(format!(
                "Global override \"{}\" matches no RAF file in raw_dir",
                key
            ));
        }
        errors.extend(validate_overlay_settings(
            &format!("Global override \"{}\"", key),
            ov.film_sim.as_ref(),
            ov.grain.as_ref(),
            ov.grain_size.as_ref(),
            ov.exposure_comp.as_ref(),
            ov.wb_mode.as_ref(),
        ));
    }

    for (i, entry) in config.outputs.iter().enumerate() {
        let out_ctx = format!("Output {} (recipe {})", i + 1, entry.recipe);
        if recipes::find(entry.recipe.as_str()).is_none() {
            errors.push(format!("{}: recipe '{}' not found", out_ctx, entry.recipe));
        }
        errors.extend(validate_overlay_settings(
            &format!("{} (entry-level)", out_ctx),
            entry.film_sim.as_ref(),
            entry.grain.as_ref(),
            entry.grain_size.as_ref(),
            entry.exposure_comp.as_ref(),
            entry.wb_mode.as_ref(),
        ));
        for (j, ov) in entry.overrides.iter().enumerate() {
            if let Some(e) = validate_glob_pattern(ov.pattern_key()) {
                errors.push(format!("Output {} override {}: {}", i + 1, j + 1, e));
            }
            let matches = raf_names.iter().any(|n| pattern_matches(ov.pattern_key(), n));
            if !matches {
                errors.push(format!(
                    "Output {} override {}: match \"{}\" matches no RAF in raw_dir",
                    i + 1,
                    j + 1,
                    ov.pattern_key()
                ));
            }
            errors.extend(validate_overlay_settings(
                &format!("Output {} override \"{}\"", i + 1, ov.pattern_key()),
                ov.film_sim.as_ref(),
                ov.grain.as_ref(),
                ov.grain_size.as_ref(),
                ov.exposure_comp.as_ref(),
                ov.wb_mode.as_ref(),
            ));
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

// ---------------------------------------------------------------------------
// Expand config to (output_dir, batches) where each batch is (RecipeSettings, jobs)
// ---------------------------------------------------------------------------

pub struct OutputBatch {
    pub output_dir: PathBuf,
    #[allow(dead_code)]
    pub suffix: String,
    /// Batches of (settings, jobs) so we can call fuji::convert once per batch.
    pub batches: Vec<(RecipeSettings, Vec<(String, String)>)>,
}

pub fn expand_config(
    project_root: &Path,
    config: &ProjectConfig,
) -> Result<Vec<OutputBatch>, String> {
    let rafs = list_rafs_in_raw_dir(project_root, &config.raw_dir)?;
    let raf_paths: Vec<String> = rafs
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    let raf_names: Vec<String> = rafs
        .iter()
        .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .collect();

    let mut result = Vec::new();
    for entry in &config.outputs {
        let base_settings = resolve_output_entry_settings(entry)?;
        let output_dir = project_root.join(output_dir_name(entry));
        let suffix = output_suffix(entry).to_string();

        // For each RAF, compute (output_path, merged RecipeSettings)
        let mut job_settings: Vec<(String, String, RecipeSettings)> = Vec::new();
        for (raf_path, raf_name) in raf_paths.iter().zip(raf_names.iter()) {
            // Global override for this file?
            let global_ov = config
                .overrides
                .iter()
                .find(|(k, _)| pattern_matches(k, raf_name));
            if let Some((_, ov)) = global_ov {
                if is_excluded_by_override(ov, output_dir_name(entry), &entry.recipe) {
                    continue; // skip this output for this file
                }
            }
            let mut settings = base_settings.clone();
            if let Some((_, ov)) = global_ov {
                apply_overlay_to_settings(&mut settings, ov);
            }
            // Output-level override for this file (first match wins)
            if let Some(ov) = entry
                .overrides
                .iter()
                .find(|o| pattern_matches(o.pattern_key(), raf_name))
            {
                apply_output_overlay_to_settings(&mut settings, ov);
            }
            let stem = Path::new(raf_name)
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy();
            let out_filename = format!("{stem}.{suffix}.jpg");
            let output_path = output_dir.join(&out_filename).to_string_lossy().into_owned();
            job_settings.push((raf_path.clone(), output_path, settings));
        }

        // Group consecutive jobs by RecipeSettings (so we minimize convert calls)
        let mut batches: Vec<(RecipeSettings, Vec<(String, String)>)> = Vec::new();
        for (raf, out, settings) in job_settings {
            let jobs = (raf, out);
            if let Some(&mut (ref last_settings, ref mut last_jobs)) = batches.last_mut() {
                if last_settings == &settings {
                    last_jobs.push(jobs);
                    continue;
                }
            }
            batches.push((settings, vec![jobs]));
        }
        // Merge adjacent batches with same settings (we split per-job above; re-group)
        let mut merged: Vec<(RecipeSettings, Vec<(String, String)>)> = Vec::new();
        for (settings, jobs) in batches {
            if let Some((m_settings, m_jobs)) = merged.last_mut() {
                if m_settings == &settings {
                    m_jobs.extend(jobs);
                    continue;
                }
            }
            merged.push((settings, jobs));
        }

        result.push(OutputBatch {
            output_dir,
            suffix,
            batches: merged,
        });
    }
    Ok(result)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn load_config_valid_toml() {
        let toml = r#"
raw_dir = "./_RAF"
[[output]]
recipe = "classic-chrome"
"#;
        let path = std::env::temp_dir().join("fxraw_test_load.toml");
        fs::write(&path, toml).unwrap();
        let config = load_config(&path).unwrap();
        assert_eq!(config.raw_dir, "./_RAF");
        assert_eq!(config.outputs.len(), 1);
        assert_eq!(config.outputs[0].recipe, "classic-chrome");
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn load_config_invalid_toml() {
        let path = std::env::temp_dir().join("fxraw_test_invalid.toml");
        fs::write(&path, "raw_dir = [").unwrap();
        let r = load_config(&path);
        assert!(r.is_err());
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn load_config_rejects_unknown_fields() {
        let toml = r#"
raw_dir = "./_RAF"
wb_mode = "auto"
[[output]]
recipe = "classic-chrome"
"#;
        let path = std::env::temp_dir().join("fxraw_test_unknown.toml");
        fs::write(&path, toml).unwrap();
        let r = load_config(&path);
        assert!(r.is_err(), "config with unknown key wb_mode should fail to load");
        let err = r.unwrap_err();
        assert!(err.contains("unknown") && err.contains("wb_mode"), "error should mention unknown field wb_mode: {}", err);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn list_rafs_in_raw_dir_finds_raf_files() {
        let dir = tempfile::tempdir().unwrap();
        let raw_dir = dir.path().join("_RAF");
        fs::create_dir_all(&raw_dir).unwrap();
        let raf1 = raw_dir.join("a.raf");
        let raf2 = raw_dir.join("b.RAF");
        let other = raw_dir.join("other.txt");
        fs::write(&raf1, b"FUJIFILMCCD-RAW").unwrap();
        fs::write(&raf2, b"FUJIFILMCCD-RAW").unwrap();
        fs::write(&other, b"x").unwrap();
        let rafs = list_rafs_in_raw_dir(dir.path(), "_RAF").unwrap();
        assert_eq!(rafs.len(), 2);
        let names: Vec<_> = rafs
            .iter()
            .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
            .collect();
        assert!(names.contains(&"a.raf".to_string()));
        assert!(names.contains(&"b.RAF".to_string()));
    }

    #[test]
    fn list_rafs_raw_dir_nonexistent() {
        let dir = tempfile::tempdir().unwrap();
        let r = list_rafs_in_raw_dir(dir.path(), "_MISSING");
        assert!(r.is_err());
    }

    #[test]
    fn validate_config_raw_dir_missing() {
        let config = ProjectConfig {
            raw_dir: "./_MISSING".into(),
            outputs: vec![],
            overrides: HashMap::new(),
        };
        let dir = tempfile::tempdir().unwrap();
        let r = validate_config(dir.path(), &config);
        assert!(r.is_err());
    }

    #[test]
    fn parse_wb_mode_valid() {
        assert!(parse_wb_mode("auto").is_ok());
        assert!(parse_wb_mode("auto-ambience").is_ok());
        assert!(parse_wb_mode("daylight").is_ok());
        assert!(parse_wb_mode("5500K").is_ok());
        let (mode, temp) = parse_wb_mode("5500K").unwrap();
        assert_eq!(mode, "temperature");
        assert_eq!(temp, Some(5500));
    }

    #[test]
    fn parse_wb_mode_invalid() {
        assert!(parse_wb_mode("invalid-wb").is_err());
        assert!(parse_wb_mode("").is_err());
    }

    #[test]
    fn validate_config_rejects_bad_wb_mode() {
        let dir = tempfile::tempdir().unwrap();
        let raw_dir = dir.path().join("_RAF");
        fs::create_dir_all(&raw_dir).unwrap();
        fs::write(raw_dir.join("one.raf"), b"FUJIFILMCCD-RAW").unwrap();
        let config = ProjectConfig {
            raw_dir: "./_RAF".into(),
            outputs: vec![OutputEntry {
                recipe: "classic-chrome".into(),
                suffix: None,
                film_sim: None,
                grain: None,
                grain_size: None,
                exposure_comp: None,
                wb_mode: Some("not-a-wb-mode".into()),
                overrides: vec![],
            }],
            overrides: HashMap::new(),
        };
        let r = validate_config(dir.path(), &config);
        let errs = r.unwrap_err();
        assert!(errs.iter().any(|e| e.contains("wb_mode") && e.contains("not-a-wb-mode")));
    }

    #[test]
    fn validate_config_rejects_bad_settings() {
        let dir = tempfile::tempdir().unwrap();
        let raw_dir = dir.path().join("_RAF");
        fs::create_dir_all(&raw_dir).unwrap();
        fs::write(raw_dir.join("one.raf"), b"FUJIFILMCCD-RAW").unwrap();
        let config = ProjectConfig {
            raw_dir: "./_RAF".into(),
            outputs: vec![OutputEntry {
                recipe: "classic-chrome".into(),
                suffix: None,
                film_sim: Some("not-a-film-sim".into()),
                grain: None,
                grain_size: None,
                exposure_comp: None,
                wb_mode: None,
                overrides: vec![],
            }],
            overrides: HashMap::new(),
        };
        let r = validate_config(dir.path(), &config);
        let errs = r.unwrap_err();
        assert!(!errs.is_empty());
        assert!(errs.iter().any(|e| e.contains("unknown film_sim") && e.contains("not-a-film-sim")));
    }

    #[test]
    fn expand_config_one_raf_one_output() {
        let dir = tempfile::tempdir().unwrap();
        let raw_dir = dir.path().join("_RAF");
        fs::create_dir_all(&raw_dir).unwrap();
        fs::write(raw_dir.join("one.raf"), b"FUJIFILMCCD-RAW").unwrap();
        let config = ProjectConfig {
            raw_dir: "./_RAF".into(),
            outputs: vec![OutputEntry {
                recipe: "classic-chrome".into(),
                suffix: None,
                film_sim: None,
                grain: None,
                grain_size: None,
                exposure_comp: None,
                wb_mode: None,
                overrides: vec![],
            }],
            overrides: HashMap::new(),
        };
        let batches = expand_config(dir.path(), &config).unwrap();
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].batches.len(), 1);
        let (_, jobs) = &batches[0].batches[0];
        assert_eq!(jobs.len(), 1);
        assert!(jobs[0].0.ends_with("one.raf"));
        assert!(jobs[0].1.ends_with("one.classic-chrome.jpg"));
    }
}
