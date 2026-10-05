use crate::ChartBuffer;

use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style, Stylize},
    symbols::Marker,
    text::Span,
    widgets::{Axis, Chart, Dataset, GraphType},
};
use realfft::{RealFftPlanner, num_complex::Complex};

use std::{f64::consts::PI, format, vec};

pub struct Spectroscope {
    sample_rate: u32,
    sample_len: usize,
    fft_planner: RealFftPlanner<f64>,
    window: bool,
    log_y: bool,
    log_x: bool,
    grid_lines: usize,
    phase: bool,
}

/* TODO:
 * - die ganzen render_..._chart refactoren als class function mit self und dann auch umbennenen
 * als render_chart ez möglich
 * - helper pop up updaten
 */

impl Spectroscope {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            sample_rate: sample_rate,
            sample_len: 2048,
            fft_planner: RealFftPlanner::new(),
            window: true,
            log_y: true,
            log_x: true,
            grid_lines: 4,
            phase: false,
        }
    }

    pub fn header(&self) -> String {
        format!(
            "resolution: {:.2}Hz, window: [{}], log_x: [{}], log_y: [{}]",
            self.sample_rate as f64 / self.sample_len as f64,
            if self.window { "X" } else { " " },
            if self.log_x { "X" } else { " " },
            if self.log_y { "X" } else { " " }
        )
    }

    pub fn handle_key(&mut self, code: KeyCode, modifiers: KeyModifiers) -> bool {
        let magnitude = match modifiers {
            KeyModifiers::CONTROL => 5.0,
            KeyModifiers::ALT => 0.2,
            _ => 1.0,
        };

        match code {
            KeyCode::Char('q') => return true, // Signal to quit
            KeyCode::Char('w') => self.window = self.window ^ true,
            KeyCode::Char('y') => self.log_y = self.log_y ^ true,
            KeyCode::Char('x') => self.log_x = self.log_x ^ true,
            KeyCode::Char('m') => self.phase = self.phase ^ true,
            KeyCode::Char('g') => update_value_i(&mut self.grid_lines, false, 1, 1.0, 2..16),
            KeyCode::Char('G') => update_value_i(&mut self.grid_lines, true, 1, 1.0, 2..16),
            KeyCode::Char('p') => update_value_i(
                &mut self.sample_len,
                true,
                10,
                magnitude,
                64..(self.sample_rate - 1) as usize,
            ),
            KeyCode::Char('o') => update_value_i(
                &mut self.sample_len,
                false,
                10,
                magnitude,
                64..(self.sample_rate - 1) as usize,
            ),
            KeyCode::Char('P') => update_value_i(
                &mut self.sample_len,
                true,
                100,
                1.0,
                64..(self.sample_rate - 1) as usize,
            ),
            KeyCode::Char('O') => update_value_i(
                &mut self.sample_len,
                false,
                100,
                1.0,
                64..(self.sample_rate - 1) as usize,
            ),
            KeyCode::Esc => {
                self.sample_len = 2048;
                self.window = true;
                self.log_x = true;
                self.log_y = true;
                self.phase = false;
            }
            _ => {}
        }
        false
    }
}

fn update_value_i(
    val: &mut usize,
    add: bool,
    amount: usize,
    mag: f64,
    range: std::ops::Range<usize>,
) {
    let delta = (amount as f64 * mag) as usize;
    if add {
        *val = val.saturating_add(delta).clamp(range.start, range.end);
    } else {
        *val = val.saturating_sub(delta).clamp(range.start, range.end);
    }
}

fn hamming_window(samples: &[f64]) -> Vec<f64> {
    let mut windowed_samples = Vec::with_capacity(samples.len());
    let period = (samples.len() - 1) as f64;
    let w = |n: usize| 0.54 - 0.46 * (2.0 * PI * n as f64 / period).cos();
    for (i, sample) in samples.iter().enumerate() {
        windowed_samples.push(sample * w(i))
    }
    windowed_samples
}

fn phase(z: Complex<f64>) -> f64 {
    let r = z.arg();
    if r.is_normal() { r } else { 0. }
}

fn phase_diff(phase_l: f64, phase_r: f64) -> f64 {
    // Computes difference wrapped into the range [-PI, +PI]
    f64::atan2((phase_l - phase_r).sin(), (phase_l - phase_r).cos())
}

pub fn render_spectroscope_chart(
    frame: &mut Frame,
    area: Rect,
    buffer_vec: &Vec<&ChartBuffer>,
    scope: &mut Spectroscope,
) {
    if scope.phase {
        create_phase_shift_chart(frame, area, buffer_vec[1], buffer_vec[2], scope);
    } else {
        create_spectroscope_chart(frame, area, buffer_vec[0], scope);
    }
}

fn create_phase_shift_chart(
    frame: &mut Frame,
    area: Rect,
    buffer_left: &ChartBuffer,
    buffer_right: &ChartBuffer,
    scope: &mut Spectroscope,
) {
    let slice_left = buffer_left.as_slice();
    let slice_right = buffer_right.as_slice();
    let len = slice_left.len();

    if len < scope.sample_len {
        scope.sample_len = len;
    }

    let start_idx = len.saturating_sub(scope.sample_len);
    let mut samples_left = if scope.window {
        hamming_window(&slice_left[start_idx..len])
    } else {
        slice_left[start_idx..len].to_vec()
    };
    let mut samples_right = if scope.window {
        hamming_window(&slice_right[start_idx..len])
    } else {
        slice_right[start_idx..len].to_vec()
    };
    samples_left.reverse();
    samples_right.reverse();

    let resolution = scope.sample_rate as f64 / scope.sample_len as f64;

    let output_len = scope.sample_len / 2 + 1;
    let mut result_left = vec![Complex::default(); output_len];
    let mut result_right = vec![Complex::default(); output_len];

    let fft = scope
        .fft_planner
        .plan_fft_forward(scope.sample_len as usize);

    let _ = fft.process(&mut samples_left, &mut result_left);
    let _ = fft.process(&mut samples_right, &mut result_right);

    let result_left = &result_left[1..];
    let result_right = &result_right[1..];

    let signal_points: Vec<(f64, f64)> = result_left
        .iter()
        .zip(result_right.iter())
        .enumerate()
        .map(|(i, (&z_l, &z_r))| {
            let freq = if scope.log_x {
                ((i + 1) as f64 * resolution).ln() // resolution capped to a minimum of 1 Hz
            } else {
                (i + 1) as f64 * resolution
            };
            (freq, phase_diff(phase(z_l), phase(z_r)))
        })
        .collect();

    let (x_axis, mut datasets) = create_frequency_axis(
        scope.log_x,
        resolution,
        output_len as f64 * resolution,
        scope.grid_lines,
        [-PI, PI],
    );

    let y_axis = Axis::default()
        .title("Angle".blue())
        .bounds([-PI, PI])
        .labels(["-90°", "90°"]);

    let (x_min, x_max) = if scope.log_x {
        (
            resolution.max(1.0).ln(),
            (output_len as f64 * resolution).ln(),
        )
    } else {
        (resolution, output_len as f64 * resolution)
    };
    let middle_line = vec![(x_min, 0.), (x_max, 0.)];
    datasets.push(
        Dataset::default()
            .marker(Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Color::DarkGray)
            .data(&middle_line),
    );

    datasets.push(
        Dataset::default()
            .name("Channel 0 & 1")
            .marker(Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Color::Cyan)
            .data(&signal_points),
    );

    let chart = Chart::new(datasets).x_axis(x_axis).y_axis(y_axis);
    frame.render_widget(chart, area);
}

fn create_spectroscope_chart(
    frame: &mut Frame,
    area: Rect,
    buffer: &ChartBuffer,
    scope: &mut Spectroscope,
) {
    let slice_combined = buffer.as_slice();
    let len = slice_combined.len();

    if len < scope.sample_len {
        scope.sample_len = len;
    }

    let start_idx = len.saturating_sub(scope.sample_len);
    let mut samples = if scope.window {
        hamming_window(&slice_combined[start_idx..len])
    } else {
        slice_combined[start_idx..len].to_vec()
    };
    samples.reverse();

    let resolution = scope.sample_rate as f64 / scope.sample_len as f64;

    let output_len = scope.sample_len / 2 + 1;
    let mut result = vec![Complex::default(); output_len];

    let fft = scope
        .fft_planner
        .plan_fft_forward(scope.sample_len as usize);

    let _ = fft.process(&mut samples, &mut result);

    let dc_val = if scope.log_y {
        result[0].norm().ln()
    } else {
        result[0].norm()
    };

    let result = &result[1..];

    let mut min_val = dc_val;
    let mut max_val = dc_val;

    let signal_points: Vec<(f64, f64)> = result
        .iter()
        .enumerate()
        .map(|(i, &z)| {
            let freq = if scope.log_x {
                ((i + 1) as f64 * resolution).ln() // resolution capped to a minimum of 1 Hz
            } else {
                (i + 1) as f64 * resolution
            };
            let val = if scope.log_y {
                z.norm().ln() // no need to worry about -inf -> ratatui clips it to the lowest y bound
            } else {
                z.norm()
            };

            if val > max_val {
                max_val = val;
            }
            if val < min_val {
                min_val = val;
            }

            (freq, val)
        })
        .collect();

    let (x_axis, mut datasets) = create_frequency_axis(
        scope.log_x,
        resolution,
        output_len as f64 * resolution,
        scope.grid_lines,
        [min_val, max_val],
    );

    let y_axis = Axis::default()
        .title("Magnitude".blue())
        .bounds([min_val, max_val]); // this normalizes the graph, so no need to normalize the
    // values itself

    datasets.push(
        Dataset::default()
            .name("Channel 0 & 1")
            .marker(Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Color::Cyan)
            .data(&signal_points),
    );

    let dc_point = vec![(
        if scope.log_x {
            resolution.ln()
        } else {
            resolution
        },
        dc_val,
    )];
    datasets.push(
        Dataset::default()
            .name("DC")
            .marker(Marker::Dot)
            .graph_type(GraphType::Scatter)
            .style(Color::Magenta)
            .data(&dc_point),
    );

    let chart = Chart::new(datasets).x_axis(x_axis).y_axis(y_axis);
    frame.render_widget(chart, area);
}

fn format_freq(freq_hz: f64) -> String {
    if freq_hz >= 1000.0 {
        let k_val = freq_hz / 1000.0;
        if k_val.fract() == 0.0 {
            format!("{:.0}k", k_val)
        } else {
            format!("{:.1}k", k_val)
        }
    } else {
        format!("{:.0}", freq_hz)
    }
}

// Generates an Axis with labels and matching grid line datasets
fn create_frequency_axis<'a>(
    is_log: bool,
    f_min: f64,
    f_max: f64,
    num_ticks: usize,
    y_bounds: [f64; 2],
) -> (Axis<'a>, Vec<Dataset<'a>>) {
    let mut labels = Vec::with_capacity(num_ticks);
    let mut grid_datasets = Vec::with_capacity(num_ticks);

    let (x_min, x_max) = if is_log {
        (f_min.max(1.0).ln(), f_max.ln())
    } else {
        (f_min, f_max)
    };

    let step = (x_max - x_min) / (num_ticks - 1) as f64;

    for i in 0..num_ticks {
        let x_pos = x_min + (i as f64 * step);

        // Calculate original frequency in Hz for the label text
        let freq_hz = if is_log { x_pos.exp() } else { x_pos };
        labels.push(Span::raw(format_freq(freq_hz)));

        // Create a vertical 2-point line at x_pos
        // Store point array on heap via Box::leak to keep reference valid for Dataset
        let points: &'static [(f64, f64); 2] =
            Box::leak(Box::new([(x_pos, y_bounds[0]), (x_pos, y_bounds[1])]));

        grid_datasets.push(
            Dataset::default()
                .marker(Marker::Braille)
                .graph_type(ratatui::widgets::GraphType::Line)
                .style(Style::default().fg(Color::DarkGray))
                .data(points),
        );
    }

    let axis = Axis::default()
        .title("Hz".blue())
        .bounds([x_min, x_max])
        .labels(labels);

    (axis, grid_datasets)
}
