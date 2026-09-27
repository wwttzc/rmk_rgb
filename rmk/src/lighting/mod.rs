//! Per-key RGB lighting: QMK's RGB Matrix, ported to RMK.
//!
//! The engine keeps one authoritative state — on/off, effect, hue, saturation,
//! brightness, speed — that keycodes, the Vial Lighting panel and the
//! `rgb.toml` defaults all write to. Whatever the state says is what the
//! configured animations render, one frame every `led_flush_limit`
//! milliseconds, out of the board's LED driver.
//!
//! Where the behaviour comes from QMK, the module implementing it names the QMK
//! source it mirrors, so the two can be read side by side:
//!
//! | RMK | QMK |
//! |---|---|
//! | [`effects`] | `quantum/rgb_matrix/animations/*` |
//! | [`lib8tion`] | `lib/lib8tion/*` |
//! | [`color`] | `quantum/color.c` |
//! | [`LightingProcessor`] | `rgb_matrix_task` plus the driver's flush |
//!
//! The board supplies its constants through [`LightingConfig`], which
//! `#[rmk_keyboard]` generates from `rgb.toml`.

pub mod color;
pub mod effect;
mod effects;
pub mod lib8tion;
mod processor;

pub use processor::LightingProcessor;

use embassy_sync::mutex::Mutex;
use rmk_types::action::LightAction;

use crate::RawMutex;
use crate::lighting::color::{Hsv, Rgb};
use crate::lighting::effect::{Effect, EffectCtx, EffectState};

/// Most LEDs a chain may have. `rmk-macro` refuses a larger `rgb.toml`.
pub const MAX_LEDS: usize = 128;

/// Bytes the stored lighting state takes.
const STATE_BYTES: usize = 8;

/// Storage slot holding the lighting state, from the range reserved for boards.
///
/// RMK's own storage never claims a user slot, so a board that stores something
/// of its own should avoid this one.
#[cfg(feature = "storage")]
pub const STORAGE_SLOT: u8 = 0xF0;

/// Board lighting constants, generated from `rgb.toml`.
#[derive(Clone, Copy)]
pub struct LightingConfig {
    /// Ceiling for every brightness the host or a keycode can set, as QMK's
    /// `RGB_MATRIX_MAXIMUM_BRIGHTNESS`.
    pub max_brightness: u8,
    /// Milliseconds between frames, QMK's `RGB_MATRIX_LED_FLUSH_LIMIT`.
    pub frame_ms: u16,
    /// Milliseconds without a key press after which the chain goes dark, `0`
    /// to never, as QMK's `RGB_MATRIX_TIMEOUT`.
    pub timeout_ms: u32,
    /// Geometric centre the beacon and pinwheel effects measure from.
    pub center: (u8, u8),
    pub hue_steps: u8,
    pub sat_steps: u8,
    pub val_steps: u8,
    pub speed_steps: u8,
    /// Whether reactive effects answer key releases instead of presses, QMK's
    /// `RGB_MATRIX_KEYRELEASES`.
    pub react_on_keyup: bool,
    /// State the chain starts in, when nothing is stored yet.
    pub default_on: bool,
    pub default_mode: u16,
    pub default_hue: u8,
    pub default_sat: u8,
    pub default_val: u8,
    pub default_speed: u8,
    pub default_flags: u8,
    /// Enabled effects, in Vial id order: what the mode keycodes step through.
    pub modes: &'static [u16],
    /// Enabled effects Vial's own list can offer, which is what the panel shows.
    pub vial_modes: &'static [u16],
    /// Each LED's position in QMK's `0..=224` by `0..=64` space, in chain order.
    pub points: &'static [(u8, u8)],
    /// Each LED's `LED_FLAG_*` mask, in chain order.
    pub flags: &'static [u8],
    /// Each LED's electrical matrix position, `None` when the LED has no key.
    pub matrix: &'static [Option<(u8, u8)>],
    /// The keyboard matrix the framebuffer effects iterate over.
    pub matrix_rows: u8,
    pub matrix_cols: u8,
}

impl LightingConfig {
    /// The LED at a matrix position, QMK's `rgb_matrix_map_row_column_to_led`.
    ///
    /// QMK's `matrix_co` is one LED per cell, so the first match is the answer.
    pub fn led_at(&self, row: u8, col: u8) -> Option<usize> {
        self.matrix.iter().position(|cell| *cell == Some((row, col)))
    }
}

/// The live lighting state.
struct LightingState {
    enabled: bool,
    mode: u16,
    hsv: Hsv,
    /// Mask filtering which LEDs render, QMK's `rgb_matrix_config.flags`.
    flags: u8,
    speed: u8,
    last_activity_ms: u32,
    /// Set when the effect or the on/off state changed, so the next frame runs
    /// the effect's init branch — QMK's `rgb_effect_params.init`.
    init_pending: bool,
    /// The mask the frame buffer was rendered with; a change clears it, as QMK
    /// does when `rgb_effect_params.flags` changes.
    rendered_flags: u8,
}

impl LightingState {
    fn new(cfg: &LightingConfig) -> Self {
        Self {
            enabled: cfg.default_on,
            mode: cfg.default_mode,
            hsv: Hsv::new(cfg.default_hue, cfg.default_sat, cfg.default_val),
            flags: cfg.default_flags,
            speed: cfg.default_speed,
            last_activity_ms: 0,
            init_pending: true,
            rendered_flags: cfg.default_flags,
        }
    }

    /// The effect to render now, QMK's
    /// `rgb_current_effect = suspend || !enable ? 0 : mode`.
    fn current_effect(&self, cfg: &LightingConfig, now_ms: u32) -> Effect {
        if !self.enabled {
            return Effect::Off;
        }
        if cfg.timeout_ms > 0 && now_ms.wrapping_sub(self.last_activity_ms) > cfg.timeout_ms {
            return Effect::Off;
        }
        Effect::from_id(self.mode).unwrap_or(Effect::SolidColor)
    }

    fn to_bytes(&self) -> [u8; STATE_BYTES] {
        let mode = self.mode.to_le_bytes();
        [
            self.enabled as u8,
            mode[0],
            mode[1],
            self.speed,
            self.hsv.h,
            self.hsv.s,
            self.hsv.v,
            self.flags,
        ]
    }

    /// Apply a stored state, ignoring bytes that do not decode.
    fn restore_from(&mut self, bytes: &[u8], cfg: &LightingConfig) {
        if bytes.len() < STATE_BYTES {
            return;
        }
        let mode = u16::from_le_bytes([bytes[1], bytes[2]]);
        if Effect::from_id(mode).is_none() {
            return;
        }
        self.enabled = bytes[0] != 0;
        self.mode = mode;
        self.speed = bytes[3];
        self.hsv = Hsv {
            h: bytes[4],
            s: bytes[5],
            v: bytes[6].min(cfg.max_brightness),
        };
        self.flags = bytes[7];
        self.init_pending = true;
    }
}

/// The started engine: the board's constants, the state, and what the effects
/// remember between frames.
struct Active {
    cfg: &'static LightingConfig,
    state: LightingState,
    effects: EffectState,
}

static ACTIVE: Mutex<RawMutex, Option<Active>> = Mutex::new(None);

/// Hand the engine its board constants. Called once, by [`LightingProcessor`].
pub async fn start(cfg: &'static LightingConfig) {
    let mut active = ACTIVE.lock().await;
    if active.is_none() {
        *active = Some(Active {
            cfg,
            state: LightingState::new(cfg),
            effects: EffectState::new(),
        });
    }
}

/// Render one frame into `frame`.
///
/// `frame` is the driver's buffer and is deliberately kept between frames:
/// several effects (RAINDROPS, STARLIGHT, PIXEL_RAIN) light a single LED per
/// frame and expect the rest to stay as they were, exactly as QMK's shared
/// buffer does.
pub async fn render(frame: &mut [Rgb], now_ms: u32) {
    let mut guard = ACTIVE.lock().await;
    let Some(active) = guard.as_mut() else {
        return;
    };

    // QMK's `rgb_task_timers` then `rgb_task_start`: hits age by the time since
    // the last frame, and the aged set is what effects read this frame.
    let delta = now_ms.wrapping_sub(active.effects.last_frame_ms);
    active.effects.last_frame_ms = now_ms;
    active.effects.hits_buffer.age(delta);
    active.effects.hits = active.effects.hits_buffer;

    let effect = active.state.current_effect(active.cfg, now_ms);
    let init = active.state.init_pending;
    active.state.init_pending = false;

    if active.state.rendered_flags != active.state.flags {
        active.state.rendered_flags = active.state.flags;
        frame.fill(Rgb::BLACK);
    }

    let mut ctx = EffectCtx {
        cfg: active.cfg,
        hsv: active.state.hsv,
        speed: active.state.speed,
        flags: active.state.flags,
        timer: now_ms,
        init,
        state: &mut active.effects,
    };
    effects::render(effect, &mut ctx, frame);
}

/// Note input activity, which the timeout counts from.
pub async fn on_activity(now_ms: u32) {
    if let Some(active) = ACTIVE.lock().await.as_mut() {
        active.state.last_activity_ms = now_ms;
    }
}

/// A key event, which the reactive effects and the typing heatmap answer.
///
/// This is QMK's `rgb_matrix_handle_key_event`: the hit is recorded on press,
/// or on release when `react_on_keyup` is set, and the heatmap's ingress runs
/// only while its effect is the selected one.
pub async fn on_key_event(row: u8, col: u8, pressed: bool) {
    let mut guard = ACTIVE.lock().await;
    let Some(active) = guard.as_mut() else {
        return;
    };
    let cfg = active.cfg;
    let reacts_to = if cfg.react_on_keyup { !pressed } else { pressed };
    if !reacts_to {
        return;
    }

    if let Some(led) = cfg.led_at(row, col) {
        let (x, y) = cfg.points[led];
        active.effects.hits_buffer.record(led as u8, x, y);
    }
    if active.state.mode == Effect::TypingHeatmap.id() {
        effects::typing_heatmap_key(cfg, &mut active.effects, row, col);
    }
}

/// Everything the host reads back about the lighting state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub enabled: bool,
    pub mode: u16,
    pub speed: u8,
    pub hsv: Hsv,
}

/// The current state, or `None` before the engine started.
pub async fn snapshot() -> Option<Snapshot> {
    let guard = ACTIVE.lock().await;
    let active = guard.as_ref()?;
    Some(Snapshot {
        enabled: active.state.enabled,
        mode: active.state.mode,
        speed: active.state.speed,
        hsv: active.state.hsv,
    })
}

/// Turn the chain on or off, as QMK's `rgb_matrix_toggle`.
pub async fn set_enabled(enabled: bool) {
    if let Some(active) = ACTIVE.lock().await.as_mut()
        && active.state.enabled != enabled
    {
        active.state.enabled = enabled;
        active.state.init_pending = true;
    }
}

/// Select an effect with its colour and speed, as the Vial Lighting panel does.
///
/// QMK ignores these while the chain is off, so Vial turns it on first. Mode
/// zero is not selectable — it is the off state — and an effect this firmware
/// does not render keeps the current one instead of going dark.
pub async fn set_mode_and_colour(mode: u16, speed: u8, hsv: Hsv) {
    if let Some(active) = ACTIVE.lock().await.as_mut() {
        if !active.state.enabled {
            return;
        }
        if mode > 0
            && mode != active.state.mode
            && let Some(effect) = Effect::from_id(mode)
            && effect != Effect::Off
        {
            active.state.mode = mode;
            active.state.init_pending = true;
        }
        active.state.speed = speed;
        active.state.hsv = Hsv {
            h: hsv.h,
            s: hsv.s,
            v: hsv.v.min(active.cfg.max_brightness),
        };
    }
}

/// Paint LEDs directly, the way Vial's direct control mode does.
///
/// `first_led` is the index of the first colour in `colours`.
pub async fn set_direct_colours(first_led: u16, colours: &[Hsv]) {
    if let Some(active) = ACTIVE.lock().await.as_mut() {
        let max = active.cfg.max_brightness;
        let count = active.cfg.points.len();
        for (offset, colour) in colours.iter().enumerate() {
            let led = first_led as usize + offset;
            if led >= count {
                break;
            }
            active.effects.direct[led] = Hsv {
                v: colour.v.min(max),
                ..*colour
            };
        }
    }
}

/// The LED count, for the host's direct control handshake.
pub async fn led_count() -> Option<u16> {
    Some(ACTIVE.lock().await.as_ref()?.cfg.points.len() as u16)
}

/// One LED's position, flags and matrix position, as VialRGB's `get_led_info`.
pub async fn led_info(led: u16) -> Option<(u8, u8, u8, u8, u8)> {
    let guard = ACTIVE.lock().await;
    let active = guard.as_ref()?;
    let led = led as usize;
    if led >= active.cfg.points.len() {
        return None;
    }
    let (x, y) = active.cfg.points[led];
    let (row, col) = active.cfg.matrix[led].unwrap_or((0xFF, 0xFF));
    Some((x, y, active.cfg.flags[led], row, col))
}

/// The brightness ceiling Vial scales its slider against.
pub async fn max_brightness() -> Option<u8> {
    Some(ACTIVE.lock().await.as_ref()?.cfg.max_brightness)
}

/// The enabled effects Vial's panel may offer, in Vial id order.
pub async fn vial_modes() -> Option<&'static [u16]> {
    Some(ACTIVE.lock().await.as_ref()?.cfg.vial_modes)
}

/// Apply a lighting keycode.
///
/// Keycodes act on press only and persist their change, matching QMK's
/// `RGB_TOG`/`RGB_MOD`/`RGB_HUI`-style keycodes, which all write to EEPROM.
pub async fn apply_light_action(action: LightAction, pressed: bool) {
    if !pressed {
        return;
    }
    let mut guard = ACTIVE.lock().await;
    let Some(active) = guard.as_mut() else {
        return;
    };
    let cfg = active.cfg;
    let state = &mut active.state;

    // Every QMK lighting keycode funnels through `mode` or `sethsv`, and both
    // return early while the chain is off. Only the toggle works when it is dark.
    if !state.enabled && action != LightAction::RgbTog {
        return;
    }

    match action {
        LightAction::RgbTog => state.enabled = !state.enabled,
        LightAction::RgbModeForward => state.mode = step_mode(cfg, state.mode, true),
        LightAction::RgbModeReverse => state.mode = step_mode(cfg, state.mode, false),
        LightAction::RgbHui => state.hsv.h = state.hsv.h.wrapping_add(cfg.hue_steps),
        LightAction::RgbHud => state.hsv.h = state.hsv.h.wrapping_sub(cfg.hue_steps),
        LightAction::RgbSai => state.hsv.s = state.hsv.s.saturating_add(cfg.sat_steps),
        LightAction::RgbSad => state.hsv.s = state.hsv.s.saturating_sub(cfg.sat_steps),
        LightAction::RgbVai => state.hsv.v = state.hsv.v.saturating_add(cfg.val_steps).min(cfg.max_brightness),
        LightAction::RgbVad => state.hsv.v = state.hsv.v.saturating_sub(cfg.val_steps),
        LightAction::RgbSpi => state.speed = state.speed.saturating_add(cfg.speed_steps),
        LightAction::RgbSpd => state.speed = state.speed.saturating_sub(cfg.speed_steps),
        // The RGBLight effects have no RGB Matrix equivalent; the two with a
        // close one select it, the rest leave the current effect alone.
        LightAction::RgbModePlain => state.mode = Effect::SolidColor.id(),
        LightAction::RgbModeBreathe => state.mode = Effect::Breathing.id(),
        LightAction::RgbModeRainbow => state.mode = Effect::CycleAll.id(),
        LightAction::RgbModeSwirl
        | LightAction::RgbModeSnake
        | LightAction::RgbModeKnight
        | LightAction::RgbModeXmas
        | LightAction::RgbModeGradient
        | LightAction::RgbModeRgbtest
        | LightAction::RgbModeTwinkle
        | LightAction::BacklightOn
        | LightAction::BacklightOff
        | LightAction::BacklightToggle
        | LightAction::BacklightDown
        | LightAction::BacklightUp
        | LightAction::BacklightStep
        | LightAction::BacklightToggleBreathing => return,
        _ => return,
    }

    state.init_pending = true;
    let bytes = state.to_bytes();
    drop(guard);

    #[cfg(feature = "storage")]
    crate::storage::store_user_data(STORAGE_SLOT, &bytes).await.ok();
    #[cfg(not(feature = "storage"))]
    let _ = bytes;
}

/// Step through the enabled effects, wrapping at both ends.
fn step_mode(cfg: &LightingConfig, mode: u16, forward: bool) -> u16 {
    let modes = cfg.modes;
    if modes.is_empty() {
        return mode;
    }
    match modes.iter().position(|&id| id == mode) {
        Some(index) if forward => modes[(index + 1) % modes.len()],
        Some(0) => modes[modes.len() - 1],
        Some(index) => modes[index - 1],
        None if forward => modes[0],
        None => modes[modes.len() - 1],
    }
}

/// Restore the stored lighting state, if the board has storage.
pub async fn restore() {
    #[cfg(feature = "storage")]
    {
        let stored = crate::storage::read_user_data(STORAGE_SLOT).await;
        let mut guard = ACTIVE.lock().await;
        if let (Some(bytes), Some(active)) = (stored, guard.as_mut()) {
            active.state.restore_from(&bytes, active.cfg);
        }
    }
}

/// Persist the lighting state, as Vial's SAVE does.
pub async fn save() {
    #[cfg(feature = "storage")]
    {
        let guard = ACTIVE.lock().await;
        let Some(active) = guard.as_ref() else {
            return;
        };
        let bytes = active.state.to_bytes();
        drop(guard);
        crate::storage::store_user_data(STORAGE_SLOT, &bytes).await.ok();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static POINTS: [(u8, u8); 2] = [(0, 0), (224, 64)];
    static FLAGS: [u8; 2] = [4, 4];
    static MATRIX: [Option<(u8, u8)>; 2] = [Some((0, 0)), Some((1, 1))];
    static MODES: [u16; 2] = [2, 6];
    static CFG: LightingConfig = LightingConfig {
        max_brightness: 128,
        frame_ms: 16,
        timeout_ms: 0,
        center: (112, 32),
        hue_steps: 8,
        sat_steps: 16,
        val_steps: 16,
        speed_steps: 16,
        react_on_keyup: false,
        default_on: false,
        default_mode: 6,
        default_hue: 0,
        default_sat: 255,
        default_val: 128,
        default_speed: 127,
        default_flags: 255,
        modes: &MODES,
        vial_modes: &MODES,
        points: &POINTS,
        flags: &FLAGS,
        matrix: &MATRIX,
        matrix_rows: 2,
        matrix_cols: 2,
    };

    #[test]
    fn the_stored_state_round_trips() {
        let mut state = LightingState::new(&CFG);
        state.enabled = true;
        state.mode = 2;
        state.speed = 42;
        state.hsv = Hsv::new(1, 2, 3);

        let bytes = state.to_bytes();
        let mut restored = LightingState::new(&CFG);
        restored.restore_from(&bytes, &CFG);

        assert!(restored.enabled, "the stored on/off wins over the default");
        assert_eq!(restored.mode, 2);
        assert_eq!(restored.speed, 42);
        assert_eq!(restored.hsv, Hsv::new(1, 2, 3));
        assert!(restored.init_pending, "a restored effect starts on its init frame");
    }

    #[test]
    fn stored_bytes_that_do_not_decode_leave_the_defaults_alone() {
        let mut state = LightingState::new(&CFG);
        state.restore_from(&[1, 2, 3], &CFG);
        assert!(!state.enabled, "a short blob is ignored");

        // Mode 99 is not an effect this firmware has.
        state.restore_from(&[1, 99, 0, 0, 0, 0, 0, 0], &CFG);
        assert!(!state.enabled, "an unknown mode is ignored");
        assert_eq!(state.mode, CFG.default_mode);
    }

    #[test]
    fn a_stored_brightness_above_the_ceiling_is_clamped() {
        let mut state = LightingState::new(&CFG);
        state.restore_from(&[1, 2, 0, 100, 10, 20, 250, 255], &CFG);
        assert_eq!(state.hsv.v, CFG.max_brightness);
    }

    #[test]
    fn the_effect_follows_the_enable_state_and_the_timeout() {
        let mut state = LightingState::new(&CFG);
        assert_eq!(state.current_effect(&CFG, 0), Effect::Off, "off by default");
        state.enabled = true;
        assert_eq!(state.current_effect(&CFG, 0), Effect::Breathing);

        // A timeout only matters once one is configured.
        let mut timed = CFG;
        timed.timeout_ms = 1000;
        let mut state = LightingState::new(&timed);
        state.enabled = true;
        state.last_activity_ms = 500;
        assert_eq!(state.current_effect(&timed, 1400), Effect::Breathing);
        assert_eq!(state.current_effect(&timed, 1600), Effect::Off, "idle too long");
    }

    /// Every lighting keycode writes the same state the host and `rgb.toml`
    /// write, so what one does shows up in the next snapshot.
    #[test]
    fn lighting_keycodes_move_the_state() {
        crate::test_support::test_block_on(async {
            start(&CFG).await;
            // Each keycode persists its change, and no storage task drains that
            // queue here, so clear it before the next one fills the channel.
            let drain = |action| async move {
                apply_light_action(action, true).await;
                crate::test_support::clear_flash_channel();
            };

            set_enabled(false).await;
            let dark = snapshot().await.unwrap();
            assert!(!dark.enabled, "the test configuration starts dark");

            // QMK ignores the value keycodes while the chain is off.
            drain(LightAction::RgbHui).await;
            assert_eq!(snapshot().await.unwrap().hsv.h, dark.hsv.h);
            drain(LightAction::RgbModeForward).await;
            assert_eq!(snapshot().await.unwrap().mode, dark.mode);

            drain(LightAction::RgbTog).await;
            assert!(snapshot().await.unwrap().enabled);

            let hue = snapshot().await.unwrap().hsv.h;
            drain(LightAction::RgbHui).await;
            assert_eq!(snapshot().await.unwrap().hsv.h, hue.wrapping_add(CFG.hue_steps));
            drain(LightAction::RgbHud).await;
            assert_eq!(snapshot().await.unwrap().hsv.h, hue);

            // A release does nothing, so a held key does not repeat.
            apply_light_action(LightAction::RgbHui, false).await;
            assert_eq!(snapshot().await.unwrap().hsv.h, hue);

            // Brightness stops at the ceiling even when the key is hammered.
            for _ in 0..8 {
                drain(LightAction::RgbVai).await;
            }
            assert_eq!(snapshot().await.unwrap().hsv.v, CFG.max_brightness);

            // The mode keycodes step through what `rgb.toml` enabled, and wrap.
            assert_eq!(snapshot().await.unwrap().mode, CFG.default_mode);
            drain(LightAction::RgbModeForward).await;
            let forward = snapshot().await.unwrap().mode;
            assert_ne!(forward, CFG.default_mode, "the step leaves the current effect");
            assert!(CFG.modes.contains(&forward), "{forward} is not an enabled effect");
            drain(LightAction::RgbModeReverse).await;
            assert_eq!(snapshot().await.unwrap().mode, CFG.default_mode);

            // A backlight keycode is not the RGB matrix's business.
            let speed = snapshot().await.unwrap().speed;
            apply_light_action(LightAction::BacklightOn, true).await;
            assert_eq!(snapshot().await.unwrap().speed, speed);
        });
    }
}
