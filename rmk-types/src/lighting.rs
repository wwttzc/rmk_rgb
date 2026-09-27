//! Catalogue of per-key RGB effects.
//!
//! The names and ids are Vial's RGB effect list
//! (`vial-qmk/quantum/vialrgb_effects.inc`), which is what the Vial Lighting
//! panel offers and what the VIA lighting commands carry. Effects QMK has but
//! Vial does not are numbered after the last Vial id instead of being squeezed
//! into a gap, so a Vial id never means two different things.
//!
//! This table is the single source of truth: `rgb.toml` is validated against
//! it, code generation emits the enabled ids from it, and the runtime resolves
//! those ids back to a rendering function.

/// Bit in [`EffectInfo::flags`]: the effect reads each LED's `x`/`y`.
pub const NEEDS_LAYOUT: u8 = 1 << 0;
/// Bit in [`EffectInfo::flags`]: the effect reads each LED's matrix position.
pub const NEEDS_MATRIX: u8 = 1 << 1;
/// Bit in [`EffectInfo::flags`]: the effect reacts to key events.
pub const REACTIVE: u8 = 1 << 2;
/// Bit in [`EffectInfo::flags`]: the effect renders from the matrix framebuffer.
pub const FRAMEBUFFER: u8 = 1 << 3;
/// Bit in [`EffectInfo::flags`]: the firmware can render this effect.
///
/// Enabling an effect without it fails the build, rather than leaving a name in
/// the Vial panel that never lights anything up.
pub const IMPLEMENTED: u8 = 1 << 4;

/// One effect of the catalogue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EffectInfo {
    /// Name used in `rgb.toml` and by `[rgb_matrix.default].animation`.
    pub name: &'static str,
    /// Vial effect id, and the value stored as the current mode.
    pub id: u16,
    /// Bitmask of the `NEEDS_*`/`REACTIVE`/`FRAMEBUFFER`/`IMPLEMENTED` flags.
    pub flags: u8,
}

impl EffectInfo {
    pub const fn needs_layout(&self) -> bool {
        self.flags & NEEDS_LAYOUT != 0
    }

    pub const fn needs_matrix(&self) -> bool {
        self.flags & NEEDS_MATRIX != 0
    }

    pub const fn is_reactive(&self) -> bool {
        self.flags & REACTIVE != 0
    }

    pub const fn implemented(&self) -> bool {
        self.flags & IMPLEMENTED != 0
    }
}

const fn effect(name: &'static str, id: u16, flags: u8) -> EffectInfo {
    EffectInfo { name, id, flags }
}

/// Every effect RMK knows about, in Vial id order.
pub static EFFECTS: &[EffectInfo] = &[
    effect("off", 0, IMPLEMENTED),
    effect("direct", 1, IMPLEMENTED | NEEDS_LAYOUT),
    effect("solid_color", 2, IMPLEMENTED),
    effect("alpha_mods", 3, IMPLEMENTED | NEEDS_LAYOUT),
    effect("gradient_up_down", 4, IMPLEMENTED | NEEDS_LAYOUT),
    effect("gradient_left_right", 5, IMPLEMENTED | NEEDS_LAYOUT),
    effect("breathing", 6, IMPLEMENTED),
    effect("band_sat", 7, IMPLEMENTED | NEEDS_LAYOUT),
    effect("band_val", 8, IMPLEMENTED | NEEDS_LAYOUT),
    effect("band_pinwheel_sat", 9, IMPLEMENTED | NEEDS_LAYOUT),
    effect("band_pinwheel_val", 10, IMPLEMENTED | NEEDS_LAYOUT),
    effect("band_spiral_sat", 11, IMPLEMENTED | NEEDS_LAYOUT),
    effect("band_spiral_val", 12, IMPLEMENTED | NEEDS_LAYOUT),
    effect("cycle_all", 13, IMPLEMENTED),
    effect("cycle_left_right", 14, IMPLEMENTED | NEEDS_LAYOUT),
    effect("cycle_up_down", 15, IMPLEMENTED | NEEDS_LAYOUT),
    effect("rainbow_moving_chevron", 16, IMPLEMENTED | NEEDS_LAYOUT),
    effect("cycle_out_in", 17, IMPLEMENTED | NEEDS_LAYOUT),
    effect("cycle_out_in_dual", 18, IMPLEMENTED | NEEDS_LAYOUT),
    effect("cycle_pinwheel", 19, IMPLEMENTED | NEEDS_LAYOUT),
    effect("cycle_spiral", 20, IMPLEMENTED | NEEDS_LAYOUT),
    effect("dual_beacon", 21, IMPLEMENTED | NEEDS_LAYOUT),
    effect("rainbow_beacon", 22, IMPLEMENTED | NEEDS_LAYOUT),
    effect("rainbow_pinwheels", 23, IMPLEMENTED | NEEDS_LAYOUT),
    effect("raindrops", 24, IMPLEMENTED),
    effect("jellybean_raindrops", 25, IMPLEMENTED),
    effect("hue_breathing", 26, IMPLEMENTED),
    effect("hue_pendulum", 27, IMPLEMENTED | NEEDS_LAYOUT),
    effect("hue_wave", 28, IMPLEMENTED | NEEDS_LAYOUT),
    effect(
        "typing_heatmap",
        29,
        IMPLEMENTED | NEEDS_MATRIX | REACTIVE | FRAMEBUFFER,
    ),
    effect("digital_rain", 30, IMPLEMENTED | NEEDS_MATRIX | FRAMEBUFFER),
    effect("solid_reactive_simple", 31, IMPLEMENTED | REACTIVE),
    effect("solid_reactive", 32, IMPLEMENTED | REACTIVE),
    effect("solid_reactive_wide", 33, IMPLEMENTED | REACTIVE | NEEDS_LAYOUT),
    effect("solid_reactive_multiwide", 34, IMPLEMENTED | REACTIVE | NEEDS_LAYOUT),
    effect("solid_reactive_cross", 35, IMPLEMENTED | REACTIVE | NEEDS_LAYOUT),
    effect("solid_reactive_multicross", 36, IMPLEMENTED | REACTIVE | NEEDS_LAYOUT),
    effect("solid_reactive_nexus", 37, IMPLEMENTED | REACTIVE | NEEDS_LAYOUT),
    effect("solid_reactive_multinexus", 38, IMPLEMENTED | REACTIVE | NEEDS_LAYOUT),
    effect("splash", 39, IMPLEMENTED | REACTIVE | NEEDS_LAYOUT),
    effect("multisplash", 40, IMPLEMENTED | REACTIVE | NEEDS_LAYOUT),
    effect("solid_splash", 41, IMPLEMENTED | REACTIVE | NEEDS_LAYOUT),
    effect("solid_multisplash", 42, IMPLEMENTED | REACTIVE | NEEDS_LAYOUT),
    effect("pixel_rain", 43, IMPLEMENTED),
    effect("pixel_fractal", 44, IMPLEMENTED | NEEDS_MATRIX),
    // Effects QMK has that Vial's list does not carry, so the panel cannot
    // select them. They are numbered past the Vial range on purpose.
    effect("pixel_flow", 45, IMPLEMENTED),
    effect("starlight_smooth", 46, IMPLEMENTED),
    effect("flower_blooming", 47, IMPLEMENTED | NEEDS_LAYOUT),
    effect("riverflow", 48, IMPLEMENTED),
    effect("starlight", 49, IMPLEMENTED),
    effect("starlight_dual_sat", 50, IMPLEMENTED),
    effect("starlight_dual_hue", 51, IMPLEMENTED),
];

/// Highest Vial effect id, i.e. the largest id Vial's own table defines.
pub const LAST_VIAL_ID: u16 = 44;

/// Largest keyboard matrix the framebuffer effects support, such as the typing
/// heatmap and digital rain. Their buffers are fixed-size statics in the
/// firmware, so a bigger matrix is refused when the configuration is resolved.
pub const MAX_MATRIX_ROWS: usize = 16;
pub const MAX_MATRIX_COLS: usize = 16;

/// Look up an effect by its `rgb.toml` name.
pub fn effect_by_name(name: &str) -> Option<&'static EffectInfo> {
    EFFECTS.iter().find(|effect| effect.name == name)
}

/// Look up an effect by the id stored as the current mode.
pub fn effect_by_id(id: u16) -> Option<&'static EffectInfo> {
    EFFECTS.iter().find(|effect| effect.id == id)
}

/// Effects Vial's Lighting panel can offer.
pub fn vial_effects() -> impl Iterator<Item = &'static EffectInfo> {
    EFFECTS.iter().filter(|effect| effect.id <= LAST_VIAL_ID)
}
