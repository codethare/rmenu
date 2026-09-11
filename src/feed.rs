//! Stdin streaming: read lines on a background thread, hand them over in batches.

use crate::items;
use std::io::{self, BufRead};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

/// Stdin lines arriving from a reader thread. The event loop already polls
/// every 16ms, so the queue only needs a mutex — no extra wakeup source.
pub(crate) struct ItemFeed {
    queue: Mutex<Vec<items::Item>>,
    done: AtomicBool,
}

impl ItemFeed {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            queue: Mutex::new(Vec::new()),
            done: AtomicBool::new(false),
        })
    }

    fn push(&self, batch: Vec<items::Item>) {
        self.queue.lock().unwrap().extend(batch);
    }

    pub(crate) fn drain(&self) -> Vec<items::Item> {
        std::mem::take(&mut *self.queue.lock().unwrap())
    }

    fn mark_done(&self) {
        self.done.store(true, Ordering::Release);
    }

    pub(crate) fn done(&self) -> bool {
        self.done.load(Ordering::Acquire)
    }

    /// Read stdin to EOF in a background thread, handing lines over in batches
    /// so a slow producer never delays the menu appearing.
    pub(crate) fn spawn() -> Arc<Self> {
        let feed = Self::new();
        let reader = Arc::clone(&feed);
        std::thread::spawn(move || {
            reader.read_into(io::stdin().lock());
            reader.mark_done();
        });
        feed
    }

    /// Drain a reader into batches. A line that is not valid UTF-8 is skipped,
    /// never a reason to stop: truncating the menu at the first bad filename
    /// would silently hide everything after it.
    fn read_into(&self, reader: impl BufRead) {
        const BATCH: usize = 256;
        let mut batch = Vec::with_capacity(BATCH);
        for line in reader.lines() {
            let Ok(line) = line else { continue };
            batch.push(items::parse(&line));
            if batch.len() >= BATCH {
                self.push(std::mem::take(&mut batch));
            }
        }
        if !batch.is_empty() {
            self.push(batch);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_feed_drains_batches_in_order_and_signals_eof() {
        let feed = ItemFeed::new();
        feed.push(vec![items::parse("a")]);
        feed.push(vec![items::parse("b"), items::parse("c")]);
        let got: Vec<String> = feed.drain().into_iter().map(|i| i.text).collect();
        assert_eq!(got, vec!["a", "b", "c"]);
        assert!(feed.drain().is_empty(), "drain empties the queue");
        assert!(!feed.done());
        feed.mark_done();
        assert!(feed.done());
    }

    #[test]
    fn item_feed_skips_invalid_utf8_without_truncating() {
        let feed = ItemFeed::new();
        feed.read_into(&b"good\n\xff not utf8\nlast\n"[..]);
        let got: Vec<String> = feed.drain().into_iter().map(|i| i.text).collect();
        assert_eq!(
            got,
            vec!["good", "last"],
            "a bad line must not end the stream"
        );
    }
}
