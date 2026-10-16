//! The order IO effects run in: one batch at a time, in the order they were
//! asked for, so a later effect always sees an earlier one's writes, even
//! across separate `update` calls.

use std::collections::VecDeque;

/// IO effects waiting to run, and whether a batch is running now.
#[derive(Debug)]
pub struct IoQueue<T> {
    pending: VecDeque<T>,
    busy: bool,
}

impl<T> Default for IoQueue<T> {
    fn default() -> Self {
        IoQueue {
            pending: VecDeque::new(),
            busy: false,
        }
    }
}

impl<T> IoQueue<T> {
    /// Queue `items`; the batch to start now, or `None` while one is
    /// running (they then run after it) or when there is nothing to run.
    pub fn enqueue(&mut self, items: Vec<T>) -> Option<Vec<T>> {
        self.pending.extend(items);
        if self.busy { None } else { self.next() }
    }

    /// The running batch has finished; the next batch to start, if any.
    pub fn finished(&mut self) -> Option<Vec<T>> {
        self.busy = false;
        self.next()
    }

    /// Everything waiting, as one batch, marking it running.
    fn next(&mut self) -> Option<Vec<T>> {
        if self.pending.is_empty() {
            return None;
        }
        self.busy = true;
        Some(self.pending.drain(..).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_batch_at_a_time_in_order() {
        let mut q = IoQueue::default();
        assert_eq!(q.enqueue(Vec::<u32>::new()), None, "nothing to run");
        assert_eq!(q.enqueue(vec![1, 2]), Some(vec![1, 2]));
        // While 1 and 2 run, later effects wait.
        assert_eq!(q.enqueue(vec![3]), None);
        assert_eq!(q.enqueue(vec![4, 5]), None);
        // They run together, in order, once the running batch is done.
        assert_eq!(q.finished(), Some(vec![3, 4, 5]));
        assert_eq!(q.enqueue(vec![6]), None);
        assert_eq!(q.finished(), Some(vec![6]));
        assert_eq!(q.finished(), None);
        // Idle again: the next effects start at once.
        assert_eq!(q.enqueue(vec![7]), Some(vec![7]));
    }
}
