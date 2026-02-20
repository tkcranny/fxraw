#!/usr/bin/env bash
set -euo pipefail

BINARY="./target/release/fuji-usb-test"
RAF="DSCF7496.RAF"
RECIPES_JSON="data/recipes.json"
OUT_DIR="/tmp"
CONVERT=false
RECIPE_FILTER=""

usage() {
    echo "Usage: $0 [OPTIONS]"
    echo ""
    echo "Batch-verify EXIF metadata of convert outputs against recipes.json."
    echo ""
    echo "Options:"
    echo "  --convert         Run camera conversion for each recipe before verifying"
    echo "  --recipe SLUG     Verify only this recipe (default: all)"
    echo "  --raf FILE        Input RAF file (default: $RAF)"
    echo "  --out-dir DIR     Output directory for JPEGs (default: $OUT_DIR)"
    echo "  --help            Show this help"
    exit 0
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --convert)   CONVERT=true; shift ;;
        --recipe)    RECIPE_FILTER="$2"; shift 2 ;;
        --raf)       RAF="$2"; shift 2 ;;
        --out-dir)   OUT_DIR="$2"; shift 2 ;;
        --help|-h)   usage ;;
        *)           echo "Unknown option: $1"; usage ;;
    esac
done

RESULTS_FILE=$(mktemp /tmp/recipe_verify_XXXXXX.tsv)
trap "rm -f '$RESULTS_FILE'" EXIT

# ───────────────────────────────────────────────────────────────
# Gather recipe slugs
# ───────────────────────────────────────────────────────────────
if [[ -n "$RECIPE_FILTER" ]]; then
    SLUGS=("$RECIPE_FILTER")
else
    SLUGS=()
    while IFS= read -r line; do
        SLUGS+=("$line")
    done < <(python3 -c "
import json
data = json.load(open('$RECIPES_JSON'))
for r in data:
    print(r['slug'])
")
fi

TOTAL=${#SLUGS[@]}
echo "═══════════════════════════════════════════════════════════════════════"
echo " Batch EXIF Verification — $TOTAL recipe(s)"
echo "═══════════════════════════════════════════════════════════════════════"
echo ""

# ───────────────────────────────────────────────────────────────
# Helpers
# ───────────────────────────────────────────────────────────────
read_recipe_field() {
    local slug="$1" field="$2"
    python3 -c "
import json, sys
data = json.load(open('$RECIPES_JSON'))
r = next((x for x in data if x['slug'] == '$slug'), None)
if r is None:
    print('RECIPE_NOT_FOUND', file=sys.stderr); sys.exit(1)
val = r.get('$field')
if val is None:
    print('UNSET')
else:
    print(val)
"
}

film_sim_display() {
    case "$1" in
        provia)                echo "Provia" ;;
        velvia)                echo "Velvia" ;;
        astia)                 echo "Astia" ;;
        classic-chrome)        echo "Classic Chrome" ;;
        classic-neg)           echo "Classic Negative" ;;
        pro-neg-hi)            echo "Pro Neg. Hi" ;;
        pro-neg-std)           echo "Pro Neg. Std" ;;
        eterna)                echo "Eterna" ;;
        eterna-bleach-bypass)  echo "Eterna Bleach Bypass" ;;
        acros)                 echo "Acros" ;;
        acros-ye)              echo "Acros+Ye Filter" ;;
        acros-r)               echo "Acros+R Filter" ;;
        acros-g)               echo "Acros+G Filter" ;;
        monochrome)            echo "B&W" ;;
        monochrome-ye)         echo "B&W+Ye Filter" ;;
        monochrome-r)          echo "B&W+R Filter" ;;
        monochrome-g)          echo "B&W+G Filter" ;;
        sepia)                 echo "Sepia" ;;
        nostalgic-neg)         echo "Nostalgic Neg" ;;
        reala-ace)             echo "Reala Ace" ;;
        *)                     echo "$1" ;;
    esac
}

wb_display() {
    case "$1" in
        auto|auto-white|auto-ambience)  echo "Auto" ;;
        daylight)       echo "Daylight" ;;
        shade)          echo "Shade" ;;
        incandescent)   echo "Incandescent" ;;
        fluorescent-1)  echo "Daylight Fluorescent" ;;
        fluorescent-2)  echo "Day White Fluorescent" ;;
        fluorescent-3)  echo "White Fluorescent" ;;
        temperature)    echo "Kelvin" ;;
        *)              echo "$1" ;;
    esac
}

matches_ci() {
    local actual expected
    actual=$(echo "$1" | tr '[:upper:]' '[:lower:]')
    expected=$(echo "$2" | tr '[:upper:]' '[:lower:]')
    [[ "$actual" == *"$expected"* ]]
}

int_of() {
    python3 -c "print(int(float('$1')))"
}

float_match() {
    python3 -c "print('PASS' if abs(float('$1') - float('$2')) < 0.01 else 'FAIL')" 2>/dev/null || echo "FAIL"
}

# Record a result line: slug|field|expected|actual|status|note
record() {
    printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$1" "$2" "$3" "$4" "$5" "$6" >> "$RESULTS_FILE"
}

# ───────────────────────────────────────────────────────────────
# Per-recipe verification
# ───────────────────────────────────────────────────────────────
verify_recipe() {
    local slug="$1"
    local jpg="$OUT_DIR/test_recipe_${slug}.jpg"

    if [[ ! -f "$jpg" ]]; then
        return 1
    fi

    local exp_film_sim exp_grain exp_grain_size exp_highlight exp_shadow
    local exp_color exp_sharpness exp_nr exp_clarity
    local exp_wb exp_wb_r exp_wb_b exp_chrome exp_chrome_blue

    exp_film_sim=$(read_recipe_field "$slug" film_sim)
    exp_grain=$(read_recipe_field "$slug" grain)
    exp_grain_size=$(read_recipe_field "$slug" grain_size)
    exp_highlight=$(read_recipe_field "$slug" highlight)
    exp_shadow=$(read_recipe_field "$slug" shadow)
    exp_color=$(read_recipe_field "$slug" color)
    exp_sharpness=$(read_recipe_field "$slug" sharpness)
    exp_nr=$(read_recipe_field "$slug" noise_reduction)
    exp_clarity=$(read_recipe_field "$slug" clarity)
    exp_wb=$(read_recipe_field "$slug" white_balance)
    exp_wb_r=$(read_recipe_field "$slug" wb_shift_r)
    exp_wb_b=$(read_recipe_field "$slug" wb_shift_b)
    exp_chrome=$(read_recipe_field "$slug" chrome_effect)
    exp_chrome_blue=$(read_recipe_field "$slug" chrome_blue)

    local exif_film exif_grain exif_grain_size exif_highlight exif_shadow
    local exif_color_raw exif_sharp_raw exif_nr_raw exif_clarity
    local exif_wb exif_wb_fine exif_chrome exif_chrome_blue

    exif_film=$(exiftool -s3 -FujiFilm:FilmMode "$jpg" 2>/dev/null || echo "EXIF_ERROR")
    exif_grain=$(exiftool -s3 -FujiFilm:GrainEffectRoughness "$jpg" 2>/dev/null || echo "EXIF_ERROR")
    exif_grain_size=$(exiftool -s3 -FujiFilm:GrainEffectSize "$jpg" 2>/dev/null || echo "EXIF_ERROR")
    exif_highlight=$(exiftool -s3 -FujiFilm:HighlightTone "$jpg" 2>/dev/null || echo "EXIF_ERROR")
    exif_shadow=$(exiftool -s3 -FujiFilm:ShadowTone "$jpg" 2>/dev/null || echo "EXIF_ERROR")
    exif_color_raw=$(exiftool -s3 -FujiFilm:Saturation "$jpg" 2>/dev/null || echo "EXIF_ERROR")
    exif_sharp_raw=$(exiftool -s3 -FujiFilm:Sharpness "$jpg" 2>/dev/null || echo "EXIF_ERROR")
    exif_nr_raw=$(exiftool -s3 -FujiFilm:NoiseReduction "$jpg" 2>/dev/null || echo "EXIF_ERROR")
    exif_clarity=$(exiftool -s3 -FujiFilm:Clarity "$jpg" 2>/dev/null || echo "EXIF_ERROR")
    exif_wb=$(exiftool -s3 -FujiFilm:WhiteBalance "$jpg" 2>/dev/null || echo "EXIF_ERROR")
    exif_wb_fine=$(exiftool -s3 -FujiFilm:WhiteBalanceFineTune "$jpg" 2>/dev/null || echo "EXIF_ERROR")
    exif_chrome=$(exiftool -s3 -FujiFilm:ColorChromeEffect "$jpg" 2>/dev/null || echo "EXIF_ERROR")
    exif_chrome_blue=$(exiftool -s3 -FujiFilm:ColorChromeFXBlue "$jpg" 2>/dev/null || echo "EXIF_ERROR")

    local exif_color exif_sharp exif_nr exif_wb_r exif_wb_b
    exif_color=$(echo "$exif_color_raw" | sed -E 's/^([+-]?[0-9]+).*/\1/')
    exif_sharp=$(echo "$exif_sharp_raw" | sed -E 's/^([+-]?[0-9]+).*/\1/')
    exif_nr=$(echo "$exif_nr_raw" | sed -E 's/^([+-]?[0-9]+).*/\1/')
    # Exiftool displays WB shifts with a ×20 factor; divide to get recipe units
    local wb_r_raw wb_b_raw
    wb_r_raw=$(echo "$exif_wb_fine" | sed -E 's/Red ([+-]?[0-9]+).*/\1/')
    wb_b_raw=$(echo "$exif_wb_fine" | sed -E 's/.*Blue ([+-]?[0-9]+).*/\1/')
    exif_wb_r=$(python3 -c "print(int($wb_r_raw) // 20)" 2>/dev/null || echo "$wb_r_raw")
    exif_wb_b=$(python3 -c "print(int($wb_b_raw) // 20)" 2>/dev/null || echo "$wb_b_raw")

    # ── FilmMode ──
    if [[ "$exp_film_sim" != "UNSET" ]]; then
        local exp_display
        exp_display=$(film_sim_display "$exp_film_sim")
        if matches_ci "$exif_film" "$exp_display"; then
            record "$slug" "FilmMode" "$exp_display" "$exif_film" "PASS" ""
        else
            record "$slug" "FilmMode" "$exp_display" "$exif_film" "FAIL" ""
        fi
    fi

    # ── GrainRoughness ──
    if [[ "$exp_grain" != "UNSET" ]]; then
        if matches_ci "$exif_grain" "$exp_grain"; then
            record "$slug" "GrainRoughness" "$exp_grain" "$exif_grain" "PASS" ""
        else
            record "$slug" "GrainRoughness" "$exp_grain" "$exif_grain" "FAIL" ""
        fi
    fi

    # ── GrainSize ──
    if [[ "$exp_grain_size" != "UNSET" ]]; then
        if matches_ci "$exif_grain_size" "$exp_grain_size"; then
            record "$slug" "GrainSize" "$exp_grain_size" "$exif_grain_size" "PASS" ""
        else
            record "$slug" "GrainSize" "$exp_grain_size" "$exif_grain_size" "FAIL" "not in D185 binary"
        fi
    fi

    # ── HighlightTone ──
    if [[ "$exp_highlight" != "UNSET" ]]; then
        local hmatch
        hmatch=$(float_match "$exif_highlight" "$exp_highlight")
        record "$slug" "HighlightTone" "$exp_highlight" "$exif_highlight" "$hmatch" ""
    fi

    # ── ShadowTone ──
    if [[ "$exp_shadow" != "UNSET" ]]; then
        local smatch
        smatch=$(float_match "$exif_shadow" "$exp_shadow")
        record "$slug" "ShadowTone" "$exp_shadow" "$exif_shadow" "$smatch" ""
    fi

    # ── Color ──
    if [[ "$exp_color" != "UNSET" ]]; then
        local exp_c_fmt
        exp_c_fmt=$(printf '%+d' "$(int_of "$exp_color")" | sed 's/^+0$/0/')
        if [[ "$exif_color" == "$exp_c_fmt" ]]; then
            record "$slug" "Color" "$exp_c_fmt" "$exif_color" "PASS" ""
        else
            record "$slug" "Color" "$exp_c_fmt" "$exif_color" "FAIL" ""
        fi
    fi

    # ── Sharpness ──
    if [[ "$exp_sharpness" != "UNSET" ]]; then
        local exp_s_fmt
        exp_s_fmt=$(printf '%+d' "$(int_of "$exp_sharpness")" | sed 's/^+0$/0/')
        if [[ "$exif_sharp" == "$exp_s_fmt" ]]; then
            record "$slug" "Sharpness" "$exp_s_fmt" "$exif_sharp" "PASS" ""
        else
            record "$slug" "Sharpness" "$exp_s_fmt" "$exif_sharp" "FAIL" ""
        fi
    fi

    # ── NoiseReduction ──
    if [[ "$exp_nr" != "UNSET" ]]; then
        local exp_nr_fmt
        exp_nr_fmt=$(printf '%+d' "$(int_of "$exp_nr")" | sed 's/^+0$/0/')
        if [[ "$exif_nr" == "$exp_nr_fmt" ]]; then
            record "$slug" "NoiseReduction" "$exp_nr_fmt" "$exif_nr" "PASS" ""
        else
            record "$slug" "NoiseReduction" "$exp_nr_fmt" "$exif_nr" "FAIL" ""
        fi
    fi

    # ── Clarity ──
    if [[ "$exp_clarity" != "UNSET" ]]; then
        local exp_cl_int
        exp_cl_int=$(int_of "$exp_clarity")
        if [[ "$exif_clarity" == "$exp_cl_int" ]]; then
            record "$slug" "Clarity" "$exp_cl_int" "$exif_clarity" "PASS" ""
        else
            record "$slug" "Clarity" "$exp_cl_int" "$exif_clarity" "FAIL" "camera may ignore"
        fi
    fi

    # ── WhiteBalance ──
    if [[ "$exp_wb" != "UNSET" ]]; then
        local exp_wb_disp
        exp_wb_disp=$(wb_display "$exp_wb")
        if matches_ci "$exif_wb" "$exp_wb_disp"; then
            record "$slug" "WhiteBalance" "$exp_wb_disp" "$exif_wb" "PASS" ""
        else
            record "$slug" "WhiteBalance" "$exp_wb_disp" "$exif_wb" "FAIL" "camera may ignore"
        fi
    fi

    # ── WB-Shift-R ──
    if [[ "$exp_wb_r" != "UNSET" ]]; then
        local exp_r_int
        exp_r_int=$(int_of "$exp_wb_r")
        if [[ "$exif_wb_r" == "$exp_r_int" ]]; then
            record "$slug" "WB-Shift-R" "$exp_r_int" "$exif_wb_r" "PASS" ""
        else
            record "$slug" "WB-Shift-R" "$exp_r_int" "$exif_wb_r" "FAIL" "camera may ignore"
        fi
    fi

    # ── WB-Shift-B ──
    if [[ "$exp_wb_b" != "UNSET" ]]; then
        local exp_b_int
        exp_b_int=$(int_of "$exp_wb_b")
        if [[ "$exif_wb_b" == "$exp_b_int" ]]; then
            record "$slug" "WB-Shift-B" "$exp_b_int" "$exif_wb_b" "PASS" ""
        else
            record "$slug" "WB-Shift-B" "$exp_b_int" "$exif_wb_b" "FAIL" "camera may ignore"
        fi
    fi

    # ── ChromeEffect ──
    if [[ "$exp_chrome" != "UNSET" ]]; then
        if matches_ci "$exif_chrome" "$exp_chrome"; then
            record "$slug" "ChromeEffect" "$exp_chrome" "$exif_chrome" "PASS" ""
        else
            record "$slug" "ChromeEffect" "$exp_chrome" "$exif_chrome" "FAIL" "not in D185 binary"
        fi
    fi

    # ── ChromeFXBlue ──
    if [[ "$exp_chrome_blue" != "UNSET" ]]; then
        if matches_ci "$exif_chrome_blue" "$exp_chrome_blue"; then
            record "$slug" "ChromeFXBlue" "$exp_chrome_blue" "$exif_chrome_blue" "PASS" ""
        else
            record "$slug" "ChromeFXBlue" "$exp_chrome_blue" "$exif_chrome_blue" "FAIL" "not in D185 binary"
        fi
    fi

    return 0
}

# ───────────────────────────────────────────────────────────────
# Main loop
# ───────────────────────────────────────────────────────────────
RECIPES_TESTED=0
RECIPES_MISSING=0

for slug in "${SLUGS[@]}"; do
    output_jpg="$OUT_DIR/test_recipe_${slug}.jpg"

    if [[ "$CONVERT" == true ]]; then
        if [[ ! -f "$RAF" ]]; then
            echo "ERROR: $RAF not found (needed for --convert)"
            exit 1
        fi
        printf "  [convert] %-45s" "$slug"
        if sudo "$BINARY" convert "$RAF" --recipe "$slug" --output "$output_jpg" >/dev/null 2>&1; then
            echo " ok"
        else
            echo " FAILED"
            continue
        fi
    fi

    if [[ ! -f "$output_jpg" ]]; then
        RECIPES_MISSING=$((RECIPES_MISSING + 1))
        continue
    fi

    RECIPES_TESTED=$((RECIPES_TESTED + 1))
    verify_recipe "$slug"
done

echo ""

# ───────────────────────────────────────────────────────────────
# Generate report from results file using Python
# ───────────────────────────────────────────────────────────────
python3 - "$RESULTS_FILE" "$RECIPES_TESTED" "$RECIPES_MISSING" <<'PYEOF'
import sys
from collections import defaultdict

results_file = sys.argv[1]
recipes_tested = int(sys.argv[2])
recipes_missing = int(sys.argv[3])

rows = []
with open(results_file) as f:
    for line in f:
        line = line.rstrip('\n')
        if not line:
            continue
        parts = line.split('\t')
        if len(parts) < 6:
            continue
        slug, field, expected, actual, status, note = parts
        rows.append((slug, field, expected, actual, status, note))

total_pass = sum(1 for r in rows if r[4] == 'PASS')
total_fail = sum(1 for r in rows if r[4] == 'FAIL')
failures = [r for r in rows if r[4] == 'FAIL']

# Per-recipe summary (one line each)
recipe_stats = defaultdict(lambda: [0, 0])
for slug, field, expected, actual, status, note in rows:
    if status == 'PASS':
        recipe_stats[slug][0] += 1
    elif status == 'FAIL':
        recipe_stats[slug][1] += 1

# Preserve insertion order
seen = []
for slug, *_ in rows:
    if slug not in seen:
        seen.append(slug)

print("  Per-recipe results:")
print("  " + "─" * 65)
print(f"  {'RECIPE':<45}  {'PASS':>4}  {'FAIL':>4}  STATUS")
print(f"  {'─'*45}  {'─'*4}  {'─'*4}  {'─'*10}")
for slug in seen:
    p, f = recipe_stats[slug]
    tag = "ALL PASS" if f == 0 else f"{f} FAIL"
    print(f"  {slug:<45}  {p:4d}  {f:4d}  {tag}")
print()

# Summary
print("═" * 71)
print(" Summary")
print("═" * 71)
print()
print(f"  Recipes tested:  {recipes_tested}")
if recipes_missing > 0:
    print(f"  Recipes missing: {recipes_missing}  (no JPEG found; run with --convert)")
print(f"  Fields passed:   {total_pass}")
print(f"  Fields failed:   {total_fail}")
print()

# Per-field failure rates
if failures:
    field_pass = defaultdict(int)
    field_fail = defaultdict(int)
    field_notes = defaultdict(set)
    for slug, field, expected, actual, status, note in rows:
        if status == 'PASS':
            field_pass[field] += 1
        elif status == 'FAIL':
            field_fail[field] += 1
            if note:
                field_notes[field].add(note)

    # Sort by failure count descending
    sorted_fields = sorted(field_fail.keys(), key=lambda f: field_fail[f], reverse=True)

    print("  Per-field failure rates:")
    print("  " + "─" * 65)
    print(f"  {'FIELD':<22}  {'PASS':>5}  {'FAIL':>5}  {'RATE':>6}  LIKELY REASON")
    print(f"  {'─'*22}  {'─'*5}  {'─'*5}  {'─'*6}  {'─'*25}")
    for field in sorted_fields:
        fc = field_fail[field]
        pc = field_pass[field]
        total = fc + pc
        rate = f"{fc/total*100:.0f}%" if total > 0 else "n/a"
        notes = field_notes[field]
        reason = ""
        if "not in D185 binary" in notes:
            reason = "not in D185 binary profile"
        elif "camera may ignore" in notes:
            reason = "camera may ignore (X100VI)"
        print(f"  {field:<22}  {pc:5d}  {fc:5d}  {rate:>6}  {reason}")
    print()

    # Detailed discrepancies
    print("  All discrepancies:")
    print("  " + "─" * 95)
    print(f"  {'RECIPE':<40}  {'FIELD':<18}  {'EXPECTED':<12}  {'ACTUAL':<12}  NOTE")
    print(f"  {'─'*40}  {'─'*18}  {'─'*12}  {'─'*12}  {'─'*20}")
    for slug, field, expected, actual, status, note in failures:
        print(f"  {slug:<40}  {field:<18}  {expected:<12}  {actual:<12}  {note}")
    print()

    print("  Note: WBShootCond (D185 index 10) must be set to 0 (OFF) for the camera")
    print("    to honour WhiteBalance mode (index 11). All recipe parameters are now")
    print("    fully supported in USB RAW CONV mode.")
    print()

print("═" * 71)
if total_fail == 0 and recipes_tested > 0:
    print(" ALL CHECKS PASSED")
else:
    print(f" Done. {total_fail} discrepancies found across {recipes_tested} recipe(s).")
print("═" * 71)

sys.exit(min(total_fail, 125))
PYEOF
