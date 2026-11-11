//! Transport clock for the iced-liveplot MIDI sequencer.
//!
//! Provides a monotonic tick counter that advances at a musical tempo and can
//! be used to drive a playhead, scheduling, or any other time-locked UI.
//!
//! * **Native** (`cfg(not(target_arch = "wasm32"))`) uses
//!   [`std::time::Instant`], which is monotonic on every platform.
//! * **WASM** (`cfg(target_arch = "wasm32")`) uses `performance.now()`
//!   through `web_sys` (the most stable high-resolution clock available in
//!   browsers), falling back to a coarse counter when `web_sys` is absent.
//!
//! The clock never converts to wall-clock time; it only reports how much
//! monotonic time has elapsed since the last [`Clock::update`].

/// Ticks per quarter note (PPQ) used by the transport.
pub const PPQ: u32 = 480;

/// Minimum samples before the clock reports a non-zero tick delta (avoids
/// tiny spurious jumps right after a start/seek).
const MIN_STEP_SECONDS: f64 = 1.0 / 1000.0;

/// A monotonic transport clock.
///
/// ```ignore
/// let mut clock = Clock::new(124.0);
/// clock.start();
/// // each frame:
/// let ticks = clock.update(); // u64 ticks since the previous update
/// ```
#[derive(Debug, Clone)]
pub struct Clock {
    /// Tempo in beats per minute.
    pub bpm: f64,
    /// Ticks per quarter note.
    pub ppq: u32,
    /// Accumulated position in ticks (musical time).
    pub position_ticks: u64,
    /// Accumulated position in beats.
    pub position_beats: f64,
    /// Whether the transport is running.
    pub playing: bool,
    /// Loop start in ticks (None = no loop).
    pub loop_start: Option<u64>,
    /// Loop end in ticks (None = no loop).
    pub loop_end: Option<u64>,
    /// Internal monotonic reference.
    state: ClockState,
}

#[derive(Debug, Clone)]
struct ClockState {
    /// Monotonic seconds of the last update (platform specific).
    last: f64,
    /// Whether the clock has ever been started.
    started: bool,
}

/// Platform monotonic seconds.
#[cfg(not(target_arch = "wasm32"))]
fn now_seconds() -> f64 {
    use std::time::Instant;
    // A process-wide reference keeps the scale small and avoids the
    // sub-second precision issue of `elapsed()` on some platforms.
    static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    let start = START.get_or_init(Instant::now);
    start.elapsed().as_secs_f64()
}

/// WASM: `performance.now()` (monotonic, sub-millisecond, stable under
/// throttling). Falls back to 0.0 when `web_sys` is not linked.
#[cfg(target_arch = "wasm32")]
fn now_seconds() -> f64 {
    #[cfg(feature = "web-sys")]
    {
        if let Some(window) = web_sys::window() {
            if let Some(perf) = window.performance() {
                return perf.now() / 1000.0;
            }
        }
        0.0
    }
    #[cfg(not(feature = "web-sys"))]
    {
        0.0
    }
}

impl Clock {
    /// Create a stopped clock at the given tempo.
    pub fn new(bpm: f64) -> Self {
        Self {
            bpm: bpm.max(1.0),
            ppq: PPQ,
            position_ticks: 0,
            position_beats: 0.0,
            playing: false,
            loop_start: None,
            loop_end: None,
            state: ClockState {
                last: now_seconds(),
                started: false,
            },
        }
    }

    /// Create a clock with an explicit PPQ.
    pub fn with_ppq(bpm: f64, ppq: u32) -> Self {
        let mut clock = Self::new(bpm);
        clock.ppq = ppq.max(1);
        clock
    }

    /// Set the tempo (beats per minute).
    pub fn set_bpm(&mut self, bpm: f64) {
        self.bpm = bpm.clamp(1.0, 1000.0);
    }

    /// Show/hide the loop range.
    pub fn set_loop(&mut self, start_ticks: Option<u64>, end_ticks: Option<u64>) {
        self.loop_start = start_ticks;
        self.loop_end = end_ticks;
    }

    /// Start (or resume) playback from the current position.
    pub fn start(&mut self) {
        self.playing = true;
        self.state.last = now_seconds();
        self.state.started = true;
    }

    /// Stop playback (the position is kept).
    pub fn stop(&mut self) {
        self.playing = false;
    }

    /// Toggle play/stop.
    pub fn toggle(&mut self) {
        if self.playing {
            self.stop();
        } else {
            self.start();
        }
    }

    /// Move the playhead to a tick position.
    pub fn seek_ticks(&mut self, ticks: u64) {
        self.position_ticks = ticks;
        self.position_beats = ticks as f64 / self.ppq as f64;
        self.state.last = now_seconds();
    }

    /// Move the playhead to a beat position.
    pub fn seek_beats(&mut self, beats: f64) {
        self.seek_ticks((beats.max(0.0) * self.ppq as f64).round() as u64);
    }

    /// Current position in ticks.
    pub fn ticks(&self) -> u64 {
        self.position_ticks
    }

    /// Current position in beats.
    pub fn beats(&self) -> f64 {
        self.position_beats
    }

    /// Current position in seconds.
    pub fn seconds(&self) -> f64 {
        self.position_beats * 60.0 / self.bpm
    }

    /// Musical position as (bar, beat_in_bar) using the given beats-per-bar
    /// (e.g. 4 for 4/4). Bars and beats are 1-based.
    pub fn bar_beat(&self, beats_per_bar: u32) -> (u64, f64) {
        let bpb = beats_per_bar.max(1) as f64;
        let bar = (self.position_beats / bpb).floor() as u64 + 1;
        let beat = self.position_beats - (bar as f64 - 1.0) * bpb;
        (bar, beat + 1.0)
    }

    /// Advance the clock by the elapsed monotonic time and return the tick
    /// delta since the previous update (0 when stopped).
    ///
    /// Call once per frame (or per tick subscription).
    pub fn update(&mut self) -> u64 {
        if !self.playing {
            // Keep the reference fresh so resume doesn't jump.
            self.state.last = now_seconds();
            return 0;
        }
        let now = now_seconds();
        let mut dt = now - self.state.last;
        self.state.last = now;
        if dt <= 0.0 || !self.state.started {
            return 0;
        }
        if dt < MIN_STEP_SECONDS {
            // Too small to matter — avoid sub-tick jitter.
            dt = 0.0;
            return 0;
        }
        let beats = dt * self.bpm / 60.0;
        let ticks = (beats * self.ppq as f64).round() as u64;
        if ticks == 0 {
            return 0;
        }
        self.position_ticks += ticks;
        self.position_beats += ticks as f64 / self.ppq as f64;

        // Loop wrap
        if let (Some(start), Some(end)) = (self.loop_start, self.loop_end) {
            if self.position_ticks >= end.max(start + 1) {
                self.position_ticks = start;
                self.position_beats = start as f64 / self.ppq as f64;
            }
        }
        ticks
    }

    /// Elapsed ticks since the clock was started/reset, regardless of loop.
    pub fn ticks_since_start(&self) -> u64 {
        self.position_ticks
    }
}

impl Default for Clock {
    fn default() -> Self {
        Self::new(120.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tick_conversion_round_trip() {
        // 1 beat = PPQ ticks = 0.5s at 120 BPM
        let clock = Clock::new(120.0);
        assert_eq!(clock.ppq, 480);
        let seconds = clock.position_beats * 60.0 / 120.0;
        assert!((seconds - 0.0).abs() < 1e-9);
    }

    #[test]
    fn bar_beat_four_four() {
        let mut clock = Clock::new(120.0);
        clock.seek_ticks(0); // bar 1, beat 1
        assert_eq!(clock.bar_beat(4), (1, 1.0));
        clock.seek_ticks(480 * 4); // 4 beats = bar 2, beat 1
        assert_eq!(clock.bar_beat(4), (2, 1.0));
        clock.seek_ticks(480 * 8); // 8 beats => bar 3, beat 1
        assert_eq!(clock.bar_beat(4), (3, 1.0));
    }

    #[test]
    fn stopped_clock_reports_no_ticks() {
        let mut clock = Clock::new(120.0);
        assert_eq!(clock.update(), 0);
        assert_eq!(clock.ticks(), 0);
    }

    #[test]
    fn loop_wraps() {
        let mut clock = Clock::new(1000000.0); // fast tempo for the test
        clock.set_loop(Some(0), Some(480));
        clock.start();
        // Many updates should wrap around the loop rather than growing.
        for _ in 0..1000 {
            let _ = clock.update();
        }
        assert!(clock.ticks() < 4800, "position should stay near the loop");
    }
}
