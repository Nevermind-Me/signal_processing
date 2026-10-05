mod audio_device_handling;
mod data_structure;
mod display;
mod virtual_audio_sink;

use audio_device_handling::*;
use data_structure::*;
use display::oscilloscope::*;
use display::spectroscope::*;
use display::vectorscope::*;

use ringbuf::HeapRb;
use ringbuf::traits::{Consumer, Producer, Split};

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::time::Duration;
use std::{format, print, println, vec};

use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};

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
    let test_fkt = true;
    if test_fkt {
        return test_func();
    }

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

fn test_func() -> Result<(), Box<dyn std::error::Error>> {
    let mut audio_handle = AudioHandling::init_audio_handling()?;
    let sample_rate = audio_handle.sample_rate as f64;
    let capacity = audio_handle.sample_rate as usize;

    println!("Number of Channels: {}", audio_handle.num_channels);

    let ring_buffer_left = HeapRb::<f64>::new(capacity);
    let (mut producer_left, mut consumer_left) = ring_buffer_left.split();

    let ring_buffer_right = HeapRb::<f64>::new(capacity);
    let (mut producer_right, mut consumer_right) = ring_buffer_right.split();

    let func = move |s: f32, _i: usize| {
        //_i = 0 is left channel and _i = 1 is right channel
        if _i == 0 {
            let _ = producer_left.try_push(s as f64);
        } else if _i == 1 {
            let _ = producer_right.try_push(s as f64);
        }
        s
    };

    audio_handle.start_audio_streams(func)?;

    let mut ui_buffer = ChartBuffer::new(capacity);
    let mut ui_buffer_left = ChartBuffer::new(capacity);
    let mut ui_buffer_right = ChartBuffer::new(capacity);
    let mut osci_scope = Oscilloscope::default();
    let mut vec_scope = Vectorscope::default();
    let mut spec_scope = Spectroscope::new(sample_rate as u32);

    let mut mode: u8 = 0;
    let mut show_help = false; // <-- 1. Track popup visibility state

    ratatui::run(|terminal| {
        loop {
            // Drain incoming audio samples into ui_buffer
            while let Some(sample_left) = consumer_left.try_pop() {
                if let Some(sample_right) = consumer_right.try_pop() {
                    ui_buffer.push((sample_left + sample_right) / 2.0);
                    ui_buffer_left.push(sample_left);
                    ui_buffer_right.push(sample_right);
                } else {
                    break;
                }
            }

            // Render current frame
            terminal.draw(|frame| {
                let layout =
                    Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).spacing(1);
                let [top, main] = frame.area().layout(&layout);

                // --- Main UI Rendering ---
                match mode {
                    0 => {
                        let title = Line::from_iter([
                            Span::from("Oscilloscope ").bold(),
                            Span::from(format!("[{}] ", osci_scope.header())).yellow(),
                            Span::from("(Press 'h' for controls)").dark_gray(),
                            Span::from(format!(" (buffer_size: {})", ui_buffer.capacity())).gray(),
                            Span::from(format!(" (sample_rate [per Channel]: {})", sample_rate))
                                .gray(),
                        ]);
                        frame.render_widget(title.centered(), top);
                        render_oscilloscope_chart(
                            frame,
                            main,
                            &vec![&ui_buffer, &ui_buffer_left, &ui_buffer_right],
                            &osci_scope,
                            sample_rate,
                        );
                    }
                    1 => {
                        let title = Line::from_iter([
                            Span::from("Vectorscope ").bold(),
                            Span::from(format!("[{}] ", vec_scope.header())).yellow(),
                            Span::from("(Press 'h' for controls) ").dark_gray(),
                            Span::from(format!(" (buffer_size: {})", ui_buffer.capacity())).gray(),
                            Span::from(format!(" (sample_rate [per Channel]: {})", sample_rate))
                                .gray(),
                        ]);
                        frame.render_widget(title.centered(), top);
                        render_vectorscope_chart(
                            frame,
                            main,
                            &vec![&ui_buffer_left, &ui_buffer_right],
                            &vec_scope,
                        );
                    }
                    2 => {
                        let title = Line::from_iter([
                            Span::from("Spectroscope ").bold(),
                            Span::from(format!("[{}] ", spec_scope.header())).yellow(),
                            Span::from("(Press 'h' for controls)").dark_gray(),
                            Span::from(format!(" (buffer_size: {})", ui_buffer.capacity())).gray(),
                            Span::from(format!(" (sample_rate [per Channel]: {})", sample_rate))
                                .gray(),
                        ]);
                        frame.render_widget(title.centered(), top);
                        render_spectroscope_chart(
                            frame,
                            main,
                            &vec![&ui_buffer, &ui_buffer_left, &ui_buffer_right],
                            &mut spec_scope,
                        );
                    }
                    _ => {}
                }

                // --- 2. Render Overlay Popup if active ---
                if show_help {
                    let popup_area = centered_rect(frame.area(), 50, 60);

                    // Clear background cells underneath popup
                    frame.render_widget(Clear, popup_area);

                    let help_text = vec![
                        Line::from(vec![Span::raw(" General Controls").bold().cyan()]),
                        Line::from("  h       : Toggle Help Overlay"),
                        Line::from("  Esc     : Close Help Overlay"),
                        Line::from(
                            "  n       : Switch Mode (Oscilloscope / Vectorscope / Spectroscope)",
                        ),
                        Line::from("  + / -   : Adjust Buffer Capacity (Hold Alt for 1000x)"),
                        Line::from("  q       : Quit Application"),
                        Line::from(""),
                        Line::from(vec![Span::raw(" Oscilloscope Controls").bold().cyan()]),
                        Line::from("  t       : Toggle Trigger Mode"),
                        Line::from("  e       : Change Trigger Edge"),
                        Line::from("  p       : Toggle Peak Detection"),
                        Line::from("  m       : Change the Mode of the Oscilloscope"),
                        Line::from("  c       : Toggle Left / Right Trigger"),
                        Line::from("  Up / Dn : Adjust Threshold"),
                        Line::from("  <- / -> : Adjust Depth"),
                        Line::from(""),
                        Line::from(vec![Span::raw(" Vectorscope Controls").bold().cyan()]),
                        Line::from("  p / o   : Adjust Vectorscope Gain"),
                        Line::from(""),
                        Line::from(vec![Span::raw(" Spectroscope Controls").bold().cyan()]),
                        Line::from(
                            "  p / o   : Increase / Decrease the Resolution (amount of Samples)",
                        ),
                        Line::from("  g / G   : Decrease / Increase the amount of Grid Lines"),
                        Line::from("  m       : Change the Mode of the Spectroscope"),
                        Line::from("  w       : Toggle Hamming Windowing"),
                        Line::from("  x       : Toggle logarithmic x-Scale"),
                        Line::from("  y       : Toggle logarithmic y-Scale"),
                        Line::from(""),
                        Line::from(vec![Span::raw(" Modifier Keys").bold().cyan()]),
                        Line::from("  Alt     : 0.2x Modifier"),
                        Line::from("  Ctrl    : 5.0x Modifier"),
                        Line::from("  Shift   : 10.0x Modifier"),
                        Line::from(""),
                        Line::from(vec![
                            Span::raw(" Hint: Adjust font-size for finer resolution.").dark_gray(),
                        ]),
                    ];

                    let popup_block = Block::bordered()
                        .title(" Keybindings / Help ")
                        .border_style(Style::default().fg(Color::Yellow));

                    let popup_widget = Paragraph::new(help_text).block(popup_block);

                    frame.render_widget(popup_widget, popup_area);
                }
            })?;

            // --- 3. Key Polling & Handling ---
            if event::poll(Duration::from_millis(12))? {
                if let Event::Key(key) = event::read()? {
                    if key.kind == KeyEventKind::Press {
                        // Toggle or dismiss help overlay
                        if key.code == KeyCode::Char('h') {
                            show_help = !show_help;
                        } else if show_help {
                            // If popup is open, Esc closes it
                            if key.code == KeyCode::Esc {
                                show_help = false;
                            }
                        } else if key.code == KeyCode::Char('n') {
                            mode = (mode + 1) % 3;
                        } else if key.code == KeyCode::Char('+') {
                            let mut modifier: usize = 10;
                            if key.modifiers == KeyModifiers::ALT {
                                modifier = 1000;
                            }
                            let nc = ui_buffer.capacity().saturating_add(modifier);
                            if nc < 4194304 {
                                ui_buffer.change_capacity(nc);
                                ui_buffer_left.change_capacity(nc);
                                ui_buffer_right.change_capacity(nc);
                            }
                        } else if key.code == KeyCode::Char('-') {
                            let mut modifier: usize = 10;
                            if key.modifiers == KeyModifiers::ALT {
                                modifier = 1000;
                            }
                            let nc = ui_buffer.capacity().saturating_sub(modifier);
                            if nc >= 100 {
                                ui_buffer.change_capacity(nc);
                                ui_buffer_left.change_capacity(nc);
                                ui_buffer_right.change_capacity(nc);
                            }
                        } else {
                            match mode {
                                0 => {
                                    if osci_scope.handle_key(key.code, key.modifiers) {
                                        break Ok(());
                                    }
                                }
                                1 => {
                                    if vec_scope.handle_key(key.code, key.modifiers) {
                                        break Ok(());
                                    }
                                }
                                2 => {
                                    if spec_scope.handle_key(key.code, key.modifiers) {
                                        break Ok(());
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
        }
    })
}

/// Helper function to center a popup rectangle relative to the parent frame area
fn centered_rect(area: Rect, percent_x: u16, percent_y: u16) -> Rect {
    let [vertical_area] = Layout::vertical([Constraint::Percentage(percent_y)])
        .flex(Flex::Center)
        .areas(area);

    let [popup_area] = Layout::horizontal([Constraint::Percentage(percent_x)])
        .flex(Flex::Center)
        .areas(vertical_area);

    popup_area
}
