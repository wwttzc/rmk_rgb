//! Per-key RGB as the Vial Lighting panel sees it.
//!
//! The panel talks to the firmware through VIA's custom value commands, with
//! VialRGB's own subcommands in the second byte. `rgb.toml` decides what exists,
//! so these cases pin down both what a board announces and what a write does.

use rmk::k;
use rmk::lighting::{self, LightingConfig};
use rmk::test_support::test_block_on;
use rmk_types::protocol::vial::{VIAL_EP_SIZE as REPORT, ViaCommand};

use crate::simulator::SimKeyboard;

/// The VialRGB subcommands, from `vial-qmk/quantum/vialrgb.h`.
const GET_INFO: u8 = 0x40;
const SET_MODE: u8 = 0x41;
const GET_MODE: u8 = 0x41;
const GET_SUPPORTED: u8 = 0x42;
const GET_NUMBER_LEDS: u8 = 0x43;
const GET_LED_INFO: u8 = 0x44;

/// Two LEDs in one matrix row, far enough apart that their positions cannot be
/// confused for each other.
static POINTS: [(u8, u8); 2] = [(0, 32), (224, 32)];
static FLAGS: [u8; 2] = [4, 4];
static MATRIX: [Option<(u8, u8)>; 2] = [Some((0, 0)), Some((0, 1))];
/// Solid colour and breathing: what the panel may offer, in Vial id order.
static MODES: [u16; 2] = [2, 6];
static CFG: LightingConfig = LightingConfig {
    max_brightness: 200,
    frame_ms: 16,
    timeout_ms: 0,
    center: (112, 32),
    hue_steps: 8,
    sat_steps: 16,
    val_steps: 16,
    speed_steps: 16,
    react_on_keyup: false,
    default_on: false,
    default_mode: 2,
    default_hue: 0,
    default_sat: 255,
    default_val: 200,
    default_speed: 127,
    default_flags: 255,
    modes: &MODES,
    vial_modes: &MODES,
    points: &POINTS,
    flags: &FLAGS,
    matrix: &MATRIX,
    matrix_rows: 1,
    matrix_cols: 2,
};

fn get(subcommand: u8) -> [u8; REPORT] {
    let mut data = [0; REPORT];
    data[0] = ViaCommand::CustomGetValue as u8;
    data[1] = subcommand;
    data
}

fn set(subcommand: u8) -> [u8; REPORT] {
    let mut data = [0; REPORT];
    data[0] = ViaCommand::CustomSetValue as u8;
    data[1] = subcommand;
    data
}

#[test]
fn the_panel_reads_what_rgb_toml_declares() {
    test_block_on(async {
        lighting::start(&CFG).await;
        let mut keyboard = SimKeyboard::builder([[[k!(A), k!(B)]]]).build().await;

        // The protocol version is what makes the Lighting panel appear at all,
        // and the panel reads the brightness ceiling from the same reply.
        let mut info = get(GET_INFO);
        info[2] = 1;
        info[3] = 0;
        info[4] = 200;
        keyboard.host_exchange(get(GET_INFO), info);

        // Direct control needs the chain length …
        let mut leds = get(GET_NUMBER_LEDS);
        leds[2] = 2;
        keyboard.host_exchange(get(GET_NUMBER_LEDS), leds);

        // … and each LED's position, flags and matrix position.
        let mut led_info = get(GET_LED_INFO);
        led_info[2] = 224;
        led_info[3] = 32;
        led_info[4] = 4;
        led_info[5] = 0;
        led_info[6] = 1;
        let mut request = get(GET_LED_INFO);
        request[2] = 1;
        keyboard.host_exchange(request, led_info);

        // The offer a panel browses: OFF first, then the enabled effects, with
        // the rest of the reply left at 0xFF as an end marker.
        let mut supported = get(GET_SUPPORTED);
        supported[2] = 2;
        supported[3] = 0;
        supported[4] = 6;
        supported[5] = 0;
        supported[6..].fill(0xFF);
        keyboard.host_exchange(get(GET_SUPPORTED), supported);

        keyboard.run().await;
    });
}

#[test]
fn a_mode_write_reaches_the_engine_and_comes_back() {
    test_block_on(async {
        lighting::start(&CFG).await;
        lighting::set_enabled(false).await;
        let mut keyboard = SimKeyboard::builder([[[k!(A), k!(B)]]]).build().await;

        // Vial turns the chain on with the mode it wants, carrying the speed and
        // the colour in the same packet.
        let mut mode = set(SET_MODE);
        mode[2] = 6;
        mode[4] = 200;
        mode[5] = 10;
        mode[6] = 20;
        mode[7] = 30;
        keyboard.host_exchange(mode, mode);

        let mut read = get(GET_MODE);
        read[2] = 6;
        read[4] = 200;
        read[5] = 10;
        read[6] = 20;
        read[7] = 30;
        keyboard.host_exchange(get(GET_MODE), read);

        // Id zero is Vial's OFF: the panel turns the chain off rather than
        // selecting a mode. The colour and speed it was given stay put, as they
        // do in QMK, so only the mode reads back as none.
        keyboard.host_exchange(set(SET_MODE), set(SET_MODE));
        let mut off_read = get(GET_MODE);
        off_read[4] = 200;
        off_read[5] = 10;
        off_read[6] = 20;
        off_read[7] = 30;
        keyboard.host_exchange(get(GET_MODE), off_read);

        // SAVE is what writes the state to storage.
        let mut save = [0; REPORT];
        save[0] = ViaCommand::CustomSave as u8;
        keyboard.host_exchange(save, save);

        keyboard.run().await;
    });
}
