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

pub struct Vectorscope {
    pub gain: f64,
}

impl Default for Vectorscope {
    fn default() -> Self {
        Self { gain: 1.0 }
    }
}

impl Vectorscope {
    pub fn header(&self) -> String {
        format!("gain: {:.2}", self.gain)
    }

    pub fn handle_key(&mut self, code: KeyCode, modifiers: KeyModifiers) -> bool {
        let magnitude = match modifiers {
            KeyModifiers::CONTROL => 5.0,
            KeyModifiers::ALT => 0.2,
            _ => 1.0,
        };

        match code {
            KeyCode::Char('q') => return true, // Signal to quit
            KeyCode::Char('p') => update_value_f(&mut self.gain, 0.1, magnitude, -1000.0..1000.0),
            KeyCode::Char('o') => update_value_f(&mut self.gain, -0.1, magnitude, -1000.0..1000.0),
            KeyCode::Char('P') => update_value_f(&mut self.gain, 0.1, 10.0, -1000.0..1000.0),
            KeyCode::Char('O') => update_value_f(&mut self.gain, -0.1, 10.0, -1000.0..1000.0),
            KeyCode::Esc => self.gain = 1.0,
            _ => {}
        }
        false
    }

    pub fn render_chart(&self, frame: &mut Frame, area: Rect, buffer_vec: &Vec<&ChartBuffer>) {
        let slice_left = buffer_vec[0].as_slice();
        let slice_right = buffer_vec[1].as_slice();

        let mut datasets = Vec::new();

        let signal_points: Vec<(f64, f64)> = slice_left
            .iter()
            .zip(slice_right.iter())
            .map(|(&x, &y)| (x * self.gain, y * self.gain))
            .collect();

        let zero_lines = vec![(-1.5, 0.0), (1.5, 0.0), (0.0, 0.0), (0.0, 1.5), (0.0, -1.5)];
        datasets.push(
            Dataset::default()
                .marker(Marker::Braille)
                .graph_type(GraphType::Line)
                .style(Color::DarkGray)
                .data(&zero_lines),
        );

        datasets.push(
            Dataset::default()
                .name("Channel 0 & 1")
                .marker(Marker::Braille)
                .graph_type(GraphType::Line)
                .style(Color::Cyan)
                .data(&signal_points),
        );

        let x_axis = Axis::default()
            .title("Channel 0 (Left)".blue())
            .bounds([-1.5, 1.5])
            .labels(["-1.5", "0.0", "1.5"]);

        let y_axis = Axis::default()
            .title("Channel 1 (Right)".blue())
            .bounds([-1.5, 1.5])
            .labels(["-1.5", "0.0", "1.5"]);

        let chart = Chart::new(datasets).x_axis(x_axis).y_axis(y_axis);
        frame.render_widget(chart, area);
    }
}

fn update_value_f(val: &mut f64, delta: f64, mag: f64, range: std::ops::Range<f64>) {
    *val = (*val + delta * mag).clamp(range.start, range.end);
}
