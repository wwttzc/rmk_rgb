//! Vial's RGB lighting protocol, ported from `vial-qmk/quantum/vialrgb.c`.
//!
//! Vial reaches lighting through VIA's custom value commands, and puts its own
//! subcommand straight in the second byte, starting at 0x40 so it cannot be
//! confused with VIA's `id_qmk_rgb_matrix_*` values below that range.

use byteorder::{ByteOrder, LittleEndian};

use crate::hid::ViaReport;
use crate::lighting::{self, color::Hsv};

/// Subcommand carrying the protocol version and the brightness ceiling.
const GET_INFO: u8 = 0x40;
/// Set the effect, its speed and its colour.
const SET_MODE: u8 = 0x41;
/// Read the current effect, speed and colour.
const GET_MODE: u8 = 0x41;
/// List the effects the panel may offer.
const GET_SUPPORTED: u8 = 0x42;
/// Paint a run of LEDs directly.
const DIRECT_FASTSET: u8 = 0x42;
const GET_NUMBER_LEDS: u8 = 0x43;
const GET_LED_INFO: u8 = 0x44;

const PROTOCOL_VERSION: u16 = 1;

/// Most LEDs one `direct_fastset` packet can carry: 27 payload bytes over three
/// channels each.
const DIRECT_LEDS_PER_PACKET: usize = 9;

/// Whether a custom value packet is addressed to the lighting protocol.
pub(crate) const fn is_lighting(subcommand: u8) -> bool {
    subcommand >= GET_INFO
}

/// Answer a `CustomGetValue` packet.
pub(crate) async fn process_get(report: &mut ViaReport) {
    let subcommand = report.output_data[1];
    let args = &mut report.input_data[2..];

    match subcommand {
        GET_INFO => {
            args[0] = PROTOCOL_VERSION as u8;
            args[1] = (PROTOCOL_VERSION >> 8) as u8;
            args[2] = lighting::max_brightness().await.unwrap_or(0);
        }
        GET_MODE => {
            if let Some(state) = lighting::snapshot().await {
                // Vial reads "no effect" as its own OFF id.
                let mode = if state.enabled { state.mode } else { 0 };
                LittleEndian::write_u16(&mut args[0..2], mode);
                args[2] = state.speed;
                args[3] = state.hsv.h;
                args[4] = state.hsv.s;
                args[5] = state.hsv.v;
            }
        }
        GET_SUPPORTED => {
            let after = LittleEndian::read_u16(&args[0..2]);
            args.fill(0xFF);
            let modes = lighting::vial_modes().await.unwrap_or(&[]);
            let mut offset = 0;
            for &mode in modes.iter().filter(|&&mode| mode > after) {
                if offset + 2 > args.len() {
                    break;
                }
                args[offset..offset + 2].copy_from_slice(&mode.to_le_bytes());
                offset += 2;
            }
        }
        GET_NUMBER_LEDS => {
            let count = lighting::led_count().await.unwrap_or(0);
            LittleEndian::write_u16(&mut args[0..2], count);
        }
        GET_LED_INFO => {
            // The C original reads the index out of the first byte only.
            if let Some((x, y, flags, row, col)) = lighting::led_info(args[0] as u16).await {
                args[0] = x;
                args[1] = y;
                args[2] = flags;
                args[3] = row;
                args[4] = col;
            }
        }
        _ => {}
    }
}

/// Answer a `CustomSetValue` packet.
pub(crate) async fn process_set(report: &mut ViaReport) {
    let subcommand = report.output_data[1];
    let args = &report.output_data[2..];

    match subcommand {
        SET_MODE => {
            let mode = LittleEndian::read_u16(&args[0..2]);
            if mode == 0 {
                lighting::set_enabled(false).await;
                return;
            }
            // Vial expects the panel to be usable as soon as it picks an
            // effect, so this turns the chain on first, as the C original does.
            lighting::set_enabled(true).await;
            lighting::set_mode_and_colour(mode, args[2], Hsv::new(args[3], args[4], args[5])).await;
        }
        DIRECT_FASTSET => {
            let first = LittleEndian::read_u16(&args[0..2]);
            let count = (args[2] as usize).min(DIRECT_LEDS_PER_PACKET);
            let mut colours = [Hsv::default(); DIRECT_LEDS_PER_PACKET];
            for (index, colour) in colours.iter_mut().enumerate().take(count) {
                *colour = Hsv::new(args[3 + index * 3], args[4 + index * 3], args[5 + index * 3]);
            }
            lighting::set_direct_colours(first, &colours[..count]).await;
        }
        _ => {}
    }
}

/// Answer a `CustomSave` packet, which is Vial's SAVE button.
///
/// VIA's generic save carries no channel id, so that is what it is recognised
/// by; the C original calls into the lighting save unconditionally.
pub(crate) async fn process_save(report: &ViaReport) {
    if report.output_data[1] == 0 {
        lighting::save().await;
    }
}
