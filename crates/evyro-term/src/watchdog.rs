//! Notices a thread whose program is running but not getting anywhere: silent for a long
//! time, or printing the same line over and over. It only reports; the person decides
//! whether to stop it or move the task to another AI.
//!
//! It also spots an AI saying it has hit its usage limit, so the switch can be offered at once
//! instead of waiting for a usage reading.

use std::time::{Duration, Instant};

/// Consecutive identical lines that count as a loop.
pub const REPEAT_LIMIT: u32 = 30;
/// Shorter lines (spinner dots, prompts) are never counted as a loop.
const MIN_LINE: usize = 12;
const MAX_PARTIAL: usize = 4096;

/// Lines of other output after which a seen limit message is forgotten: the AI carried on.
const LIMIT_FORGOTTEN_AFTER: u32 = 80;

#[derive(Debug)]
pub struct Watch {
    last_output: Instant,
    last_line: String,
    repeats: u32,
    partial: String,
    /// The line in which the AI said it hit a usage limit, until output moves on.
    limit: Option<String>,
    lines_since_limit: u32,
}

impl Watch {
    pub fn new(now: Instant) -> Self {
        Self {
            last_output: now,
            last_line: String::new(),
            repeats: 0,
            partial: String::new(),
            limit: None,
            lines_since_limit: 0,
        }
    }

    /// The line in which the AI said it hit a usage limit, if it did and has not carried on since.
    pub fn limit(&self) -> Option<&str> {
        self.limit.as_deref()
    }

    fn note_limit(&mut self, line: &str, is_complete_line: bool) {
        if is_limit_message(line) {
            self.limit = Some(line.chars().take(160).collect());
            self.lines_since_limit = 0;
        } else if is_complete_line && self.limit.is_some() && line.chars().count() >= MIN_LINE {
            self.lines_since_limit += 1;
            if self.lines_since_limit > LIMIT_FORGOTTEN_AFTER {
                self.limit = None;
            }
        }
    }

    /// Record output that arrived at `now`.
    pub fn feed(&mut self, text: &str, now: Instant) {
        self.last_output = now;
        self.partial.push_str(text);
        while let Some(end) = self.partial.find('\n') {
            let line = clean(&self.partial[..end]);
            self.partial.drain(..=end);
            self.note_limit(&line, true);
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
        // A screen that is repainted in place may never end the line, so look at what is pending too.
        if !self.partial.is_empty() {
            let pending = clean(&self.partial);
            self.note_limit(&pending, false);
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

/// Whether a line is an AI saying it ran out of usage. These are the phrasings the coding agents
/// print; a new wording needs a line here. Matching is on words, not on exact text, and a person
/// still decides: the desktop only offers the switch.
pub fn is_limit_message(line: &str) -> bool {
    let t: String = line
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if t.len() < 12 || t.len() > 400 {
        return false;
    }
    let has = |needles: &[&str]| needles.iter().any(|n| t.contains(n));
    (t.contains("usage limit") && has(&["reached", "hit", "exceeded", "resets", "reset"]))
        || has(&[
            "hit your limit",
            "hit your usage limit",
            "reached your usage limit",
            "reached your limit",
        ])
        || (has(&[
            "5-hour limit",
            "weekly limit",
            "daily limit",
            "monthly limit",
            "session limit",
        ]) && has(&["reached", "hit", "resets"]))
        || has(&[
            "exceeded your current quota",
            "quota exceeded",
            "resource_exhausted",
            "insufficient_quota",
        ])
        || has(&[
            "out of usage",
            "out of credits",
            "credit balance is too low",
        ])
        || (t.contains("limit reached") && has(&["resets", "try again"]))
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
                            // Moving the cursor right is how a screen draws a space.
                            if n == 'C' {
                                out.push(' ');
                            }
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

    #[test]
    fn the_limit_messages_agents_print_are_recognised() {
        for yes in [
            "Claude usage limit reached. Your limit will reset at 3pm.",
            "You've hit your limit · resets 5pm (Europe/Berlin)",
            "5-hour limit reached ∙ resets 2pm",
            "Weekly limit reached, resets Monday",
            "You've hit your usage limit. Try again at 4:12 PM.",
            "Error: You exceeded your current quota, please check your plan",
            "429 RESOURCE_EXHAUSTED: Quota exceeded for this model",
            "Credit balance is too low to access the API",
            "You are out of usage for this session",
        ] {
            assert!(is_limit_message(yes), "{yes}");
        }
        for no in [
            "I will now fix the failing test.",
            "usage: evyro run --to <agent>",
            "The rate of requests is fine",
            "limit the number of retries to three",
            "reading the file",
        ] {
            assert!(!is_limit_message(no), "{no}");
        }
    }

    #[test]
    fn a_limit_is_seen_in_a_repainted_screen_and_in_coloured_text() {
        let t0 = Instant::now();
        let mut w = Watch::new(t0);
        // No newline at all, words separated by cursor-forward moves, as a full-screen UI draws them.
        w.feed(
            "\u{1b}[31mYou've\u{1b}[1Chit\u{1b}[1Cyour\u{1b}[1Climit\u{1b}[0m \u{b7} resets 5pm",
            t0,
        );
        assert!(
            w.limit().unwrap().contains("hit your limit"),
            "{:?}",
            w.limit()
        );
    }

    #[test]
    fn a_seen_limit_is_forgotten_once_the_output_carries_on() {
        let t0 = Instant::now();
        let mut w = Watch::new(t0);
        w.feed("5-hour limit reached \u{2219} resets 2pm\n", t0);
        assert!(w.limit().is_some());
        for i in 0..LIMIT_FORGOTTEN_AFTER {
            w.feed(&format!("working on step number {i}\n"), t0);
        }
        assert!(w.limit().is_some(), "not yet");
        w.feed("one more line of real output\n", t0);
        assert!(w.limit().is_none());
    }
}
