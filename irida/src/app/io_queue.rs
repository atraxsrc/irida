//! The order IO effects run in: one batch at a time, in the order they were
//! asked for, so a later effect always sees an earlier one's writes, even
//! across separate `update` calls.

use std::collections::VecDeque;

/// IO effects waiting to run, and whether a batch is running now.
#[derive(Debug)]
pub struct IoQueue<T> {
    pending: VecDeque<T>,
    busy: bool,
    exit_waiting: bool,
}

impl<T> Default for IoQueue<T> {
    fn default() -> Self {
        IoQueue {
            pending: VecDeque::new(),
            busy: false,
            exit_waiting: false,
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

    /// Exit was asked for: true to exit now, or false while IO is still to
    /// run, so a queued save is never dropped; `exit_due` says when.
    pub fn exit(&mut self) -> bool {
        self.exit_waiting = self.busy;
        !self.busy
    }

    /// True once a waiting exit can go ahead: everything queued has run.
    pub fn exit_due(&self) -> bool {
        self.exit_waiting && !self.busy
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

    #[test]
    fn exit_waits_for_queued_io() {
        let mut q = IoQueue::default();
        assert!(q.exit(), "idle: exit at once");
        assert!(!q.exit_due());

        assert_eq!(q.enqueue(vec![1]), Some(vec![1]));
        assert_eq!(q.enqueue(vec![2]), None);
        assert!(!q.exit(), "a save is still queued");
        assert!(!q.exit_due());
        assert_eq!(q.finished(), Some(vec![2]));
        assert!(!q.exit_due(), "2 is running");
        assert_eq!(q.finished(), None);
        assert!(q.exit_due());
    }
}
