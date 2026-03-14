/// Iterator that yields `(start_uid, end_uid)` inclusive ranges in newest-first order.
///
/// Given a UID range `[lowest_uid, uid_next)` (uid_next is exclusive, per IMAP semantics),
/// divides it into batches of `batch_size` UIDs, yielding the highest range first.
#[derive(Debug)]
pub struct BatchIterator {
    /// Lowest UID to include (inclusive).
    lowest_uid: u32,
    /// Current cursor — the next batch ends at this UID (inclusive).
    /// Decrements by batch_size each iteration.
    cursor: Option<u32>,
    /// Number of UIDs per batch.
    batch_size: u32,
}

impl BatchIterator {
    /// Create a new batch iterator.
    ///
    /// - `lowest_uid`: smallest UID in the mailbox (inclusive). Typically 1,
    ///   but may be higher if UIDs have been expunged.
    /// - `uid_next`: the UIDNEXT value from SELECT — one past the highest existing UID.
    /// - `batch_size`: number of UIDs per batch (e.g., 500).
    pub fn new(lowest_uid: u32, uid_next: u32, batch_size: u32) -> Self {
        let cursor = if uid_next > lowest_uid {
            Some(uid_next - 1) // highest existing UID
        } else {
            None // empty mailbox
        };
        Self {
            lowest_uid,
            cursor,
            batch_size,
        }
    }

    /// How many batches remain (estimate — UIDs may be sparse).
    pub fn estimated_batches(&self) -> u32 {
        match self.cursor {
            None => 0,
            Some(cursor) => {
                let range = cursor - self.lowest_uid + 1;
                (range + self.batch_size - 1) / self.batch_size
            }
        }
    }
}

impl Iterator for BatchIterator {
    /// `(start_uid, end_uid)` — both inclusive.
    type Item = (u32, u32);

    fn next(&mut self) -> Option<Self::Item> {
        let end = self.cursor?;
        if end < self.lowest_uid {
            return None;
        }

        let start = if end >= self.lowest_uid + self.batch_size - 1 {
            end - self.batch_size + 1
        } else {
            self.lowest_uid
        };

        // Advance cursor
        if start <= self.lowest_uid {
            self.cursor = None; // this was the last batch
        } else {
            self.cursor = Some(start - 1);
        }

        Some((start, end))
    }
}

/// Convert a `(start, end)` UID range to an IMAP sequence string.
///
/// - Single UID: `"42"`
/// - Range: `"501:1000"`
pub fn batch_to_sequence(start: u32, end: u32) -> String {
    if start == end {
        start.to_string()
    } else {
        format!("{start}:{end}")
    }
}
