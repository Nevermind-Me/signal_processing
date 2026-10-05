use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

/// RAII Guard to ensure terminal raw mode is disabled on exit
struct RawModeGuard;
impl RawModeGuard {
    fn new() -> Self {
        let _ = crossterm::terminal::enable_raw_mode();
        Self
    }
}
impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = crossterm::terminal::disable_raw_mode();
    }
}

/// Generic 2nd-Order Biquad Audio Filter
#[derive(Clone, Copy)]
struct BiquadFilter {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl BiquadFilter {
    /// Create a Low-Pass Filter (cuts highs -> underwater/muffled sound)
    fn lowpass(sample_rate: f32, cutoff_hz: f32) -> Self {
        let omega = 2.0 * std::f32::consts::PI * cutoff_hz / sample_rate;
        let alpha = omega.sin() / (2.0 * 0.7071);
        let cos_w = omega.cos();

        let b0 = (1.0 - cos_w) / 2.0;
        let b1 = 1.0 - cos_w;
        let b2 = (1.0 - cos_w) / 2.0;
        let a0 = 1.0 + alpha;
        let a1 = -2.0 * cos_w;
        let a2 = 1.0 - alpha;

        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }

    /// Create a High-Pass Filter (cuts lows -> thin/phone-speaker sound)
    fn highpass(sample_rate: f32, cutoff_hz: f32) -> Self {
        let omega = 2.0 * std::f32::consts::PI * cutoff_hz / sample_rate;
        let alpha = omega.sin() / (2.0 * 0.7071);
        let cos_w = omega.cos();

        let b0 = (1.0 + cos_w) / 2.0;
        let b1 = -(1.0 + cos_w);
        let b2 = (1.0 + cos_w) / 2.0;
        let a0 = 1.0 + alpha;
        let a1 = -2.0 * cos_w;
        let a2 = 1.0 - alpha;

        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }

    fn process(&mut self, sample: f32) -> f32 {
        let out = self.b0 * sample + self.b1 * self.x1 + self.b2 * self.x2
            - self.a1 * self.y1
            - self.a2 * self.y2;

        self.x2 = self.x1;
        self.x1 = sample;
        self.y2 = self.y1;
        self.y1 = out;

        out
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut audio_handle = AudioHandling::init_audio_handling()?;

    // Filter cutoffs
    let lpf_cutoff = 500.0; // 500 Hz cutoff for Low-Pass
    let hpf_cutoff = 2000.0; // 2000 Hz cutoff for High-Pass

    let mut lpf_filters = vec![
        BiquadFilter::lowpass(audio_handle.sample_rate, lpf_cutoff);
        audio_handle.num_channels
    ];
    let mut hpf_filters = vec![
        BiquadFilter::highpass(audio_handle.sample_rate, hpf_cutoff);
        audio_handle.num_channels
    ];

    // Filter Mode: 0 = LowPass, 1 = HighPass, 2 = Bypass
    let filter_mode = Arc::new(AtomicU8::new(0));
    let mode_clone = filter_mode.clone();

    let func = move |s: f32, i: usize| match mode_clone.load(Ordering::Relaxed) {
        0 => lpf_filters[i].process(s),
        1 => hpf_filters[i].process(s),
        _ => s, // Bypass (raw audio)
    };

    audio_handle.start_audio_streams(func)?;

    // Enable Terminal Raw Mode for instant key handling
    let _raw_guard = RawModeGuard::new();

    print!("\x1B[2J\x1B[1;1H"); // Clear terminal screen
    println!("=== Live Audio Filter Controller ===");
    println!("Press [L] -> Low-Pass Filter (500 Hz - Muffled/Underwater)");
    println!("Press [H] -> High-Pass Filter (2000 Hz - Thin/Radio)");
    println!("Press [B] -> Bypass (Original Untouched Audio)");
    println!("Press [Q] or Ctrl+C -> Exit\n");
    println!("Current State: [Low-Pass Filter Active]");

    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || {
        r.store(false, Ordering::SeqCst);
    })?;

    // Keyboard Loop
    while running.load(Ordering::SeqCst) {
        if event::poll(std::time::Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('l') | KeyCode::Char('L') => {
                        filter_mode.store(0, Ordering::Relaxed);
                        print!("\r\x1B[KCurrent State: [Low-Pass Filter Active]");
                    }
                    KeyCode::Char('h') | KeyCode::Char('H') => {
                        filter_mode.store(1, Ordering::Relaxed);
                        print!("\r\x1B[KCurrent State: [High-Pass Filter Active]");
                    }
                    KeyCode::Char('b') | KeyCode::Char('B') => {
                        filter_mode.store(2, Ordering::Relaxed);
                        print!("\r\x1B[KCurrent State: [Bypass - Raw Audio]");
                    }
                    KeyCode::Char('q') | KeyCode::Char('Q') => {
                        running.store(false, Ordering::SeqCst);
                    }
                    _ => {}
                }
            }
        }
    }

    println!("\nExiting...");
    Ok(())
}
