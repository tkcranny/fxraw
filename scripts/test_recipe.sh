#!/usr/bin/env bash
set -euo pipefail

BINARY="./target/release/fuji-usb-test"
RAF="DSCF7496.RAF"
RECIPE="${1:-classic-amber}"
OUTPUT="/tmp/test_recipe_${RECIPE}.jpg"

# ───────────────────────────────────────────────────────────────
# Parse expected values from recipes.json for the given recipe
# ───────────────────────────────────────────────────────────────
RECIPES_JSON="data/recipes.json"

read_recipe_field() {
    python3 -c "
import json, sys
data = json.load(open('$RECIPES_JSON'))
r = next((x for x in data if x['slug'] == '$RECIPE'), None)
if r is None:
    print('RECIPE_NOT_FOUND', file=sys.stderr)
    sys.exit(1)
field = '$1'
val = r.get(field)
if val is None:
    print('UNSET')
else:
    print(val)
"
}

EXPECT_FILM_SIM=$(read_recipe_field film_sim)
EXPECT_GRAIN=$(read_recipe_field grain)
EXPECT_GRAIN_SIZE=$(read_recipe_field grain_size)
EXPECT_HIGHLIGHT=$(read_recipe_field highlight)
EXPECT_SHADOW=$(read_recipe_field shadow)
EXPECT_COLOR=$(read_recipe_field color)
EXPECT_SHARPNESS=$(read_recipe_field sharpness)
EXPECT_NR=$(read_recipe_field noise_reduction)
EXPECT_CLARITY=$(read_recipe_field clarity)
EXPECT_WB=$(read_recipe_field white_balance)
EXPECT_WB_SHIFT_R=$(read_recipe_field wb_shift_r)
EXPECT_WB_SHIFT_B=$(read_recipe_field wb_shift_b)
EXPECT_CHROME_EFFECT=$(read_recipe_field chrome_effect)
EXPECT_CHROME_BLUE=$(read_recipe_field chrome_blue)

echo "═══════════════════════════════════════════════════════════"
echo " Recipe: $RECIPE"
echo "═══════════════════════════════════════════════════════════"
echo ""

# ───────────────────────────────────────────────────────────────
# Step 1: Convert (if --convert flag is passed)
# ───────────────────────────────────────────────────────────────
if [[ "${2:-}" == "--convert" ]]; then
    echo "[convert] $RAF → $OUTPUT (recipe: $RECIPE)"
    echo ""
    if [[ ! -f "$RAF" ]]; then
        echo "ERROR: $RAF not found"
        exit 1
    fi
    sudo "$BINARY" convert "$RAF" --recipe "$RECIPE" --output "$OUTPUT"
    echo ""
fi

if [[ ! -f "$OUTPUT" ]]; then
    echo "ERROR: $OUTPUT not found. Run with --convert flag first:"
    echo "  ./test_recipe.sh $RECIPE --convert"
    exit 1
fi

# ───────────────────────────────────────────────────────────────
# Step 2: Extract EXIF values
# ───────────────────────────────────────────────────────────────
echo "[exiftool] Reading $OUTPUT"
echo ""

EXIF_FILM_SIM=$(exiftool -s3 -FujiFilm:FilmMode "$OUTPUT")
EXIF_GRAIN=$(exiftool -s3 -FujiFilm:GrainEffectRoughness "$OUTPUT")
EXIF_GRAIN_SIZE=$(exiftool -s3 -FujiFilm:GrainEffectSize "$OUTPUT")
EXIF_HIGHLIGHT=$(exiftool -s3 -FujiFilm:HighlightTone "$OUTPUT")
EXIF_SHADOW=$(exiftool -s3 -FujiFilm:ShadowTone "$OUTPUT")
EXIF_COLOR_RAW=$(exiftool -s3 -FujiFilm:Saturation "$OUTPUT")
EXIF_SHARPNESS_RAW=$(exiftool -s3 -FujiFilm:Sharpness "$OUTPUT")
EXIF_NR_RAW=$(exiftool -s3 -FujiFilm:NoiseReduction "$OUTPUT")
EXIF_CLARITY=$(exiftool -s3 -FujiFilm:Clarity "$OUTPUT")
EXIF_WB=$(exiftool -s3 -FujiFilm:WhiteBalance "$OUTPUT")
EXIF_WB_FINE=$(exiftool -s3 -FujiFilm:WhiteBalanceFineTune "$OUTPUT")
EXIF_CHROME_EFFECT=$(exiftool -s3 -FujiFilm:ColorChromeEffect "$OUTPUT")
EXIF_CHROME_BLUE=$(exiftool -s3 -FujiFilm:ColorChromeFXBlue "$OUTPUT")

# Parse compound values
# Saturation "+4 (highest)" → extract first number
EXIF_COLOR=$(echo "$EXIF_COLOR_RAW" | sed -E 's/^([+-]?[0-9]+).*/\1/')
# Sharpness "-2 (soft)" → extract first number
EXIF_SHARPNESS=$(echo "$EXIF_SHARPNESS_RAW" | sed -E 's/^([+-]?[0-9]+).*/\1/')
# NoiseReduction "-4 (weakest)" → extract first number
EXIF_NR=$(echo "$EXIF_NR_RAW" | sed -E 's/^([+-]?[0-9]+).*/\1/')
# WhiteBalanceFineTune "Red +20, Blue -120" → extract, divide by 20 for recipe units
WB_R_RAW=$(echo "$EXIF_WB_FINE" | sed -E 's/Red ([+-]?[0-9]+).*/\1/')
WB_B_RAW=$(echo "$EXIF_WB_FINE" | sed -E 's/.*Blue ([+-]?[0-9]+).*/\1/')
EXIF_WB_SHIFT_R=$(python3 -c "print(int($WB_R_RAW) // 20)")
EXIF_WB_SHIFT_B=$(python3 -c "print(int($WB_B_RAW) // 20)")

# ───────────────────────────────────────────────────────────────
# Step 3: Comparison
# ───────────────────────────────────────────────────────────────
PASS=0
FAIL=0
SKIP=0

# Film simulation name mapping (recipe slug → exiftool display)
film_sim_display() {
    case "$1" in
        provia)       echo "Provia" ;;
        velvia)       echo "Velvia" ;;
        astia)        echo "Astia" ;;
        classic-chrome) echo "Classic Chrome" ;;
        classic-neg)  echo "Classic Negative" ;;
        pro-neg-hi)   echo "Pro Neg. Hi" ;;
        pro-neg-std)  echo "Pro Neg. Std" ;;
        eterna)       echo "Eterna" ;;
        eterna-bleach-bypass) echo "Eterna Bleach Bypass" ;;
        acros)        echo "Acros" ;;
        acros-ye)     echo "Acros+Ye Filter" ;;
        acros-r)      echo "Acros+R Filter" ;;
        acros-g)      echo "Acros+G Filter" ;;
        monochrome)   echo "B&W" ;;
        monochrome-ye) echo "B&W+Ye Filter" ;;
        monochrome-r) echo "B&W+R Filter" ;;
        monochrome-g) echo "B&W+G Filter" ;;
        sepia)        echo "Sepia" ;;
        nostalgic-neg) echo "Nostalgic Neg" ;;
        reala-ace)    echo "Reala Ace" ;;
        *)            echo "$1" ;;
    esac
}

# Case-insensitive partial match (exiftool names vary across firmware)
matches_ci() {
    local actual
    local expected
    actual=$(echo "$1" | tr '[:upper:]' '[:lower:]')
    expected=$(echo "$2" | tr '[:upper:]' '[:lower:]')
    [[ "$actual" == *"$expected"* ]]
}

check() {
    local field="$1"
    local expected="$2"
    local actual="$3"
    local note="${4:-}"

    if [[ "$expected" == "UNSET" ]]; then
        printf "  %-22s %-20s  SKIP (not in recipe)\n" "$field" "$actual"
        SKIP=$((SKIP + 1))
        return
    fi

    if [[ "$actual" == "$expected" ]]; then
        printf "  %-22s %-20s  ✓ PASS\n" "$field" "$actual"
        PASS=$((PASS + 1))
    else
        printf "  %-22s %-20s  ✗ FAIL (expected: %s)%s\n" "$field" "$actual" "$expected" "${note:+ [$note]}"
        FAIL=$((FAIL + 1))
    fi
}

check_approx() {
    local field="$1"
    local expected="$2"
    local actual="$3"
    local note="${4:-}"

    if [[ "$expected" == "UNSET" ]]; then
        printf "  %-22s %-20s  SKIP (not in recipe)\n" "$field" "$actual"
        SKIP=$((SKIP + 1))
        return
    fi

    # Compare as floats: allow ±0.01 tolerance
    local match
    match=$(python3 -c "print('yes' if abs(float('$actual') - float('$expected')) < 0.01 else 'no')")
    if [[ "$match" == "yes" ]]; then
        printf "  %-22s %-20s  ✓ PASS\n" "$field" "$actual"
        PASS=$((PASS + 1))
    else
        printf "  %-22s %-20s  ✗ FAIL (expected: %s)%s\n" "$field" "$actual" "$expected" "${note:+ [$note]}"
        FAIL=$((FAIL + 1))
    fi
}

check_film_sim() {
    local expected_slug="$1"
    local actual="$2"
    local expected_display
    expected_display=$(film_sim_display "$expected_slug")

    if matches_ci "$actual" "$expected_display"; then
        printf "  %-22s %-20s  ✓ PASS\n" "FilmMode" "$actual"
        PASS=$((PASS + 1))
    else
        printf "  %-22s %-20s  ✗ FAIL (expected: %s)\n" "FilmMode" "$actual" "$expected_display"
        FAIL=$((FAIL + 1))
    fi
}

check_enum() {
    local field="$1"
    local expected="$2"
    local actual="$3"
    local note="${4:-}"

    if [[ "$expected" == "UNSET" ]]; then
        printf "  %-22s %-20s  SKIP (not in recipe)\n" "$field" "$actual"
        SKIP=$((SKIP + 1))
        return
    fi

    if matches_ci "$actual" "$expected"; then
        printf "  %-22s %-20s  ✓ PASS\n" "$field" "$actual"
        PASS=$((PASS + 1))
    else
        printf "  %-22s %-20s  ✗ FAIL (expected: %s)%s\n" "$field" "$actual" "$expected" "${note:+ [$note]}"
        FAIL=$((FAIL + 1))
    fi
}

echo "  FIELD                  ACTUAL                RESULT"
echo "  ─────────────────────  ────────────────────  ──────────────────────"

check_film_sim "$EXPECT_FILM_SIM" "$EXIF_FILM_SIM"
check_enum "GrainRoughness" "$EXPECT_GRAIN" "$EXIF_GRAIN"
check_enum "GrainSize" "$EXPECT_GRAIN_SIZE" "$EXIF_GRAIN_SIZE"
check_approx "HighlightTone" "$EXPECT_HIGHLIGHT" "$EXIF_HIGHLIGHT"
check_approx "ShadowTone" "$EXPECT_SHADOW" "$EXIF_SHADOW"

if [[ "$EXPECT_COLOR" != "UNSET" ]]; then
    check "Color" "$(printf '%+d' "$EXPECT_COLOR" | sed 's/^+0$/0/')" "$EXIF_COLOR"
else
    check "Color" "UNSET" "$EXIF_COLOR"
fi

if [[ "$EXPECT_SHARPNESS" != "UNSET" ]]; then
    check "Sharpness" "$(printf '%+d' "$EXPECT_SHARPNESS" | sed 's/^+0$/0/')" "$EXIF_SHARPNESS"
else
    check "Sharpness" "UNSET" "$EXIF_SHARPNESS"
fi

if [[ "$EXPECT_NR" != "UNSET" ]]; then
    check "NoiseReduction" "$(printf '%+d' "$EXPECT_NR" | sed 's/^+0$/0/')" "$EXIF_NR"
else
    check "NoiseReduction" "UNSET" "$EXIF_NR"
fi

check "Clarity" "${EXPECT_CLARITY}" "$EXIF_CLARITY"

# WB mode
if [[ "$EXPECT_WB" != "UNSET" ]]; then
    WB_DISPLAY="$EXPECT_WB"
    case "$EXPECT_WB" in
        auto)          WB_DISPLAY="Auto" ;;
        daylight)      WB_DISPLAY="Daylight" ;;
        shade)         WB_DISPLAY="Shade" ;;
        incandescent)  WB_DISPLAY="Incandescent" ;;
        fluorescent-1) WB_DISPLAY="Daylight Fluorescent" ;;
        fluorescent-2) WB_DISPLAY="Day White Fluorescent" ;;
        fluorescent-3) WB_DISPLAY="White Fluorescent" ;;
        temperature)   WB_DISPLAY="Kelvin" ;;
    esac
    check_enum "WhiteBalance" "$WB_DISPLAY" "$EXIF_WB"
else
    check_enum "WhiteBalance" "UNSET" "$EXIF_WB"
fi

if [[ "$EXPECT_WB_SHIFT_R" != "UNSET" ]]; then
    # Truncate decimal: "1.0" → "1"
    R_INT=$(python3 -c "print(int(float('$EXPECT_WB_SHIFT_R')))")
    check "WB-Shift-R" "$R_INT" "$EXIF_WB_SHIFT_R"
else
    check "WB-Shift-R" "UNSET" "$EXIF_WB_SHIFT_R"
fi
if [[ "$EXPECT_WB_SHIFT_B" != "UNSET" ]]; then
    B_INT=$(python3 -c "print(int(float('$EXPECT_WB_SHIFT_B')))")
    check "WB-Shift-B" "$B_INT" "$EXIF_WB_SHIFT_B"
else
    check "WB-Shift-B" "UNSET" "$EXIF_WB_SHIFT_B"
fi

check_enum "ChromeEffect" "$EXPECT_CHROME_EFFECT" "$EXIF_CHROME_EFFECT"
check_enum "ChromeFXBlue" "$EXPECT_CHROME_BLUE" "$EXIF_CHROME_BLUE"

echo ""
echo "═══════════════════════════════════════════════════════════"
printf " Results: %d passed, %d failed, %d skipped\n" "$PASS" "$FAIL" "$SKIP"
echo "═══════════════════════════════════════════════════════════"

if [[ "$FAIL" -gt 0 ]]; then
    echo ""
    echo "Re-convert to verify: sudo $BINARY convert $RAF --recipe $RECIPE --output $OUTPUT"
fi

exit $FAIL
