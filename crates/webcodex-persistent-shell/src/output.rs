//! Bounded output retention.

use super::*;

#[derive(Debug)]
pub struct BoundedBuffer {
    bytes: VecDeque<u8>,
    first_offset: u64,
    next_offset: u64,
    max_bytes: usize,
}

impl BoundedBuffer {
    pub fn new(max_bytes: usize) -> Self {
        Self {
            bytes: VecDeque::with_capacity(max_bytes.min(8192)),
            first_offset: 0,
            next_offset: 0,
            max_bytes: max_bytes.max(MIN_OUTPUT_BYTES),
        }
    }

    pub fn append(&mut self, bytes: &[u8]) {
        self.next_offset = self
            .next_offset
            .saturating_add(u64::try_from(bytes.len()).unwrap_or(u64::MAX));
        self.bytes.extend(bytes.iter().copied());
        while self.bytes.len() > self.max_bytes {
            self.bytes.pop_front();
            self.first_offset = self.first_offset.saturating_add(1);
        }
    }

    pub fn set_max_bytes(&mut self, max_bytes: usize) {
        self.max_bytes = max_bytes.max(MIN_OUTPUT_BYTES);
        while self.bytes.len() > self.max_bytes {
            self.bytes.pop_front();
            self.first_offset = self.first_offset.saturating_add(1);
        }
    }

    pub fn cursor(&self) -> u64 {
        self.next_offset
    }

    pub fn snapshot_since(&self, requested_start: u64) -> (String, bool) {
        let start = requested_start.max(self.first_offset).min(self.next_offset);
        let skip = usize::try_from(start.saturating_sub(self.first_offset)).unwrap_or(usize::MAX);
        let retained = self.bytes.iter().skip(skip).copied().collect::<Vec<_>>();
        (
            String::from_utf8_lossy(&retained).into_owned(),
            requested_start < self.first_offset,
        )
    }
}
