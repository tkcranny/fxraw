//! Project config (fjx.toml): raw_dir, outputs, overrides; load, validate, expand to jobs.

use crate::profile::{parse_exposure_comp, FilmSimulation, GrainEffect, GrainSize, RecipeSettings};
use crate::recipes;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::fs;

pub const CONFIG_FILENAME: &str = "fjx.toml";
pub const ALL_OUTPUTS_DIR: &str = "_ALL_OUTPUTS";

// ---------------------------------------------------------------------------
// Raw TOML structures (serde)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
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
pub struct OutputEntry {
    pub recipe: String,
    #[serde(default)]
    pub name: Option<String>,
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
    pub keep_wb: Option<bool>,
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
    pub keep_wb: Option<bool>,
}

impl OutputPerImageOverride {
    fn pattern_key(&self) -> &str {
        &self.match_pattern
    }
}

#[derive(Debug, Deserialize)]
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
    pub keep_wb: Option<bool>,
}

// TOML uses "match" which is a Rust keyword; custom Deserialize.
impl<'de> Deserialize<'de> for OutputPerImageOverride {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
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
            keep_wb: Option<bool>,
        }
        let h = Helper::deserialize(deserializer)?;
        Ok(OutputPerImageOverride {
            match_pattern: h.match_pattern,
            film_sim: h.film_sim,
            grain: h.grain,
            grain_size: h.grain_size,
            exposure_comp: h.exposure_comp,
            keep_wb: h.keep_wb,
        })
    }
}

// ---------------------------------------------------------------------------
// Discovery and load
// ---------------------------------------------------------------------------

/// Find fjx.toml in current directory, then parent directories. Returns the path to the file.
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
    if overlay.keep_wb == Some(true) {
        base.white_balance = None;
        base.wb_temp = None;
        base.wb_shift_r = None;
        base.wb_shift_b = None;
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
    if overlay.keep_wb == Some(true) {
        base.white_balance = None;
        base.wb_temp = None;
        base.wb_shift_r = None;
        base.wb_shift_b = None;
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
    if entry.keep_wb == Some(true) {
        base.white_balance = None;
        base.wb_temp = None;
        base.wb_shift_r = None;
        base.wb_shift_b = None;
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

/// Output directory name for this entry (name or recipe slug).
pub fn output_dir_name(entry: &OutputEntry) -> &str {
    entry
        .name
        .as_deref()
        .unwrap_or(entry.recipe.as_str())
}

/// Suffix for filenames (suffix or recipe slug).
pub fn output_suffix(entry: &OutputEntry) -> &str {
    entry
        .suffix
        .as_deref()
        .unwrap_or(entry.recipe.as_str())
}

/// Check if global override excludes this output (by recipe slug or output name).
fn is_excluded_by_override(override_entry: &PerImageOverride, output_dir_name: &str, recipe_slug: &str) -> bool {
    override_entry
        .exclude_outputs
        .iter()
        .any(|s| s == output_dir_name || s == recipe_slug)
}

// ---------------------------------------------------------------------------
// List RAFs in raw_dir (relative to project root = dir containing fjx.toml)
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

    for (key, _) in &config.overrides {
        let matches = raf_names.iter().any(|n| pattern_matches(key, n));
        if !matches {
            errors.push(format!(
                "Override key \"{}\" matches no RAF file in raw_dir",
                key
            ));
        }
    }

    for (i, entry) in config.outputs.iter().enumerate() {
        if recipes::find(entry.recipe.as_str()).is_none() {
            errors.push(format!(
                "Output {}: recipe '{}' not found",
                i + 1,
                entry.recipe
            ));
        }
        for (j, ov) in entry.overrides.iter().enumerate() {
            let matches = raf_names.iter().any(|n| pattern_matches(ov.pattern_key(), n));
            if !matches {
                errors.push(format!(
                    "Output {} override {}: match \"{}\" matches no RAF in raw_dir",
                    i + 1,
                    j + 1,
                    ov.pattern_key()
                ));
            }
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
        let path = std::env::temp_dir().join("fjx_test_load.toml");
        fs::write(&path, toml).unwrap();
        let config = load_config(&path).unwrap();
        assert_eq!(config.raw_dir, "./_RAF");
        assert_eq!(config.outputs.len(), 1);
        assert_eq!(config.outputs[0].recipe, "classic-chrome");
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn load_config_invalid_toml() {
        let path = std::env::temp_dir().join("fjx_test_invalid.toml");
        fs::write(&path, "raw_dir = [").unwrap();
        let r = load_config(&path);
        assert!(r.is_err());
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
    fn expand_config_one_raf_one_output() {
        let dir = tempfile::tempdir().unwrap();
        let raw_dir = dir.path().join("_RAF");
        fs::create_dir_all(&raw_dir).unwrap();
        fs::write(raw_dir.join("one.raf"), b"FUJIFILMCCD-RAW").unwrap();
        let config = ProjectConfig {
            raw_dir: "./_RAF".into(),
            outputs: vec![OutputEntry {
                recipe: "classic-chrome".into(),
                name: None,
                suffix: None,
                film_sim: None,
                grain: None,
                grain_size: None,
                exposure_comp: None,
                keep_wb: None,
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
