use arc_swap::ArcSwapAny;
use std::{sync::Arc, vec};

use crossterm::event::{KeyCode, KeyModifiers};

use ratatui::{Frame, layout::Rect};

type FilterBox = Box<dyn Fn(f32) -> f32 + Send + Sync>;

// TODO: implement the rest of the fir filter popup and the missing fir filter struct itself. A
// title and so on is also missing (should look nice ...)
pub struct FIR_Filter {
    sample_rate: u32,
    active: bool,
    unresolved_commit: bool, // to track, if the output enclosure needs to be changed (saves
    // syncronized access to the shared filter enclosure)
    filter_swap: Arc<ArcSwapAny<Arc<Box<dyn Fn(f32) -> f32 + Send + Sync>>>>,
    // filter_vec: Vec<>, vector with another private struct for the fir filters
    // makes sense to display the filters in a list from 1 to n in which order they are applied to
    // output stream
}

impl FIR_Filter {
    pub fn new(
        sample_rate: u32,
        filter_swap: Arc<ArcSwapAny<Arc<Box<dyn Fn(f32) -> f32 + Send + Sync>>>>,
    ) -> Self {
        FIR_Filter {
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
