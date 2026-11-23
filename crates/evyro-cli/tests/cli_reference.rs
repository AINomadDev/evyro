//! docs/user-guide/CLI_REFERENCE.md shows the real `--help` text of every
//! command it documents. This test fails when the two drift apart.
//! `EVYRO_UPDATE_DOCS=1 cargo test -p evyro-cli --test cli_reference` rewrites
//! the blocks from the current binary.

use std::path::Path;
use std::process::Command;

fn help(args: &[&str]) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_evyro"))
        .args(args)
        .arg("--help")
        .env("COLUMNS", "100")
        .output()
        .unwrap();
    assert!(out.status.success(), "evyro {args:?} --help failed");
    String::from_utf8(out.stdout)
        .unwrap()
        .replace("\r\n", "\n")
        .replace("evyro.exe", "evyro")
        .trim_end()
        .to_string()
}

#[test]
fn cli_reference_matches_help() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/user-guide/CLI_REFERENCE.md");
    let doc = std::fs::read_to_string(&path)
        .unwrap()
        .replace("\r\n", "\n");
    let update = std::env::var_os("EVYRO_UPDATE_DOCS").is_some();
    let mut out = String::new();
    let mut problems = Vec::new();
    // `## Commands` holds the top-level help; each `### evyro <cmd>` its own.
    let mut pending: Option<Vec<String>> = None;
    let mut lines = doc.lines().peekable();
    while let Some(line) = lines.next() {
        out.push_str(line);
        out.push('\n');
        if line == "## Commands" {
            pending = Some(vec![]);
        } else if let Some(cmd) = line.strip_prefix("### evyro ") {
            pending = Some(cmd.split_whitespace().map(String::from).collect());
        } else if line == "```text" {
            let Some(args) = pending.take() else { continue };
            let mut block = Vec::new();
            for l in lines.by_ref() {
                if l == "```" {
                    break;
                }
                block.push(l);
            }
            let documented = block.join("\n").trim_end().to_string();
            let args: Vec<&str> = args.iter().map(String::as_str).collect();
            let real = help(&args);
            if documented != real {
                problems.push(format!("evyro {}", args.join(" ")));
            }
            out.push_str(if update { &real } else { &documented });
            out.push_str("\n```\n");
        }
    }
    if update {
        std::fs::write(&path, out).unwrap();
        return;
    }
    assert!(
        problems.is_empty(),
        "CLI_REFERENCE.md is out of date for: {}\nrun EVYRO_UPDATE_DOCS=1 cargo test -p evyro-cli --test cli_reference",
        problems.join(", ")
    );
}

#[test]
fn every_command_is_documented() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/user-guide/CLI_REFERENCE.md");
    let doc = std::fs::read_to_string(&path).unwrap();
    let top = help(&[]);
    let cmds: Vec<&str> = top
        .lines()
        .skip_while(|l| *l != "Commands:")
        .skip(1)
        .take_while(|l| !l.is_empty())
        .filter_map(|l| l.split_whitespace().next())
        .filter(|c| *c != "help")
        .collect();
    assert!(!cmds.is_empty());
    let missing: Vec<&&str> = cmds
        .iter()
        .filter(|c| !doc.contains(&format!("### evyro {c}")))
        .collect();
    assert!(
        missing.is_empty(),
        "no `### evyro <cmd>` section for {missing:?}"
    );
}
