use slint::Color;

use crate::{MaterialScheme, MaterialSchemes};

pub fn parse_hex_color(hex: &str, fallback: Color) -> Color {
    let s = hex.trim().trim_start_matches('#');
    if s.len() == 6
        && let (Ok(r), Ok(g), Ok(b)) = (
            u8::from_str_radix(&s[0..2], 16),
            u8::from_str_radix(&s[2..4], 16),
            u8::from_str_radix(&s[4..6], 16),
        )
    {
        return Color::from_rgb_u8(r, g, b);
    }
    if s.len() == 8
        && let (Ok(r), Ok(g), Ok(b), Ok(a)) = (
            u8::from_str_radix(&s[0..2], 16),
            u8::from_str_radix(&s[2..4], 16),
            u8::from_str_radix(&s[4..6], 16),
            u8::from_str_radix(&s[6..8], 16),
        )
    {
        return Color::from_argb_u8(a, r, g, b);
    }
    fallback
}

fn mix(a: (u8, u8, u8), b: (u8, u8, u8), t: f32) -> Color {
    let clamp = t.clamp(0.0, 1.0);
    let r = (a.0 as f32 * (1.0 - clamp) + b.0 as f32 * clamp).round() as u8;
    let g = (a.1 as f32 * (1.0 - clamp) + b.1 as f32 * clamp).round() as u8;
    let bl = (a.2 as f32 * (1.0 - clamp) + b.2 as f32 * clamp).round() as u8;
    Color::from_rgb_u8(r, g, bl)
}

fn rgb(c: Color) -> (u8, u8, u8) {
    (c.red(), c.green(), c.blue())
}

/// Generates cohesive Material Design 3 Expressive light and dark tonal schemes from any user accent hex color.
/// Uses the restrained Mono Lavender surface hierarchy when `#4A4459` is active, and derives balanced tonal
/// palettes for custom user accent colors.
pub fn build_material_schemes(accent_hex: &str) -> MaterialSchemes {
    if accent_hex.trim().eq_ignore_ascii_case("#4A4459")
        || accent_hex.trim().eq_ignore_ascii_case("4A4459")
    {
        return mono_lavender_schemes();
    }

    let seed = rgb(parse_hex_color(accent_hex, Color::from_rgb_u8(74, 68, 89)));
    let white = (255, 255, 255);
    let black = (0, 0, 0);
    let neutral_light = (252, 248, 253);
    let neutral_dark = (20, 19, 23);

    let light_primary = mix(seed, black, 0.08);
    let light_container = mix(seed, white, 0.82);
    let light_on_container = mix(seed, black, 0.65);

    let dark_primary = mix(seed, white, 0.56);
    let dark_on_primary = mix(seed, black, 0.68);
    let dark_container = mix(seed, black, 0.35);
    let dark_on_container = mix(seed, white, 0.82);

    let secondary_seed = (
        ((seed.0 as u16 + 100) / 2) as u8,
        ((seed.1 as u16 + 105) / 2) as u8,
        ((seed.2 as u16 + 120) / 2) as u8,
    );
    let tertiary_seed = (
        ((seed.2 as u16 + 90) / 2) as u8,
        ((seed.0 as u16 + 85) / 2) as u8,
        ((seed.1 as u16 + 115) / 2) as u8,
    );

    let light = MaterialScheme {
        primary: light_primary,
        surfaceTint: light_primary,
        onPrimary: Color::from_rgb_u8(255, 255, 255),
        primaryContainer: light_container,
        onPrimaryContainer: light_on_container,
        secondary: mix(secondary_seed, black, 0.15),
        onSecondary: Color::from_rgb_u8(255, 255, 255),
        secondaryContainer: mix(secondary_seed, white, 0.82),
        onSecondaryContainer: Color::from_rgb_u8(27, 27, 31),
        tertiary: mix(tertiary_seed, black, 0.15),
        onTertiary: Color::from_rgb_u8(255, 255, 255),
        tertiaryContainer: mix(tertiary_seed, white, 0.82),
        onTertiaryContainer: Color::from_rgb_u8(30, 26, 34),
        error: Color::from_rgb_u8(179, 38, 30),
        onError: Color::from_rgb_u8(255, 255, 255),
        errorContainer: Color::from_rgb_u8(249, 222, 220),
        onErrorContainer: Color::from_rgb_u8(65, 14, 11),
        background: mix(neutral_light, seed, 0.02),
        onBackground: Color::from_rgb_u8(28, 27, 31),
        surface: mix(neutral_light, seed, 0.02),
        onSurface: Color::from_rgb_u8(28, 27, 31),
        surfaceVariant: mix((233, 229, 234), seed, 0.04),
        onSurfaceVariant: Color::from_rgb_u8(72, 69, 78),
        outline: Color::from_rgb_u8(121, 116, 126),
        outlineVariant: Color::from_rgb_u8(202, 196, 208),
        shadow: Color::from_rgb_u8(0, 0, 0),
        scrim: Color::from_rgb_u8(0, 0, 0),
        inverseSurface: Color::from_rgb_u8(49, 48, 51),
        inverseOnSurface: Color::from_rgb_u8(244, 239, 244),
        inversePrimary: dark_primary,
        primaryFixed: light_container,
        onPrimaryFixed: light_on_container,
        primaryFixedDim: dark_primary,
        onPrimaryFixedVariant: dark_container,
        secondaryFixed: Color::from_rgb_u8(230, 225, 230),
        onSecondaryFixed: Color::from_rgb_u8(27, 27, 31),
        secondaryFixedDim: Color::from_rgb_u8(200, 197, 205),
        onSecondaryFixedVariant: Color::from_rgb_u8(72, 70, 77),
        tertiaryFixed: Color::from_rgb_u8(233, 224, 234),
        onTertiaryFixed: Color::from_rgb_u8(30, 26, 34),
        tertiaryFixedDim: Color::from_rgb_u8(210, 198, 212),
        onTertiaryFixedVariant: Color::from_rgb_u8(82, 67, 70),
        surfaceDim: Color::from_rgb_u8(220, 216, 222),
        surfaceBright: Color::from_rgb_u8(252, 248, 253),
        surfaceContainerLowest: Color::from_rgb_u8(255, 255, 255),
        surfaceContainerLow: mix((245, 241, 246), seed, 0.025),
        surfaceContainer: mix((239, 235, 240), seed, 0.035),
        surfaceContainerHigh: mix((233, 229, 234), seed, 0.045),
        surfaceContainerHighest: mix((228, 224, 229), seed, 0.055),
    };

    let dark = MaterialScheme {
        primary: dark_primary,
        surfaceTint: dark_primary,
        onPrimary: dark_on_primary,
        primaryContainer: dark_container,
        onPrimaryContainer: dark_on_container,
        secondary: mix(secondary_seed, white, 0.45),
        onSecondary: Color::from_rgb_u8(49, 48, 54),
        secondaryContainer: Color::from_rgb_u8(72, 70, 77),
        onSecondaryContainer: Color::from_rgb_u8(228, 225, 234),
        tertiary: mix(tertiary_seed, white, 0.45),
        onTertiary: Color::from_rgb_u8(64, 40, 67),
        tertiaryContainer: Color::from_rgb_u8(82, 67, 70),
        onTertiaryContainer: Color::from_rgb_u8(240, 222, 226),
        error: Color::from_rgb_u8(242, 184, 181),
        onError: Color::from_rgb_u8(96, 20, 16),
        errorContainer: Color::from_rgb_u8(140, 29, 24),
        onErrorContainer: Color::from_rgb_u8(249, 222, 220),
        background: mix(neutral_dark, seed, 0.04),
        onBackground: Color::from_rgb_u8(227, 226, 231),
        surface: mix(neutral_dark, seed, 0.04),
        onSurface: Color::from_rgb_u8(227, 226, 231),
        surfaceVariant: Color::from_rgb_u8(72, 69, 80),
        onSurfaceVariant: Color::from_rgb_u8(201, 197, 209),
        outline: Color::from_rgb_u8(147, 143, 155),
        outlineVariant: Color::from_rgb_u8(72, 69, 80),
        shadow: Color::from_rgb_u8(0, 0, 0),
        scrim: Color::from_rgb_u8(0, 0, 0),
        inverseSurface: Color::from_rgb_u8(227, 226, 231),
        inverseOnSurface: Color::from_rgb_u8(49, 48, 52),
        inversePrimary: light_primary,
        primaryFixed: light_container,
        onPrimaryFixed: light_on_container,
        primaryFixedDim: dark_primary,
        onPrimaryFixedVariant: dark_container,
        secondaryFixed: Color::from_rgb_u8(230, 225, 230),
        onSecondaryFixed: Color::from_rgb_u8(27, 27, 31),
        secondaryFixedDim: Color::from_rgb_u8(200, 197, 205),
        onSecondaryFixedVariant: Color::from_rgb_u8(72, 70, 77),
        tertiaryFixed: Color::from_rgb_u8(233, 224, 234),
        onTertiaryFixed: Color::from_rgb_u8(30, 26, 34),
        tertiaryFixedDim: Color::from_rgb_u8(210, 198, 212),
        onTertiaryFixedVariant: Color::from_rgb_u8(82, 67, 70),
        surfaceDim: Color::from_rgb_u8(16, 15, 19),
        surfaceBright: Color::from_rgb_u8(54, 53, 58),
        surfaceContainerLowest: Color::from_rgb_u8(14, 13, 17),
        surfaceContainerLow: mix((28, 27, 31), seed, 0.05),
        surfaceContainer: mix((32, 31, 35), seed, 0.06),
        surfaceContainerHigh: mix((43, 41, 45), seed, 0.07),
        surfaceContainerHighest: mix((53, 52, 56), seed, 0.08),
    };

    MaterialSchemes { light, dark }
}

fn mono_lavender_schemes() -> MaterialSchemes {
    let light = MaterialScheme {
        primary: Color::from_rgb_u8(0x4A, 0x44, 0x59),
        surfaceTint: Color::from_rgb_u8(0x4A, 0x44, 0x59),
        onPrimary: Color::from_rgb_u8(0xFF, 0xFF, 0xFF),
        primaryContainer: Color::from_rgb_u8(0xE6, 0xE0, 0xF0),
        onPrimaryContainer: Color::from_rgb_u8(0x1A, 0x16, 0x26),
        secondary: Color::from_rgb_u8(0x5F, 0x5D, 0x65),
        onSecondary: Color::from_rgb_u8(0xFF, 0xFF, 0xFF),
        secondaryContainer: Color::from_rgb_u8(0xE6, 0xE1, 0xE6),
        onSecondaryContainer: Color::from_rgb_u8(0x1B, 0x1B, 0x1F),
        tertiary: Color::from_rgb_u8(0x6D, 0x58, 0x62),
        onTertiary: Color::from_rgb_u8(0xFF, 0xFF, 0xFF),
        tertiaryContainer: Color::from_rgb_u8(0xE9, 0xE0, 0xEA),
        onTertiaryContainer: Color::from_rgb_u8(0x1E, 0x1A, 0x22),
        error: Color::from_rgb_u8(0xB3, 0x26, 0x1E),
        onError: Color::from_rgb_u8(0xFF, 0xFF, 0xFF),
        errorContainer: Color::from_rgb_u8(0xF9, 0xDE, 0xDC),
        onErrorContainer: Color::from_rgb_u8(0x41, 0x0E, 0x0B),
        background: Color::from_rgb_u8(0xFC, 0xF8, 0xFD),
        onBackground: Color::from_rgb_u8(0x1C, 0x1B, 0x1F),
        surface: Color::from_rgb_u8(0xFC, 0xF8, 0xFD),
        onSurface: Color::from_rgb_u8(0x1C, 0x1B, 0x1F),
        surfaceVariant: Color::from_rgb_u8(0xE9, 0xE5, 0xEA),
        onSurfaceVariant: Color::from_rgb_u8(0x48, 0x45, 0x4E),
        outline: Color::from_rgb_u8(0x79, 0x74, 0x7E),
        outlineVariant: Color::from_rgb_u8(0xCA, 0xC4, 0xD0),
        shadow: Color::from_rgb_u8(0x00, 0x00, 0x00),
        scrim: Color::from_rgb_u8(0x00, 0x00, 0x00),
        inverseSurface: Color::from_rgb_u8(0x31, 0x30, 0x33),
        inverseOnSurface: Color::from_rgb_u8(0xF4, 0xEF, 0xF4),
        inversePrimary: Color::from_rgb_u8(0xCF, 0xC3, 0xE0),
        primaryFixed: Color::from_rgb_u8(0xE6, 0xE0, 0xF0),
        onPrimaryFixed: Color::from_rgb_u8(0x1A, 0x16, 0x26),
        primaryFixedDim: Color::from_rgb_u8(0xCA, 0xC3, 0xDC),
        onPrimaryFixedVariant: Color::from_rgb_u8(0x4A, 0x44, 0x59),
        secondaryFixed: Color::from_rgb_u8(0xE6, 0xE1, 0xE6),
        onSecondaryFixed: Color::from_rgb_u8(0x1B, 0x1B, 0x1F),
        secondaryFixedDim: Color::from_rgb_u8(0xC8, 0xC5, 0xCD),
        onSecondaryFixedVariant: Color::from_rgb_u8(0x48, 0x46, 0x4D),
        tertiaryFixed: Color::from_rgb_u8(0xE9, 0xE0, 0xEA),
        onTertiaryFixed: Color::from_rgb_u8(0x1E, 0x1A, 0x22),
        tertiaryFixedDim: Color::from_rgb_u8(0xD5, 0xC4, 0xD8),
        onTertiaryFixedVariant: Color::from_rgb_u8(0x52, 0x43, 0x46),
        surfaceDim: Color::from_rgb_u8(0xDC, 0xD8, 0xDE),
        surfaceBright: Color::from_rgb_u8(0xFC, 0xF8, 0xFD),
        surfaceContainerLowest: Color::from_rgb_u8(0xFF, 0xFF, 0xFF),
        surfaceContainerLow: Color::from_rgb_u8(0xF5, 0xF1, 0xF6),
        surfaceContainer: Color::from_rgb_u8(0xEF, 0xEB, 0xF0),
        surfaceContainerHigh: Color::from_rgb_u8(0xE9, 0xE5, 0xEA),
        surfaceContainerHighest: Color::from_rgb_u8(0xE4, 0xE0, 0xE5),
    };

    let dark = MaterialScheme {
        primary: Color::from_rgb_u8(0xCA, 0xC3, 0xDC),
        surfaceTint: Color::from_rgb_u8(0xCA, 0xC3, 0xDC),
        onPrimary: Color::from_rgb_u8(0x33, 0x2D, 0x41),
        primaryContainer: Color::from_rgb_u8(0x4A, 0x44, 0x59),
        onPrimaryContainer: Color::from_rgb_u8(0xE7, 0xDF, 0xF8),
        secondary: Color::from_rgb_u8(0xC8, 0xC5, 0xCD),
        onSecondary: Color::from_rgb_u8(0x31, 0x30, 0x36),
        secondaryContainer: Color::from_rgb_u8(0x48, 0x46, 0x4D),
        onSecondaryContainer: Color::from_rgb_u8(0xE4, 0xE1, 0xEA),
        tertiary: Color::from_rgb_u8(0xDB, 0xC1, 0xC6),
        onTertiary: Color::from_rgb_u8(0x3C, 0x2A, 0x2E),
        tertiaryContainer: Color::from_rgb_u8(0x52, 0x43, 0x46),
        onTertiaryContainer: Color::from_rgb_u8(0xF0, 0xDE, 0xE2),
        error: Color::from_rgb_u8(0xF2, 0xB8, 0xB5),
        onError: Color::from_rgb_u8(0x60, 0x14, 0x10),
        errorContainer: Color::from_rgb_u8(0x8C, 0x1D, 0x18),
        onErrorContainer: Color::from_rgb_u8(0xF9, 0xDE, 0xDC),
        background: Color::from_rgb_u8(0x14, 0x13, 0x17),
        onBackground: Color::from_rgb_u8(0xE3, 0xE2, 0xE7),
        surface: Color::from_rgb_u8(0x14, 0x13, 0x17),
        onSurface: Color::from_rgb_u8(0xE3, 0xE2, 0xE7),
        surfaceVariant: Color::from_rgb_u8(0x48, 0x45, 0x50),
        onSurfaceVariant: Color::from_rgb_u8(0xC9, 0xC5, 0xD1),
        outline: Color::from_rgb_u8(0x93, 0x8F, 0x9B),
        outlineVariant: Color::from_rgb_u8(0x48, 0x45, 0x50),
        shadow: Color::from_rgb_u8(0x00, 0x00, 0x00),
        scrim: Color::from_rgb_u8(0x00, 0x00, 0x00),
        inverseSurface: Color::from_rgb_u8(0xE3, 0xE2, 0xE7),
        inverseOnSurface: Color::from_rgb_u8(0x31, 0x30, 0x34),
        inversePrimary: Color::from_rgb_u8(0x61, 0x5B, 0x71),
        primaryFixed: Color::from_rgb_u8(0xE6, 0xE0, 0xF0),
        onPrimaryFixed: Color::from_rgb_u8(0x1A, 0x16, 0x26),
        primaryFixedDim: Color::from_rgb_u8(0xCA, 0xC3, 0xDC),
        onPrimaryFixedVariant: Color::from_rgb_u8(0x4A, 0x44, 0x59),
        secondaryFixed: Color::from_rgb_u8(0xE6, 0xE1, 0xE6),
        onSecondaryFixed: Color::from_rgb_u8(0x1B, 0x1B, 0x1F),
        secondaryFixedDim: Color::from_rgb_u8(0xC8, 0xC5, 0xCD),
        onSecondaryFixedVariant: Color::from_rgb_u8(0x48, 0x46, 0x4D),
        tertiaryFixed: Color::from_rgb_u8(0xE9, 0xE0, 0xEA),
        onTertiaryFixed: Color::from_rgb_u8(0x1E, 0x1A, 0x22),
        tertiaryFixedDim: Color::from_rgb_u8(0xD5, 0xC4, 0xD8),
        onTertiaryFixedVariant: Color::from_rgb_u8(0x52, 0x43, 0x46),
        surfaceDim: Color::from_rgb_u8(0x14, 0x13, 0x17),
        surfaceBright: Color::from_rgb_u8(0x3A, 0x38, 0x3E),
        surfaceContainerLowest: Color::from_rgb_u8(0x0F, 0x0E, 0x12),
        surfaceContainerLow: Color::from_rgb_u8(0x1C, 0x1B, 0x1F),
        surfaceContainer: Color::from_rgb_u8(0x20, 0x1F, 0x23),
        surfaceContainerHigh: Color::from_rgb_u8(0x2B, 0x29, 0x2D),
        surfaceContainerHighest: Color::from_rgb_u8(0x35, 0x34, 0x38),
    };

    MaterialSchemes { light, dark }
}
