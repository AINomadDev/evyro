//! The ACP client against a stand-in agent (`fake_acp_agent.py`), with no
//! Evyro session around it: streaming updates, permission answers, file reads
//! and writes inside the root only, and cancellation.
#![cfg(unix)]

use evyro_acp::{pick_option, AcpClient, Event, Launch};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

fn fake() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fake_acp_agent.py")
}

fn start(dir: &std::path::Path, events: Arc<Mutex<Vec<Event>>>, allow: Option<bool>) -> AcpClient {
    let slot: Arc<Mutex<Option<AcpClient>>> = Arc::new(Mutex::new(None));
    let s2 = slot.clone();
    let c = AcpClient::spawn(
        &Launch {
            program: fake(),
            args: vec![],
            cwd: dir.to_path_buf(),
            env_set: vec![],
            env_remove: vec![],
        },
        Arc::new(move |e: Event| {
            if let Event::Permission { request_id, params } = &e {
                let c = s2.lock().unwrap().clone().unwrap();
                let opt = allow.and_then(|a| pick_option(params, a));
                c.answer_permission(request_id, opt.as_deref()).unwrap();
            }
            events.lock().unwrap().push(e);
        }),
    )
    .unwrap();
    *slot.lock().unwrap() = Some(c.clone());
    c
}

fn text(events: &[Event]) -> String {
    events
        .iter()
        .filter_map(|e| match e {
            Event::Update { update, .. } => update
                .pointer("/content/text")
                .and_then(|t| t.as_str())
                .map(String::from),
            _ => None,
        })
        .collect()
}

#[test]
fn a_turn_with_permission_and_files() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("repo");
    std::fs::create_dir(&dir).unwrap();
    std::fs::write(dir.join("README.md"), "# Hello\nmore\n").unwrap();
    std::fs::write(dir.join(".env"), "TOKEN=x\n").unwrap();
    std::fs::write(tmp.path().join("outside.txt"), "secret\n").unwrap();
    let events = Arc::new(Mutex::new(Vec::new()));
    let c = start(&dir, events.clone(), Some(true));
    let info = c.initialize().unwrap();
    assert_eq!(info["agentInfo"]["name"], "fake-acp");
    let sid = c.new_session(&dir, &[]).unwrap();
    assert_eq!(
        c.prompt(&sid, "please write the notes").unwrap(),
        "end_turn"
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("notes.txt")).unwrap(),
        "written by the fake agent\n"
    );
    let ev = events.lock().unwrap();
    let t = text(&ev);
    assert!(t.contains("README starts with: # Hello"), "{t}");
    assert!(t.contains("Wrote notes.txt"), "{t}");
    assert!(t.contains("Reading .env was refused"), "{t}");
    assert!(t.contains("Reading outside the project was refused"), "{t}");
    assert!(ev
        .iter()
        .any(|e| matches!(e, Event::FileWrite { path, .. } if path == "notes.txt")));
    assert!(ev
        .iter()
        .any(|e| matches!(e, Event::FileRead { path } if path == "README.md")));
    assert!(ev
        .iter()
        .any(|e| matches!(e, Event::Update { update, .. } if update["sessionUpdate"] == "plan")));
    drop(ev);
    c.shutdown();
}

#[test]
fn rejected_permission_writes_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("README.md"), "x\n").unwrap();
    let events = Arc::new(Mutex::new(Vec::new()));
    let c = start(tmp.path(), events.clone(), Some(false));
    c.initialize().unwrap();
    let sid = c.new_session(tmp.path(), &[]).unwrap();
    assert_eq!(c.prompt(&sid, "write it").unwrap(), "end_turn");
    assert!(!tmp.path().join("notes.txt").exists());
    assert!(text(&events.lock().unwrap()).contains("Permission denied"));
    c.shutdown();
}

#[test]
fn cancel_stops_the_turn() {
    let tmp = tempfile::tempdir().unwrap();
    let events = Arc::new(Mutex::new(Vec::new()));
    let c = start(tmp.path(), events.clone(), None);
    c.initialize().unwrap();
    let sid = c.new_session(tmp.path(), &[]).unwrap();
    let c2 = c.clone();
    let s2 = sid.clone();
    let h = std::thread::spawn(move || c2.prompt(&s2, "go slow").unwrap());
    for _ in 0..100 {
        if text(&events.lock().unwrap()).contains("Working slowly") {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    c.cancel(&sid).unwrap();
    assert_eq!(h.join().unwrap(), "cancelled");
    c.shutdown();
}

#[test]
fn a_dead_agent_fails_calls_instead_of_hanging() {
    let tmp = tempfile::tempdir().unwrap();
    let events = Arc::new(Mutex::new(Vec::new()));
    let c = start(tmp.path(), events.clone(), None);
    c.initialize().unwrap();
    c.shutdown();
    assert!(c.new_session(tmp.path(), &[]).is_err());
}
