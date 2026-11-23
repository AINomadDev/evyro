use evyro_core::handoff::ResumeOutcome;
use serde::Serialize;

pub struct Out {
    pub json: bool,
    pub quiet: bool,
}

impl Out {
    pub fn print<T: Serialize, F: FnOnce() -> String>(&self, v: &T, human: F) {
        if self.json {
            println!(
                "{}",
                serde_json::to_string_pretty(v).unwrap_or_else(|_| "null".into())
            );
        } else if !self.quiet {
            let s = human();
            if !s.is_empty() {
                println!("{}", s.trim_end());
            }
        }
    }
}

pub fn render_resume(r: &ResumeOutcome) -> String {
    let mut s = String::new();
    s.push_str("Resume checks:\n");
    for c in &r.checks {
        s.push_str(&format!(
            "  {} {}: {}\n",
            if c.met { "ok  " } else { "WARN" },
            c.name,
            c.detail
        ));
    }
    if r.drift.drifted {
        s.push_str(&format!(
            "\nState drift since the handoff checkpoint: {}\n",
            r.drift.summary()
        ));
        for f in &r.drift.modified_since {
            s.push_str(&format!("  modified since: {f}\n"));
        }
        for f in &r.drift.newly_dirty {
            s.push_str(&format!("  newly changed:  {f}\n"));
        }
        for f in &r.drift.no_longer_dirty {
            s.push_str(&format!("  no longer dirty: {f}\n"));
        }
        for c in &r.drift.new_commits {
            s.push_str(&format!("  new commit: {c}\n"));
        }
    }
    if !r.accepted {
        s.push_str("\nNot accepted. Review the drift (git diff), then re-run with --accept-drift to continue from the current state.\n");
        return s;
    }
    if let Some(ses) = &r.session {
        s.push_str(&format!(
            "\nAccepted handoff {}. You ({}) now own the task; session {}.\n\n",
            r.handoff.id, ses.harness, ses.id
        ));
    }
    s.push_str(&r.contract.to_markdown());
    s
}
