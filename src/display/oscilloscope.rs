use crate::data_structure::ChartBuffer;
use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Stylize},
    symbols::Marker,
    widgets::{Axis, Chart, Dataset, GraphType},
};
use std::{format, vec};

pub struct Oscilloscope {
    triggering: bool,
    left_trigger: bool,
    falling_edge: bool,
    threshold: f64,
    depth: u32,
    peaks: bool,
    mode: u8,
    scale: f64,
}

impl Default for Oscilloscope {
    fn default() -> Self {
        Self {
            triggering: false,
            left_trigger: true,
            falling_edge: false,
            threshold: 0.0,
            depth: 1,
            peaks: true,
            mode: 0,
            scale: 1.5,
        }
    }
}

impl Oscilloscope {
    pub fn header(&self) -> String {
        if self.triggering {
            format!(
                "Trigger{}: {} {:.2}{}",
                if self.mode == 1 {
                    if self.left_trigger { " [0]" } else { " [1]" }
                } else {
                    ""
                },
                if self.falling_edge { "v" } else { "^" },
                self.threshold,
                if self.depth > 1 {
                    format!(" (depth: {})", self.depth)
                } else {
                    "".into()
                },
            )
        } else {
            "Trigger: Live".into()
        }
    }

    pub fn handle_key(&mut self, code: KeyCode, modifiers: KeyModifiers) -> bool {
        let magnitude = match modifiers {
            KeyModifiers::SHIFT => 10.0,
            KeyModifiers::CONTROL => 5.0,
            KeyModifiers::ALT => 0.2,
            _ => 1.0,
        };

        match code {
            KeyCode::Char('q') => return true, // Signal to quit
            KeyCode::Char('t') => self.triggering = !self.triggering,
            KeyCode::Char('e') => self.falling_edge = !self.falling_edge,
            KeyCode::Char('p') => self.peaks = !self.peaks,
            KeyCode::Char('i') => update_value_f(&mut self.scale, 0.05, magnitude, 0.01..40.0),
            KeyCode::Char('d') => update_value_f(&mut self.scale, -0.05, magnitude, 0.01..40.0),
            KeyCode::Char('I') => update_value_f(&mut self.scale, 0.5, 1.0, 0.01..40.0),
            KeyCode::Char('D') => update_value_f(&mut self.scale, -0.5, 1.0, 0.01..40.0),
            KeyCode::Up => update_value_f(
                &mut self.threshold,
                0.05,
                magnitude,
                -self.scale..self.scale,
            ),
            KeyCode::Down => update_value_f(
                &mut self.threshold,
                -0.05,
                magnitude,
                -self.scale..self.scale,
            ),
            KeyCode::Right => update_value_i(&mut self.depth, true, 1, magnitude, 1..65535),
            KeyCode::Left => update_value_i(&mut self.depth, false, 1, magnitude, 1..65535),
            KeyCode::Esc => {
                self.triggering = false;
                self.mode = 0
            }
            KeyCode::Char('m') => self.mode = (self.mode + 1) % 4,
            KeyCode::Char('c') => self.left_trigger = self.left_trigger ^ true,
            _ => {}
        }
        false
    }

    pub fn render_chart(
        &self,
        frame: &mut Frame,
        area: Rect,
        buffer_vec: &Vec<&ChartBuffer>,
        sample_rate: f64,
    ) {
        let slice_combined = buffer_vec[0].as_slice();
        let slice_left = buffer_vec[1].as_slice();
        let slice_right = buffer_vec[2].as_slice();

        let capacity_f64 = buffer_vec[0].capacity() as f64;

        let mut trigger_offset = 0;
        let depth = self.depth.max(1) as usize;

        if self.triggering {
            let slice;
            if self.mode == 0 {
                slice = slice_combined;
            } else if (self.left_trigger && self.mode == 1) || self.mode == 2 {
                slice = slice_left;
            } else {
                slice = slice_right;
            }
            for i in 0..slice.len() {
                if triggered(slice, i, self.threshold, depth, self.falling_edge) {
                    break;
                }
                trigger_offset += 1;
            }
        }

        let mut min_sample = 0.0f64;
        let mut max_sample = 0.0f64;

        let mut datasets = Vec::new();

        let zero_line = vec![(0.0, 0.0), (capacity_f64, 0.0)];
        datasets.push(
            Dataset::default()
                .marker(Marker::Braille)
                .graph_type(GraphType::Line)
                .style(Color::DarkGray)
                .data(&zero_line),
        );

        let trigger_pt = vec![(0.0, self.threshold), (capacity_f64, self.threshold)];
        if self.triggering {
            datasets.push(
                Dataset::default()
                    .name("T")
                    .marker(Marker::Braille)
                    .graph_type(GraphType::Line)
                    .style(Color::Yellow)
                    .data(&trigger_pt),
            );
        }

        let depth_line = vec![(depth as f64, -1.5), (depth as f64, 1.5)];
        if depth > 1 && self.triggering {
            datasets.push(
                Dataset::default()
                    .marker(Marker::Braille)
                    .graph_type(GraphType::Line)
                    .style(Color::Magenta)
                    .data(&depth_line),
            );
        }

        let signal_points: Vec<(f64, f64)>;
        let signal_points_left: Vec<(f64, f64)>;
        let signal_points_right: Vec<(f64, f64)>;
        match self.mode {
            0 => {
                signal_points = slice_combined
                    .iter()
                    .enumerate()
                    .map(|(i, &sample)| {
                        if sample < min_sample {
                            min_sample = sample;
                        }
                        if sample > max_sample {
                            max_sample = sample;
                        }
                        (i, sample)
                    })
                    .filter(|&(i, _)| i >= trigger_offset)
                    .map(|(i, sample)| ((i - trigger_offset) as f64, sample))
                    .collect();
                datasets.push(
                    Dataset::default()
                        .name("Channel 0 & 1")
                        .marker(Marker::Braille)
                        .graph_type(GraphType::Line)
                        .style(Color::Cyan)
                        .data(&signal_points),
                );
            }
            1 => {
                signal_points_left = slice_left
                    .iter()
                    .enumerate()
                    .map(|(i, &sample)| {
                        if sample < min_sample {
                            min_sample = sample;
                        }
                        if sample > max_sample {
                            max_sample = sample;
                        }
                        (i, sample)
                    })
                    .filter(|&(i, _)| i >= trigger_offset)
                    .map(|(i, sample)| ((i - trigger_offset) as f64, sample))
                    .collect();
                signal_points_right = slice_right
                    .iter()
                    .enumerate()
                    .map(|(i, &sample)| {
                        if sample < min_sample {
                            min_sample = sample;
                        }
                        if sample > max_sample {
                            max_sample = sample;
                        }
                        (i, sample)
                    })
                    .filter(|&(i, _)| i >= trigger_offset)
                    .map(|(i, sample)| ((i - trigger_offset) as f64, sample))
                    .collect();
                datasets.push(
                    Dataset::default()
                        .name("Channel 0 (Left)")
                        .marker(Marker::Braille)
                        .graph_type(GraphType::Line)
                        .style(Color::Blue)
                        .data(&signal_points_left),
                );
                datasets.push(
                    Dataset::default()
                        .name("Channel 1 (Right)")
                        .marker(Marker::Braille)
                        .graph_type(GraphType::Line)
                        .style(Color::Red)
                        .data(&signal_points_right),
                );
            }
            2 => {
                signal_points = slice_left
                    .iter()
                    .enumerate()
                    .map(|(i, &sample)| {
                        if sample < min_sample {
                            min_sample = sample;
                        }
                        if sample > max_sample {
                            max_sample = sample;
                        }
                        (i, sample)
                    })
                    .filter(|&(i, _)| i >= trigger_offset)
                    .map(|(i, sample)| ((i - trigger_offset) as f64, sample))
                    .collect();
                datasets.push(
                    Dataset::default()
                        .name("Channel 0 (Left)")
                        .marker(Marker::Braille)
                        .graph_type(GraphType::Line)
                        .style(Color::Blue)
                        .data(&signal_points),
                );
            }
            3 => {
                signal_points = slice_right
                    .iter()
                    .enumerate()
                    .map(|(i, &sample)| {
                        if sample < min_sample {
                            min_sample = sample;
                        }
                        if sample > max_sample {
                            max_sample = sample;
                        }
                        (i, sample)
                    })
                    .filter(|&(i, _)| i >= trigger_offset)
                    .map(|(i, sample)| ((i - trigger_offset) as f64, sample))
                    .collect();
                datasets.push(
                    Dataset::default()
                        .name("Channel 1 (Right)")
                        .marker(Marker::Braille)
                        .graph_type(GraphType::Line)
                        .style(Color::Red)
                        .data(&signal_points),
                );
            }
            _ => {}
        }

        let peak_bar = vec![(0.0, min_sample), (0.0, max_sample)];
        if self.peaks {
            datasets.push(
                Dataset::default()
                    .marker(Marker::Dot)
                    .graph_type(GraphType::Scatter)
                    .style(Color::Magenta)
                    .data(&peak_bar),
            );
        }

        let x_axis = Axis::default()
            .title("Time".blue())
            .bounds([0.0, capacity_f64])
            .labels([format!("-{:.2}s", capacity_f64 / sample_rate), "0s".into()]);

        let y_axis = Axis::default()
            .title("Amplitude".blue())
            .bounds([-self.scale, self.scale])
            .labels([
                format!("-{:.2}", self.scale),
                "0.00".into(),
                format!("{:.2}", self.scale),
            ]);

        let chart = Chart::new(datasets).x_axis(x_axis).y_axis(y_axis);
        frame.render_widget(chart, area);
    }
}

fn update_value_f(val: &mut f64, delta: f64, mag: f64, range: std::ops::Range<f64>) {
    *val = (*val + delta * mag).clamp(range.start, range.end);
}

fn update_value_i(val: &mut u32, add: bool, amount: u32, mag: f64, range: std::ops::Range<u32>) {
    let delta = (amount as f64 * mag) as u32;
    if add {
        *val = val.saturating_add(delta).clamp(range.start, range.end);
    } else {
        *val = val.saturating_sub(delta).clamp(range.start, range.end);
    }
}

fn triggered(data: &[f64], index: usize, threshold: f64, depth: usize, falling_edge: bool) -> bool {
    if data.len() < index + 1 + depth {
        return false;
    }

    if falling_edge {
        if data[index] >= threshold {
            for i in 1..=depth {
                if data[index + i] >= threshold {
                    return false;
                }
            }
            true
        } else {
            false
        }
    } else if data[index] <= threshold {
        for i in 1..=depth {
            if data[index + i] <= threshold {
                return false;
            }
        }
        true
    } else {
        false
    }
}
