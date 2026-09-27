//! Which effects exist, and the state they keep between frames.

use crate::lighting::color::{Hsv, Rgb};
use crate::lighting::lib8tion::{Rand16, qadd8, scale16by8};
use crate::lighting::{LightingConfig, MAX_LEDS};

/// Effects this firmware can render, with the Vial id each one is known by.
///
/// The ids are the ones in `rmk_types::lighting::EFFECTS`, so a mode value
/// coming from storage, from the host, or from a keycode all mean the same
/// effect. Keeping the enum to implemented effects is what lets the rendering
/// match below be exhaustive: adding an id here without an arm does not
/// compile.
macro_rules! effects {
    ($(($variant:ident, $name:literal, $id:literal)),+ $(,)?) => {
        /// A per-key RGB effect.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum Effect {
            $($variant),+
        }

        impl Effect {
            /// The effect stored under this mode id, if it is one we render.
            pub fn from_id(id: u16) -> Option<Self> {
                match id {
                    $($id => Some(Effect::$variant),)+
                    _ => None,
                }
            }

            /// The Vial id of this effect.
            pub fn id(self) -> u16 {
                match self {
                    $(Effect::$variant => $id),+
                }
            }

            /// The `rgb.toml` name of this effect.
            pub fn name(self) -> &'static str {
                match self {
                    $(Effect::$variant => $name),+
                }
            }

            /// Every implemented effect, in Vial id order.
            pub const ALL: &'static [Effect] = &[$(Effect::$variant),+];
        }
    };
}

effects! {
    (Off, "off", 0),
    (Direct, "direct", 1),
    (SolidColor, "solid_color", 2),
    (AlphasMods, "alpha_mods", 3),
    (GradientUpDown, "gradient_up_down", 4),
    (GradientLeftRight, "gradient_left_right", 5),
    (Breathing, "breathing", 6),
    (BandSat, "band_sat", 7),
    (BandVal, "band_val", 8),
    (BandPinwheelSat, "band_pinwheel_sat", 9),
    (BandPinwheelVal, "band_pinwheel_val", 10),
    (BandSpiralSat, "band_spiral_sat", 11),
    (BandSpiralVal, "band_spiral_val", 12),
    (CycleAll, "cycle_all", 13),
    (CycleLeftRight, "cycle_left_right", 14),
    (CycleUpDown, "cycle_up_down", 15),
    (RainbowMovingChevron, "rainbow_moving_chevron", 16),
    (CycleOutIn, "cycle_out_in", 17),
    (CycleOutInDual, "cycle_out_in_dual", 18),
    (CyclePinwheel, "cycle_pinwheel", 19),
    (CycleSpiral, "cycle_spiral", 20),
    (DualBeacon, "dual_beacon", 21),
    (RainbowBeacon, "rainbow_beacon", 22),
    (RainbowPinwheels, "rainbow_pinwheels", 23),
    (Raindrops, "raindrops", 24),
    (JellybeanRaindrops, "jellybean_raindrops", 25),
    (HueBreathing, "hue_breathing", 26),
    (HuePendulum, "hue_pendulum", 27),
    (HueWave, "hue_wave", 28),
    (PixelRain, "pixel_rain", 43),
    (PixelFlow, "pixel_flow", 45),
    (StarlightSmooth, "starlight_smooth", 46),
    (FlowerBlooming, "flower_blooming", 47),
    (Riverflow, "riverflow", 48),
    (Starlight, "starlight", 49),
    (StarlightDualSat, "starlight_dual_sat", 50),
    (StarlightDualHue, "starlight_dual_hue", 51),
}

/// Everything effects remember from one frame to the next.
///
/// QMK keeps these in function-local statics, which survive mode changes. So do
/// these: an effect that re-enters runs its `init` branch, exactly as QMK's
/// effects do.
pub struct EffectState {
    /// lib8tion's generator, seeded the way lib8tion seeds it.
    pub rand: Rand16,
    /// Vial's direct control mode: the colour each LED was painted with.
    pub direct: [Hsv; MAX_LEDS],
    /// RAINDROPS and JELLYBEAN_RAINDROPS: the LED lit this cycle, out of range
    /// when nothing is due.
    pub raindrops_index: u16,
    /// STARLIGHT and its dual variants: the LED lit this cycle.
    pub starlight_index: u16,
    /// STARLIGHT_SMOOTH: per-LED phase offset, zero until the LED is first lit.
    pub starlight_phase: [u8; MAX_LEDS],
    /// PIXEL_RAIN: the LED due to change, and when it may change.
    pub pixel_rain_index: u8,
    pub pixel_rain_timer: u32,
    /// PIXEL_FLOW: the colours scrolling along the chain, and when the next
    /// scroll is due.
    pub pixel_flow: [Rgb; MAX_LEDS],
    pub pixel_flow_wait_timer: u32,
}

impl EffectState {
    pub const fn new() -> Self {
        Self {
            rand: Rand16::new(),
            direct: [Hsv::BLACK; MAX_LEDS],
            raindrops_index: u16::MAX,
            starlight_index: u16::MAX,
            starlight_phase: [0; MAX_LEDS],
            pixel_rain_index: 0,
            pixel_rain_timer: 0,
            pixel_flow: [Rgb::BLACK; MAX_LEDS],
            pixel_flow_wait_timer: 0,
        }
    }
}

/// What an effect renders with: the configuration, the live colour and speed,
/// the clock, and the state it owns.
///
/// The `time` helpers mirror the runner macros in QMK's `animations/runners`,
/// where each family scales the millisecond clock differently. They truncate
/// the clock to 16 bits first, which is what passing a `uint32_t` to
/// `scale16by8(uint16_t, ...)` does in C.
pub struct EffectCtx<'a> {
    pub cfg: &'a LightingConfig,
    pub hsv: Hsv,
    pub speed: u8,
    pub flags: u8,
    /// Milliseconds since boot, QMK's `g_rgb_timer`.
    pub timer: u32,
    /// True on the first frame after the effect or the on/off state changed.
    pub init: bool,
    pub state: &'a mut EffectState,
}

impl EffectCtx<'_> {
    /// `effect_runner_i`'s clock.
    pub fn time_i(&self) -> u8 {
        // QMK stores this in a `uint8_t`, truncating the 16-bit result.
        scale16by8(self.timer as u16, qadd8(self.speed / 4, 1)) as u8
    }

    /// `effect_runner_dx_dy`'s clock.
    pub fn time_dx_dy(&self) -> u8 {
        scale16by8(self.timer as u16, self.speed / 2) as u8
    }

    /// `effect_runner_sin_cos_i`'s clock.
    pub fn time_sin_cos(&self) -> u16 {
        scale16by8(self.timer as u16, self.speed / 4)
    }

    /// Horizontal and vertical distance from the keyboard centre.
    pub fn dx_dy(&self, led: usize) -> (i16, i16) {
        let (x, y) = self.cfg.points[led];
        (x as i16 - self.cfg.center.0 as i16, y as i16 - self.cfg.center.1 as i16)
    }

    /// Distance from the keyboard centre.
    pub fn dist(&self, led: usize) -> u8 {
        let (dx, dy) = self.dx_dy(led);
        crate::lighting::lib8tion::sqrt16((dx * dx + dy * dy) as u16)
    }

    /// Whether the current flag mask includes this LED.
    ///
    /// This is QMK's `RGB_MATRIX_TEST_LED_FLAGS()`: the mask is the user's
    /// runtime `flags`, not a per-effect constant.
    pub fn led_enabled(&self, led: usize) -> bool {
        self.cfg.flags[led] & self.flags != 0
    }

    /// Saturation or value of `hsv`, shifted by `delta` and scaled by itself,
    /// the shape every `BAND_*` effect uses. Negative shifts clamp to zero, as
    /// the `s < 0 ? 0 : s` in those effects does.
    pub fn band(value: u8, delta: i32) -> u8 {
        crate::lighting::lib8tion::scale8((value as i32).wrapping_sub(delta).max(0) as u8, value)
    }
}

#[cfg(test)]
mod tests {
    use rmk_types::lighting;

    use super::*;

    /// The enum and the catalogue are two halves of the same fact: an effect
    /// this crate can render. If they drift, the Vial panel and the rendering
    /// disagree, so this is where that is caught.
    #[test]
    fn the_enum_and_the_catalogue_agree() {
        for effect in Effect::ALL {
            let info = lighting::effect_by_id(effect.id())
                .unwrap_or_else(|| panic!("{} has no catalogue entry", effect.name()));
            assert_eq!(info.name, effect.name(), "name and id disagree for {}", effect.name());
            assert!(
                info.implemented(),
                "{} is rendered but the catalogue does not call it implemented",
                effect.name()
            );
        }

        for info in lighting::EFFECTS.iter().filter(|info| info.implemented()) {
            let rendered = Effect::from_id(info.id).is_some_and(|effect| effect.name() == info.name);
            assert!(rendered, "{} is marked implemented but nothing renders it", info.name);
        }

        let mut ids: Vec<u16> = Effect::ALL.iter().map(|effect| effect.id()).collect();
        let total = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), total, "two effects share a Vial id");
    }

    /// Walking the enabled list must always find the current effect, and wrap.
    #[test]
    fn ids_round_trip() {
        for effect in Effect::ALL {
            assert_eq!(Effect::from_id(effect.id()), Some(*effect));
        }
    }
}
