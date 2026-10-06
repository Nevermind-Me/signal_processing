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

use std::time::Duration;
use std::{format, println, vec};

use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};

fn main() -> Result<(), Box<dyn std::error::Error>> {
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
    let mut show_help = false;

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
                        osci_scope.render_chart(
                            frame,
                            main,
                            &vec![&ui_buffer, &ui_buffer_left, &ui_buffer_right],
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
                        vec_scope.render_chart(
                            frame,
                            main,
                            &vec![&ui_buffer_left, &ui_buffer_right],
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
                        spec_scope.render_chart(
                            frame,
                            main,
                            &vec![&ui_buffer, &ui_buffer_left, &ui_buffer_right],
                        );
                    }
                    _ => {}
                }

                // --- Render Overlay Popup if active ---
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

            // --- Key Polling & Handling ---
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
