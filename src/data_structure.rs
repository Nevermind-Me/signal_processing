use std::vec;

pub struct ChartBuffer {
    data: Vec<f64>,
    capacity: usize,
    head: usize, // Next write index
    len: usize,  // Current number of stored elements
}

impl ChartBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            // Allocate 2x capacity to guarantee contiguous slices when wrapped
            data: vec![0.0; capacity * 2],
            capacity,
            head: 0,
            len: 0,
        }
    }

    pub fn change_capacity(&mut self, capacity: usize) {
        if capacity == 0 {
            return;
        }
        self.capacity = capacity;
        self.data.resize(capacity * 2, 0.0);
        if self.len > capacity {
            self.len = capacity;
        }
        self.head = self.head % capacity;
    }

    pub fn push(&mut self, value: f64) {
        if self.capacity == 0 {
            return;
        }

        // Double-write: current slot and mirrored slot
        self.data[self.head] = value;
        self.data[self.head + self.capacity] = value;

        self.head = (self.head + 1) % self.capacity;
        if self.len < self.capacity {
            self.len += 1;
        }
    }

    /// Returns a single contiguous slice of all elements in chronological order (oldest -> newest).
    pub fn as_slice(&self) -> &[f64] {
        if self.len < self.capacity {
            // Buffer isn't full yet: slice from start to capacity length (contains now 0.0 default
            // values, but thats good for my purpose)
            &self.data[0..self.capacity - 1]
        } else {
            // Buffer is full: 'head' points directly to the oldest element.
            // Mirrored allocation guarantees head..head+capacity is continuous.
            &self.data[self.head..self.head + self.capacity]
        }
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }
}
