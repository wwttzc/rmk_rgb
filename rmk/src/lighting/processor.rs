//! The task that drives the chain: QMK's `rgb_matrix_task` plus the driver's
//! flush.

use embassy_time::{Duration, Instant, Ticker};
use smart_leds_trait::{RGB, SmartLedsWrite};

use crate::core_traits::Runnable;
use crate::lighting::color::Rgb;
use crate::lighting::{self, LightingConfig, MAX_LEDS};

/// Renders lighting frames into a board's LED chain.
///
/// The generated main creates one of these per board and runs it, so the
/// lighting task lives alongside the keyboard, matrix and host tasks.
pub struct LightingProcessor<L> {
    leds: L,
    cfg: &'static LightingConfig,
    /// The frame handed to the driver, and the one last sent. A frame that did
    /// not change is not sent again, which keeps a static effect off the wire.
    frame: [Rgb; MAX_LEDS],
    shown: [Rgb; MAX_LEDS],
}

impl<L> LightingProcessor<L> {
    pub const fn new(leds: L, cfg: &'static LightingConfig) -> Self {
        Self {
            leds,
            cfg,
            frame: [Rgb::BLACK; MAX_LEDS],
            shown: [Rgb::BLACK; MAX_LEDS],
        }
    }
}

impl<L: SmartLedsWrite<Color = RGB<u8>>> Runnable for LightingProcessor<L> {
    async fn run(&mut self) -> ! {
        lighting::start(self.cfg).await;
        // The storage task runs alongside this one, so this read is answered.
        lighting::restore().await;

        let count = self.cfg.points.len();
        let mut ticker = Ticker::every(Duration::from_millis(self.cfg.frame_ms.max(1) as u64));
        loop {
            ticker.next().await;
            let now_ms = Instant::now().as_millis() as u32;
            lighting::render(&mut self.frame[..count], now_ms).await;

            if self.frame[..count] != self.shown[..count] {
                let frame = &self.frame[..count];
                let colours = frame.iter().map(|c| RGB { r: c.r, g: c.g, b: c.b });
                if self.leds.write(colours).is_ok() {
                    self.shown[..count].copy_from_slice(frame);
                }
            }
        }
    }
}
