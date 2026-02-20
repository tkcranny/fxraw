use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

use crate::recipes;

struct ExifData {
    fields: HashMap<String, String>,
}

impl ExifData {
    fn get(&self, key: &str) -> Option<&str> {
        self.fields.get(key).map(|s| s.as_str()).filter(|s| *s != "-")
    }

    fn get_num(&self, key: &str) -> Option<f64> {
        self.get(key).and_then(|s| {
            // "0 (normal)" / "+2 (high)" / "-4 (weakest)" → extract leading number
            let num_str = s.split_whitespace().next().unwrap_or(s);
            num_str.parse::<f64>().ok()
        })
    }
}

fn run_exiftool(path: &Path) -> Result<ExifData, String> {
    let tags = [
        "-FujiFilm:FilmMode",
        "-FujiFilm:GrainEffectRoughness",
        "-FujiFilm:GrainEffectSize",
        "-FujiFilm:HighlightTone",
        "-FujiFilm:ShadowTone",
        "-FujiFilm:Saturation",
        "-FujiFilm:Sharpness",
        "-FujiFilm:NoiseReduction",
        "-FujiFilm:Clarity",
        "-FujiFilm:WhiteBalance",
        "-FujiFilm:WhiteBalanceFineTune",
        "-FujiFilm:ColorChromeEffect",
        "-FujiFilm:ColorChromeFXBlue",
        "-FujiFilm:DynamicRange",
        "-ExposureCompensation",
        "-ISO",
        "-FujiFilm:ColorTemperature",
    ];

    let output = Command::new("exiftool")
        .args(["-s", "-f"])
        .args(&tags)
        .arg(path)
        .output()
        .map_err(|e| format!("failed to run exiftool: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("exiftool failed: {stderr}"));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut fields = HashMap::new();
    for line in stdout.lines() {
        if let Some((key, val)) = line.split_once(':') {
            fields.insert(key.trim().to_string(), val.trim().to_string());
        }
    }

    Ok(ExifData { fields })
}

fn film_mode_to_slug(exif_name: &str) -> &str {
    match exif_name {
        "Provia" => "provia",
        "Velvia" => "velvia",
        "Astia" => "astia",
        "Classic Chrome" => "classic-chrome",
        "Classic Negative" => "classic-neg",
        "Pro Neg. Hi" => "pro-neg-hi",
        "Pro Neg. Std" => "pro-neg-std",
        "Eterna" => "eterna",
        "Eterna Bleach Bypass" => "eterna-bleach-bypass",
        "Acros" => "acros",
        "Acros+Ye Filter" => "acros-ye",
        "Acros+R Filter" => "acros-r",
        "Acros+G Filter" => "acros-g",
        "B&W" => "monochrome",
        "B&W+Ye Filter" => "monochrome-ye",
        "B&W+R Filter" => "monochrome-r",
        "B&W+G Filter" => "monochrome-g",
        "Sepia" => "sepia",
        "Nostalgic Neg" => "nostalgic-neg",
        "Reala Ace" => "reala-ace",
        _ => exif_name,
    }
}

fn wb_exif_to_slug(exif_name: &str) -> &str {
    match exif_name {
        "Auto" | "Auto (white priority)" | "Auto (ambiance priority)" => "auto",
        "Daylight" => "daylight",
        "Shade" => "shade",
        "Incandescent" => "incandescent",
        "Daylight Fluorescent" => "fluorescent-1",
        "Day White Fluorescent" => "fluorescent-2",
        "White Fluorescent" => "fluorescent-3",
        "Kelvin" => "temperature",
        "Underwater" => "underwater",
        _ => exif_name,
    }
}

fn chrome_exif_to_slug(exif_val: &str) -> &str {
    match exif_val.to_lowercase().as_str() {
        "off" => "off",
        "weak" => "weak",
        "strong" => "strong",
        _ => exif_val,
    }
}

fn parse_wb_fine(s: &str) -> (i32, i32) {
    // "Red +20, Blue -120" → (1, -6) in recipe units (÷20)
    let mut r = 0i32;
    let mut b = 0i32;
    for part in s.split(',') {
        let part = part.trim();
        if let Some(num_str) = part.strip_prefix("Red ") {
            if let Ok(v) = num_str.trim().parse::<i32>() {
                r = v / 20;
            }
        } else if let Some(num_str) = part.strip_prefix("Blue ") {
            if let Ok(v) = num_str.trim().parse::<i32>() {
                b = v / 20;
            }
        }
    }
    (r, b)
}

fn dr_exif_to_value(s: &str) -> Option<u32> {
    match s {
        "Standard" => Some(100),
        "Wide1" | "Wide 1" => Some(200),
        "Wide2" | "Wide 2" => Some(400),
        _ => {
            // Try parsing e.g. "200%"
            s.trim_end_matches('%').parse::<u32>().ok()
        }
    }
}

fn display_settings(exif: &ExifData) {
    let row = |label: &str, val: &str| {
        println!("  {:<18} {}", label, val);
    };

    let film = exif.get("FilmMode").unwrap_or("unknown");
    row("Film Simulation", film_mode_to_slug(film));

    let dr = exif
        .get("DynamicRange")
        .and_then(dr_exif_to_value)
        .map(|v| format!("DR{v}"))
        .unwrap_or("auto".into());
    row("Dynamic Range", &dr);

    let grain = exif.get("GrainEffectRoughness").unwrap_or("Off");
    let grain_size = exif.get("GrainEffectSize").unwrap_or("Small");
    if grain.eq_ignore_ascii_case("off") {
        row("Grain", "off");
    } else {
        row(
            "Grain",
            &format!("{} / {}", grain.to_lowercase(), grain_size.to_lowercase()),
        );
    }

    let chrome = exif.get("ColorChromeEffect").unwrap_or("Off");
    let chrome_b = exif.get("ColorChromeFXBlue").unwrap_or("Off");
    row(
        "Color Effect",
        &format!(
            "chrome:{}  blue:{}",
            chrome.to_lowercase(),
            chrome_b.to_lowercase()
        ),
    );

    let wb = exif.get("WhiteBalance").unwrap_or("Auto");
    let wb_slug = wb_exif_to_slug(wb);
    let (shift_r, shift_b) = exif
        .get("WhiteBalanceFineTune")
        .map(|s| parse_wb_fine(s))
        .unwrap_or((0, 0));
    let wb_display = if wb_slug == "temperature" {
        let temp = exif.get("ColorTemperature").unwrap_or("?");
        format!("{temp}K")
    } else {
        wb_slug.to_string()
    };
    let fmt_shift = |v: i32| -> String {
        if v == 0 {
            "0".into()
        } else {
            format!("{v:+}")
        }
    };
    row(
        "White Balance",
        &format!(
            "{}  R:{} B:{}",
            wb_display,
            fmt_shift(shift_r),
            fmt_shift(shift_b)
        ),
    );

    let hl = exif.get_num("HighlightTone").unwrap_or(0.0);
    let sh = exif.get_num("ShadowTone").unwrap_or(0.0);
    let fmt = |v: f64| -> String {
        if v == 0.0 {
            "0".into()
        } else {
            format!("{v:+}")
        }
    };
    row("Tone", &format!("highlight:{}  shadow:{}", fmt(hl), fmt(sh)));

    let fmti = |v: f64| -> String {
        if v == 0.0 {
            "0".into()
        } else {
            format!("{:+}", v as i32)
        }
    };
    row(
        "Color",
        &fmti(exif.get_num("Saturation").unwrap_or(0.0)),
    );
    row(
        "Sharpness",
        &fmti(exif.get_num("Sharpness").unwrap_or(0.0)),
    );
    row("NR", &fmti(exif.get_num("NoiseReduction").unwrap_or(0.0)));
    row("Clarity", &fmti(exif.get_num("Clarity").unwrap_or(0.0)));

    let iso = exif.get("ISO").unwrap_or("?");
    row("ISO", iso);

    let ev = exif
        .get("ExposureCompensation")
        .unwrap_or("0")
        .to_string();
    row("Exposure", &ev);
}

struct MatchScore {
    score: f64,
    max_score: f64,
}

fn score_recipe(exif: &ExifData, r: &recipes::Recipe) -> MatchScore {
    let mut score = 0.0f64;
    let mut max = 0.0f64;

    // Film sim (heavily weighted — most defining characteristic)
    let exif_film = exif
        .get("FilmMode")
        .map(film_mode_to_slug)
        .unwrap_or("");
    max += 5.0;
    if exif_film == r.film_sim {
        score += 5.0;
    }

    // Dynamic range
    let exif_dr = exif.get("DynamicRange").and_then(dr_exif_to_value);
    if let (Some(exif_v), Some(rec_v)) = (exif_dr, r.dynamic_range) {
        max += 2.0;
        if exif_v == rec_v {
            score += 2.0;
        }
    }

    // Numeric fields (1 point each, partial credit for close values)
    let num_fields: &[(&str, Option<f64>)] = &[
        ("HighlightTone", r.highlight),
        ("ShadowTone", r.shadow),
        ("Saturation", r.color),
        ("Sharpness", r.sharpness),
        ("NoiseReduction", r.noise_reduction),
        ("Clarity", r.clarity),
    ];
    for (tag, recipe_val) in num_fields {
        if let (Some(exif_v), Some(rec_v)) = (exif.get_num(tag), recipe_val) {
            max += 1.0;
            let diff = (exif_v - rec_v).abs();
            if diff < 0.01 {
                score += 1.0;
            } else if diff <= 1.0 {
                score += 0.5;
            }
        }
    }

    // Grain
    let exif_grain = exif
        .get("GrainEffectRoughness")
        .unwrap_or("Off")
        .to_lowercase();
    let rec_grain = r.grain.as_deref().unwrap_or("off").to_lowercase();
    max += 1.0;
    if exif_grain == rec_grain {
        score += 1.0;
    }

    if exif_grain != "off" && rec_grain != "off" {
        let exif_gs = exif
            .get("GrainEffectSize")
            .unwrap_or("Small")
            .to_lowercase();
        let rec_gs = r.grain_size.as_deref().unwrap_or("small").to_lowercase();
        max += 0.5;
        if exif_gs == rec_gs {
            score += 0.5;
        }
    }

    // Chrome effects
    let exif_chrome = chrome_exif_to_slug(exif.get("ColorChromeEffect").unwrap_or("Off"));
    let rec_chrome = r.chrome_effect.as_deref().unwrap_or("off");
    max += 1.0;
    if exif_chrome.eq_ignore_ascii_case(rec_chrome) {
        score += 1.0;
    }

    let exif_cb = chrome_exif_to_slug(exif.get("ColorChromeFXBlue").unwrap_or("Off"));
    let rec_cb = r.chrome_blue.as_deref().unwrap_or("off");
    max += 1.0;
    if exif_cb.eq_ignore_ascii_case(rec_cb) {
        score += 1.0;
    }

    // White balance mode
    let exif_wb = exif
        .get("WhiteBalance")
        .map(wb_exif_to_slug)
        .unwrap_or("auto");
    let rec_wb = r.white_balance.as_deref().unwrap_or("auto");
    max += 2.0;
    if exif_wb == rec_wb {
        score += 2.0;
    }

    // WB color temperature (for Kelvin mode)
    if exif_wb == "temperature" && rec_wb == "temperature" {
        if let (Some(exif_temp), Some(rec_temp)) = (
            exif.get("ColorTemperature").and_then(|s| s.parse::<u32>().ok()),
            r.wb_temp,
        ) {
            max += 1.0;
            let diff = (exif_temp as i32 - rec_temp as i32).unsigned_abs();
            if diff <= 100 {
                score += 1.0;
            } else if diff <= 500 {
                score += 0.5;
            }
        }
    }

    // WB shifts
    let (exif_r, exif_b) = exif
        .get("WhiteBalanceFineTune")
        .map(|s| parse_wb_fine(s))
        .unwrap_or((0, 0));
    if let Some(rec_r) = r.wb_shift_r {
        max += 0.5;
        if exif_r == rec_r as i32 {
            score += 0.5;
        }
    }
    if let Some(rec_b) = r.wb_shift_b {
        max += 0.5;
        if exif_b == rec_b as i32 {
            score += 0.5;
        }
    }

    MatchScore {
        score,
        max_score: max,
    }
}

pub fn run(path: &str) {
    let p = Path::new(path);
    if !p.exists() {
        eprintln!("File not found: {path}");
        std::process::exit(1);
    }

    let exif = match run_exiftool(p) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };

    let filename = p.file_name().unwrap_or_default().to_string_lossy();
    println!("{filename}");
    println!("{}", "─".repeat(filename.len()));
    display_settings(&exif);

    // Score all recipes
    let all = recipes::all();
    let mut scored: Vec<_> = all
        .iter()
        .map(|r| {
            let m = score_recipe(&exif, r);
            (r, m.score, m.max_score)
        })
        .collect();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

    println!("\nClosest recipes:");
    let top = &scored[..scored.len().min(5)];
    for (r, score, max) in top {
        let pct = if *max > 0.0 {
            score / max * 100.0
        } else {
            0.0
        };
        println!("  {:<35} {:>5.1}%  ({:.1}/{:.1})", r.slug, pct, score, max);
    }
}
