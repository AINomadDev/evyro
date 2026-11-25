//! Notices a thread whose program is running but not getting anywhere: silent for a long
//! time, or printing the same line over and over. It only reports; the person decides
//! whether to stop it or move the task to another AI.

use std::time::{Duration, Instant};

/// Consecutive identical lines that count as a loop.
pub const REPEAT_LIMIT: u32 = 30;
/// Shorter lines (spinner dots, prompts) are never counted as a loop.
const MIN_LINE: usize = 12;
const MAX_PARTIAL: usize = 4096;

#[derive(Debug)]
pub struct Watch {
    last_output: Instant,
    last_line: String,
    repeats: u32,
    partial: String,
}

impl Watch {
    pub fn new(now: Instant) -> Self {
        Self {
            last_output: now,
            last_line: String::new(),
            repeats: 0,
            partial: String::new(),
        }
    }

    /// Record output that arrived at `now`.
    pub fn feed(&mut self, text: &str, now: Instant) {
        self.last_output = now;
        self.partial.push_str(text);
        while let Some(end) = self.partial.find('\n') {
            let line = clean(&self.partial[..end]);
            self.partial.drain(..=end);
            if line.chars().count() < MIN_LINE {
                continue;
            }
            if line == self.last_line {
                self.repeats += 1;
            } else {
                self.last_line = line;
                self.repeats = 1;
            }
        }
        if self.partial.len() > MAX_PARTIAL {
            let keep = self.partial.len() - MAX_PARTIAL;
            let cut = (keep..self.partial.len())
                .find(|i| self.partial.is_char_boundary(*i))
                .unwrap_or(self.partial.len());
            self.partial.drain(..cut);
        }
    }

    /// Why this thread looks stuck at `now`, if it does. A loop is reported before silence.
    pub fn verdict(&self, now: Instant, quiet: Duration) -> Option<String> {
        if self.repeats >= REPEAT_LIMIT {
            let shown: String = self.last_line.chars().take(60).collect();
            return Some(format!(
                "repeating the same line {} times: {shown}",
                self.repeats
            ));
        }
        let silent = now.saturating_duration_since(self.last_output);
        if silent >= quiet {
            return Some(format!("no output for {} minutes", silent.as_secs() / 60));
        }
        None
    }
}

/// A line without colour codes, carriage returns and surrounding spaces.
fn clean(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\u{1b}' => match chars.next() {
                // CSI: parameters, then a final byte in @..~
                Some('[') => {
                    for n in chars.by_ref() {
                        if ('@'..='~').contains(&n) {
                            break;
                        }
                    }
                }
                // OSC: up to BEL or ESC \
                Some(']') => {
                    while let Some(n) = chars.next() {
                        if n == '\u{7}' {
                            break;
                        }
                        if n == '\u{1b}' {
                            chars.next();
                            break;
                        }
                    }
                }
                _ => {}
            },
            '\r' => {}
            c => out.push(c),
        }
    }
    out.trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    const QUIET: Duration = Duration::from_secs(600);

    #[test]
    fn steady_varied_output_is_fine() {
        let t0 = Instant::now();
        let mut w = Watch::new(t0);
        for i in 0..100 {
            w.feed(&format!("compiling crate number {i}\n"), t0);
        }
        assert_eq!(w.verdict(t0, QUIET), None);
    }

    #[test]
    fn the_same_line_over_and_over_is_a_loop() {
        let t0 = Instant::now();
        let mut w = Watch::new(t0);
        for _ in 0..REPEAT_LIMIT - 1 {
            w.feed("I will now fix the failing test.\n", t0);
        }
        assert_eq!(w.verdict(t0, QUIET), None);
        w.feed("I will now fix the failing test.\n", t0);
        let why = w.verdict(t0, QUIET).unwrap();
        assert!(why.contains("30 times"), "{why}");
    }

    #[test]
    fn a_different_line_resets_the_count_and_colours_do_not_hide_a_loop() {
        let t0 = Instant::now();
        let mut w = Watch::new(t0);
        for _ in 0..REPEAT_LIMIT - 1 {
            w.feed("\u{1b}[32mretrying the request now\u{1b}[0m\r\n", t0);
        }
        w.feed("something else entirely\n", t0);
        w.feed("\u{1b}[31mretrying the request now\u{1b}[0m\n", t0);
        assert_eq!(w.verdict(t0, QUIET), None);
    }

    #[test]
    fn short_lines_and_a_line_split_across_chunks() {
        let t0 = Instant::now();
        let mut w = Watch::new(t0);
        for _ in 0..100 {
            w.feed("...\n", t0);
        }
        assert_eq!(w.verdict(t0, QUIET), None);
        for _ in 0..REPEAT_LIMIT {
            w.feed("waiting for the ", t0);
            w.feed("server to answer\n", t0);
        }
        assert!(w.verdict(t0, QUIET).unwrap().contains("repeating"));
    }

    #[test]
    fn silence_is_reported_only_after_the_limit() {
        let t0 = Instant::now();
        let mut w = Watch::new(t0);
        w.feed("hello there, working on it\n", t0);
        assert_eq!(w.verdict(t0 + Duration::from_secs(599), QUIET), None);
        let why = w.verdict(t0 + Duration::from_secs(660), QUIET).unwrap();
        assert!(why.contains("11 minutes"), "{why}");
        // Output clears it.
        w.feed("back again and working\n", t0 + Duration::from_secs(700));
        assert_eq!(w.verdict(t0 + Duration::from_secs(701), QUIET), None);
    }

    #[test]
    fn an_endless_line_without_a_newline_stays_bounded() {
        let t0 = Instant::now();
        let mut w = Watch::new(t0);
        for _ in 0..100 {
            w.feed(&"é".repeat(1000), t0);
        }
        assert!(w.partial.len() <= MAX_PARTIAL + 4);
    }
}
