# RGB Lighting

Per-key RGB lighting is configured in `rgb.toml`, a file that sits next to
`keyboard.toml`. The keys and their defaults follow QMK's `rgb_matrix` so that
an existing QMK configuration can be read across, and the animations are QMK's
own, ported to the same arithmetic.

RMK reads `rgb.toml` itself: there is no code to write, and a keyboard without
the file simply has no lighting. Enabling an effect that RMK cannot render, or
writing a key RMK has no behaviour for, fails the build — a name in the Vial
panel that never lights anything up is worse than an error message.

## Requirements

- The `rgb_matrix` Cargo feature of `rmk`. A keyboard file that describes a
  chain without the feature, or the feature without a file, is rejected.
- A board whose chip RMK can drive a WS2812 chain from. Today that is
  `esp32s3`, through one RMT channel.
- `esp-hal-smartled` and `smart-leds-trait` in the firmware's `Cargo.toml`; the
  generated code names them.

## A minimal `rgb.toml`

```toml
[ws2812]
pin = "GPIO4"

[rgb_matrix]
driver = "ws2812"
led_count = 3

[rgb_matrix.animations]
breathing = true

[[rgb_matrix.layout]]
matrix = [0, 0]
x = 0
y = 0
flags = 4

# … one [[rgb_matrix.layout]] entry per LED, in chain order
```

## `[ws2812]`

| Key | Default | Meaning |
|---|---|---|
| `pin` | — | Data pin, written like the pins in `keyboard.toml` (`"GPIO4"`). |
| `color_order` | `"grb"` | Byte order on the wire: `rgb`, `grb`, `bgr`, `rgbw` or `grbw`. Change it if red and green come out swapped. |
| `timing_ns` | `1250` | One bit period, QMK's `WS2812_TIMING`. |
| `t1h_ns` | `900` | High phase of a `1` bit, QMK's `WS2812_T1H`. |
| `t0h_ns` | `350` | High phase of a `0` bit, QMK's `WS2812_T0H`. |
| `reset_us` | `280` | Latch pulse, QMK's `WS2812_TRST_US`. |

## `[rgb_matrix]`

| Key | Default | Meaning |
|---|---|---|
| `driver` | — | `"ws2812"`, the only driver implemented. |
| `led_count` | length of `layout` | Chain length. Must agree with `[[rgb_matrix.layout]]` when both are given. |
| `max_brightness` | `255` | QMK's `RGB_MATRIX_MAXIMUM_BRIGHTNESS`: a ceiling on every brightness a keycode or the host sets, and the scale Vial's slider uses. |
| `timeout` | `0` | QMK's `RGB_MATRIX_TIMEOUT`: milliseconds without a key press after which the chain goes dark. `0` never times out. |
| `led_flush_limit` | `16` | QMK's `RGB_MATRIX_LED_FLUSH_LIMIT`: milliseconds between frames. |
| `react_on_keyup` | `false` | QMK's `RGB_MATRIX_KEYRELEASES`: reactive effects answer releases instead of presses. Needs a reactive effect enabled. |
| `center_point` | `[112, 32]` | QMK's `RGB_MATRIX_CENTER`, the centre the pinwheel, spiral and beacon effects measure from. |
| `hue_steps`, `sat_steps`, `val_steps`, `speed_steps` | `8`, `16`, `16`, `16` | QMK's `RGB_MATRIX_*_STEP`: how far one keycode moves a value. |

QMK keys that RMK has no behaviour for are rejected rather than ignored:
`led_process_limit` (RMK renders the whole chain in one pass, which is what QMK
does when the limit covers the chain), `sleep`, `split_count` and `flag_steps`.

## `[rgb_matrix.default]`

The state the chain starts in, used only while nothing is stored yet. Vial's
SAVE button and the lighting keycodes both write to storage.

| Key | Default |
|---|---|
| `on` | `true` |
| `animation` | `"solid_color"`, and it must be enabled below |
| `hue` | `0` |
| `sat` | `255` |
| `val` | `max_brightness` |
| `speed` | `127` |
| `flags` | `255` (`LED_FLAG_ALL`) |

## `[rgb_matrix.animations]`

Each key is an effect name, and `true` compiles it in. Only the effects listed
here exist in the firmware, and only the ones Vial knows appear in its Lighting
panel.

- Solid colour is always compiled in; QMK gives it no enable flag.
- `off` is not an effect. Being off is `[rgb_matrix.default].on = false`.
- Effects QMK has but this port does not render yet are rejected by name.

## `[[rgb_matrix.layout]]`

One entry per LED, **in chain order**: the first entry is the LED wired to the
data pin. Each entry describes the key that LED sits under.

| Key | Meaning |
|---|---|
| `matrix` | The key's electrical position, `[row, col]`, as in `keyboard.toml`'s `[layout]`. Omit it for a LED with no key, such as underglow. |
| `x`, `y` | The key's position in QMK's coordinate space: `x` spans `0..=224` and `y` spans `0..=64`. The animations assume that space. |
| `flags` | QMK's `LED_FLAG_*` mask. `4` (`LED_FLAG_KEYLIGHT`) for a key backlight. |

The order is the one thing that cannot be derived from the schematic: it is how
the LEDs are wired. Wrong order shows up as effects that run in the wrong
direction. To read the real order, select **Direct Control** in Vial's Lighting
panel and paint one LED at a time.

## Effects

| Effect | Vial id | Notes |
|---|---|---|
| `solid_color` | 2 | Always compiled in |
| `direct` | 1 | Vial's Direct Control painting |
| `alpha_mods` | 3 | Modifier LEDs take a shifted hue |
| `gradient_up_down`, `gradient_left_right` | 4, 5 | Position-based |
| `breathing` | 6 | |
| `band_sat`, `band_val` | 7, 8 | Position-based |
| `band_pinwheel_sat`, `band_pinwheel_val` | 9, 10 | |
| `band_spiral_sat`, `band_spiral_val` | 11, 12 | |
| `cycle_all` | 13 | |
| `cycle_left_right`, `cycle_up_down` | 14, 15 | Position-based |
| `rainbow_moving_chevron` | 16 | |
| `cycle_out_in`, `cycle_out_in_dual` | 17, 18 | |
| `cycle_pinwheel`, `cycle_spiral` | 19, 20 | |
| `dual_beacon`, `rainbow_beacon`, `rainbow_pinwheels` | 21, 22, 23 | |
| `raindrops`, `jellybean_raindrops` | 24, 25 | |
| `hue_breathing`, `hue_pendulum`, `hue_wave` | 26, 27, 28 | |
| `pixel_rain` | 43 | |
| `pixel_flow` | — | QMK only; not in Vial's list |
| `starlight_smooth`, `starlight`, `starlight_dual_sat`, `starlight_dual_hue` | — | QMK only |
| `flower_blooming`, `riverflow` | — | QMK only |

Not ported yet: `typing_heatmap`, `digital_rain`, the twelve
`solid_reactive_*`/`splash` effects, and `pixel_fractal`. They need key events or
the matrix framebuffer.

## How this differs from QMK

- A frame renders the whole chain in one pass, so per-frame effect state
  advances once per frame. That is QMK's behaviour with
  `led_process_limit >= led_count`.
- `FASTLED_SCALE8_FIXED=1` is on, as it is in QMK's own build, so brightness
  behaves identically. Without it everything would be a step darker.
- QMK's `atan2_8` returns a wrapped angle for `dy < 0`; that quirk is kept, so
  the pinwheel and spiral effects have the same discontinuity QMK has.
- The CIE1931 gamma curve is not applied, matching a QMK build that does not
  define `USE_CIE1931_CURVE`.
- Keycodes and Vial writes are persisted to the same user storage slot
  (`0xF0`); QMK uses its EEPROM block.
