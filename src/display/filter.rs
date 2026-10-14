use arc_swap::ArcSwapAny;
use std::{f32::consts::PI, sync::Arc, vec};

use crossterm::event::{KeyCode, KeyModifiers};

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Stylize;
use ratatui::text::{Line, Span};

type FilterBox = Box<dyn Fn(f32) -> f32 + Send + Sync>;

// TODO: implement the rest of the fir filter popup and the missing fir filter struct itself. A
// title and so on is also missing (should look nice ...)
// IIR Filter could also be interesting ...
// very helpful (https://www.advsolned.com/unterschied-zwischen-iir-und-fir-filtern-ein-praktischer-leitfaden/)
// a pole graph could also be interesting
pub struct Filter {
    sample_rate: u32,
    active: bool,
    unresolved_commit: bool, // to track, if the output enclosure needs to be changed (saves
    // syncronized access to the shared filter enclosure)
    filter_swap: Arc<ArcSwapAny<Arc<Box<dyn Fn(f32) -> f32 + Send + Sync>>>>,
    // filter_vec: Vec<>, vector with another private struct for the fir filters
    // makes sense to display the filters in a list from 1 to n in which order they are applied to
    // output stream
}

struct FIRFilter {
    sample_rate: u32,
    order: u8,
    w: Vec<f32>,
    b: Vec<f32>,
}

struct IIRFilter {
    sample_rate: u32,
    order: u8,
    sections: Vec<Biquad>,
}

struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    w1: f32,
    w2: f32,
}

impl Biquad {
    fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.w1;
        self.w1 = self.b1 * x - self.a1 * y + self.w2;
        self.w2 = self.b2 * x - self.a2 * y;
        y
    }
}

impl FIRFilter {
    fn new(sample_rate: u32, order: u8, cutoff: f32, highpass: bool) -> Self {
        let m = order as usize;
        let mut b = vec![0.0; m + 1];
        let fc = cutoff / sample_rate as f32;

        // windowed-sinc coefficients
        for i in 0..=m {
            let n = i as f32;
            let center = m as f32 / 2.0;

            let h = if (n - center).abs() < 1e-5 {
                2.0 * fc
            } else {
                let angle = 2.0 * PI * fc * (n - center);
                angle.sin() / (PI * (n - center))
            };

            // Hamming window
            let window = 0.54 - 0.46 * (2.0 * PI * n / m as f32).cos();
            b[i] = h * window;
        }

        // highpass spectral inversion or lowpass normalization
        if highpass {
            let center = m / 2;
            for i in 0..=m {
                let delta = if i == center { 1.0 } else { 0.0 };
                b[i] = delta - b[i];
            }
        } else {
            let sum: f32 = b.iter().sum();
            if sum.abs() > 1e-6 {
                for coeff in &mut b {
                    *coeff /= sum;
                }
            }
        }

        Self {
            sample_rate,
            order,
            w: vec![0.0; m],
            b,
        }
    }

    fn process(&mut self, s: f32) -> f32 {
        let y = self.w[0] + self.b[0] * s;
        if self.order > 1 {
            for i in 0..=(self.order - 2) as usize {
                self.w[i] = self.b[i + 1] * s + self.w[i + 1];
            }
        }
        self.w[(self.order - 1) as usize] = self.b[self.order as usize] * s;
        y
    }
}

impl IIRFilter {
    fn new(sample_rate: u32, order: u8, cutoff: f32, highpass: bool) -> Self {
        let fc = cutoff.clamp(1.0, (sample_rate / 2 - 1) as f32);
        let omega = (PI * fc / sample_rate as f32).tan();
        let mut sections = Vec::new();

        let num_biquads = order / 2;
        let has_odd_pole = order % 2 != 0;

        // Generate 2nd-order sections (biquads) for Butterworth pole pairs
        for k in 0..num_biquads {
            let theta = (2 * k as u32 + 1) as f32 * PI / (2.0 * order as f32);
            let q = 1.0 / (2.0 * theta.cos());

            let denom = omega * omega + omega / q + 1.0;
            let (b0, b1, b2) = if highpass {
                let coeff = 1.0 / denom;
                (coeff, -2.0 * coeff, coeff)
            } else {
                let coeff = (omega * omega) / denom;
                (coeff, 2.0 * coeff, coeff)
            };
            let a1 = 2.0 * (omega * omega - 1.0) / denom;
            let a2 = (omega * omega - omega / q + 1.0) / denom;

            sections.push(Biquad {
                b0,
                b1,
                b2,
                a1,
                a2,
                w1: 0.0,
                w2: 0.0,
            });
        }

        // Handle odd filter orders by adding a 1st-order section
        if has_odd_pole {
            let denom = 1.0 + omega;
            let (b0, b1, b2) = if highpass {
                (1.0 / denom, -1.0 / denom, 0.0)
            } else {
                (omega / denom, omega / denom, 0.0)
            };
            let a1 = (omega - 1.0) / denom;
            let a2 = 0.0;

            sections.push(Biquad {
                b0,
                b1,
                b2,
                a1,
                a2,
                w1: 0.0,
                w2: 0.0,
            });
        }

        Self {
            sample_rate,
            order,
            sections,
        }
    }

    fn process(&mut self, mut s: f32) -> f32 {
        for section in &mut self.sections {
            s = section.process(s);
        }
        s
    }
}

impl Filter {
    pub fn new(
        sample_rate: u32,
        filter_swap: Arc<ArcSwapAny<Arc<Box<dyn Fn(f32) -> f32 + Send + Sync>>>>,
    ) -> Self {
        Filter {
            sample_rate: sample_rate,
            active: false,
            filter_swap: filter_swap,
            unresolved_commit: false,
        }
    }

    pub fn handle_key(&mut self, code: KeyCode, modifiers: KeyModifiers) {
        match code {
            KeyCode::Char('a') => self.active = !self.active,
            KeyCode::Char('c') => self.unresolved_commit = true,
            _ => {}
        }
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        if self.unresolved_commit {
            self.resolve_commit();
        }
        let layout = Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).spacing(1);
        let [top, main] = frame.area().layout(&layout);
        let title = Line::from_iter([
            Span::from("FIR- and IIR-Filter").bold(),
            Span::from("(Press 'h' for controls)").dark_gray(),
        ]);
        frame.render_widget(title.centered(), top);
    }

    fn resolve_commit(&mut self) {
        if self.active {
            let new_filter: FilterBox = Box::new(|s| {
                let t = 0.5 * s;
                t
            });
            self.filter_swap.store(Arc::new(new_filter));
        } else {
            let new_filter: FilterBox = Box::new(|s| s);
            self.filter_swap.store(Arc::new(new_filter));
        }
        self.unresolved_commit = false;
    }
}
