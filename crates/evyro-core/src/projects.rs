//! Global project registry with stable IDs, stack and command detection.

use crate::db::{from_json, to_json, Store};
use crate::error::{Error, Result};
use crate::{brand, git, util};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Commands {
    #[serde(default, deserialize_with = "command_list")]
    pub build: Vec<Vec<String>>,
    #[serde(default, deserialize_with = "command_list")]
    pub test: Vec<Vec<String>>,
    #[serde(default, deserialize_with = "command_list")]
    pub lint: Vec<Vec<String>>,
}

/// Commands are a list of argv lists, `test = [["cargo", "test"]]`. People
/// also write one command as a list of words (`test = ["./test.sh"]`) or as
/// a string (`test = "cargo test"`, split on spaces, no shell), so accept those.
fn command_list<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> std::result::Result<Vec<Vec<String>>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Item {
        Argv(Vec<String>),
        Line(String),
    }
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Shape {
        Line(String),
        List(Vec<Item>),
    }
    let words = |s: &str| s.split_whitespace().map(String::from).collect::<Vec<_>>();
    Ok(match Shape::deserialize(d)? {
        Shape::Line(s) => vec![words(&s)],
        // ["cargo", "test"] is one argv; ["cargo test", "npm test"] is two lines.
        Shape::List(items)
            if !items.is_empty()
                && items
                    .iter()
                    .all(|i| matches!(i, Item::Line(s) if !s.contains(char::is_whitespace))) =>
        {
            vec![items
                .into_iter()
                .map(|i| match i {
                    Item::Line(s) => s,
                    Item::Argv(_) => unreachable!(),
                })
                .collect()]
        }
        Shape::List(items) => items
            .into_iter()
            .map(|i| match i {
                Item::Argv(a) => a,
                Item::Line(s) => words(&s),
            })
            .collect(),
    }
    .into_iter()
    .filter(|a| !a.is_empty())
    .collect())
}

impl Commands {
    pub fn all(&self) -> impl Iterator<Item = &Vec<String>> {
        self.build
            .iter()
            .chain(self.test.iter())
            .chain(self.lint.iter())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub alias: Option<String>,
    pub root_path: String,
    pub git_remote: Option<String>,
    pub main_branch: Option<String>,
    pub stack: Vec<String>,
    pub commands: Commands,
    pub archived: bool,
    pub favorite: bool,
    pub created_at: String,
    pub updated_at: String,
    pub last_activity_at: Option<String>,
}

impl Project {
    pub fn root(&self) -> PathBuf {
        PathBuf::from(&self.root_path)
    }
    pub fn evyro_dir(&self) -> PathBuf {
        self.root().join(brand::PROJECT_DIR)
    }
}

/// Portable, committed identity file: `.evyro/project.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectFile {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub commands: Commands,
}

pub fn read_project_file(root: &Path) -> Option<ProjectFile> {
    project_file_problem(root).ok().flatten()
}

/// Read `.evyro/project.toml`: `Ok(None)` when there is none, `Err` with the
/// parse error when it cannot be read, so callers can say what is wrong
/// instead of silently ignoring the file.
pub fn project_file_problem(root: &Path) -> std::result::Result<Option<ProjectFile>, String> {
    let p = root.join(brand::PROJECT_DIR).join("project.toml");
    let Ok(s) = std::fs::read_to_string(&p) else {
        return Ok(None);
    };
    toml::from_str(&s)
        .map(Some)
        .map_err(|e| format!("{} could not be read: {e}", p.display()))
}

/// Detect stacks and default build/test/lint argv from marker files.
pub fn detect_stack(root: &Path) -> (Vec<String>, Commands) {
    let mut stack = Vec::new();
    let mut c = Commands::default();
    let has = |f: &str| root.join(f).exists();
    let argv = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();

    if has("Cargo.toml") {
        stack.push("rust".into());
        c.build.push(argv(&["cargo", "build"]));
        c.test.push(argv(&["cargo", "test"]));
        c.lint.push(argv(&["cargo", "clippy", "--all-targets"]));
    }
    if has("package.json") {
        stack.push("node".into());
        let pm = if has("pnpm-lock.yaml") {
            "pnpm"
        } else if has("yarn.lock") {
            "yarn"
        } else if has("bun.lockb") || has("bun.lock") {
            "bun"
        } else {
            "npm"
        };
        let scripts: serde_json::Value = std::fs::read_to_string(root.join("package.json"))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        let scripts = scripts.get("scripts").cloned().unwrap_or_default();
        for (key, list) in [
            ("build", &mut c.build),
            ("test", &mut c.test),
            ("lint", &mut c.lint),
        ] {
            if scripts.get(key).is_some() {
                list.push(argv(&[pm, "run", key]));
            }
        }
        if has("tsconfig.json") {
            stack.push("typescript".into());
        }
    }
    if has("pyproject.toml") || has("requirements.txt") || has("setup.py") {
        stack.push("python".into());
        c.test.push(argv(&["python", "-m", "pytest"]));
    }
    if has("go.mod") {
        stack.push("go".into());
        c.build.push(argv(&["go", "build", "./..."]));
        c.test.push(argv(&["go", "test", "./..."]));
        c.lint.push(argv(&["go", "vet", "./..."]));
    }
    if has("pom.xml") {
        stack.push("java".into());
        c.test.push(argv(&["mvn", "-q", "test"]));
    } else if has("build.gradle") || has("build.gradle.kts") {
        stack.push("java".into());
        let g = if has("gradlew") {
            "./gradlew"
        } else {
            "gradle"
        };
        c.test.push(argv(&[g, "test"]));
    }
    if has("build.sbt") {
        stack.push("scala".into());
        c.test.push(argv(&["sbt", "test"]));
    }
    (stack, c)
}

fn row_to_project(r: &rusqlite::Row) -> rusqlite::Result<Project> {
    Ok(Project {
        id: r.get(0)?,
        name: r.get(1)?,
        alias: r.get(2)?,
        root_path: r.get(3)?,
        git_remote: r.get(4)?,
        main_branch: r.get(5)?,
        stack: from_json(r.get(6)?),
        commands: from_json(r.get(7)?),
        archived: r.get::<_, i64>(8)? != 0,
        favorite: r.get::<_, i64>(9)? != 0,
        created_at: r.get(10)?,
        updated_at: r.get(11)?,
        last_activity_at: r.get(12)?,
    })
}

const PROJECT_COLS: &str = "id, name, alias, root_path, git_remote, main_branch, stack, commands, archived, favorite, created_at, updated_at, last_activity_at";

#[derive(Debug, Clone, Serialize)]
pub struct RegisterOutcome {
    pub project: Project,
    /// "created" | "existing" | "moved"
    pub action: String,
}

impl Store {
    /// Register (or re-associate) a repository. Idempotent.
    pub fn register_project(&self, path: &Path, name: Option<&str>) -> Result<RegisterOutcome> {
        let root = git::toplevel(path)
            .unwrap_or_else(|_| util::canonical(path).unwrap_or(path.to_path_buf()));
        let root = util::canonical(&root)?;
        let root_s = root.to_string_lossy().to_string();

        if let Some(p) = self.find_project_by_root(&root_s)? {
            return Ok(RegisterOutcome {
                project: p,
                action: "existing".into(),
            });
        }

        let (stack, detected) = detect_stack(&root);
        let pf = read_project_file(&root);
        let remote = git::remote_url(&root).ok().flatten();
        let branch = git::default_branch(&root).ok().flatten();
        let now = util::now();

        // A committed project.toml carries a stable id across moves/renames.
        if let Some(pf) = &pf {
            if let Ok(existing) = self.get_project(&pf.id) {
                if Path::new(&existing.root_path)
                    .join(brand::PROJECT_DIR)
                    .join("project.toml")
                    .exists()
                {
                    return Err(Error::Conflict(format!(
                        "project id {} is already registered at {} (looks like a copy or second clone). \
                         Remove .evyro/project.toml in the copy to register it as a new project.",
                        pf.id, existing.root_path
                    )));
                }
                self.conn.execute(
                    "UPDATE projects SET root_path = ?1, git_remote = ?2, updated_at = ?3 WHERE id = ?4",
                    rusqlite::params![root_s, remote, now, existing.id],
                )?;
                self.emit(
                    Some(&existing.id),
                    None,
                    None,
                    "project.moved",
                    serde_json::json!({"from": existing.root_path, "to": root_s}),
                )?;
                return Ok(RegisterOutcome {
                    project: self.get_project(&existing.id)?,
                    action: "moved".into(),
                });
            }
        }

        let id = pf
            .as_ref()
            .map(|p| p.id.clone())
            .unwrap_or_else(|| util::new_id("prj"));
        let name = name
            .map(|s| s.to_string())
            .or_else(|| pf.as_ref().map(|p| p.name.clone()))
            .unwrap_or_else(|| {
                root.file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or("project".into())
            });
        let commands = match &pf {
            Some(p) if p.commands != Commands::default() => p.commands.clone(),
            _ => detected,
        };
        self.conn.execute(
            &format!("INSERT INTO projects({PROJECT_COLS}) VALUES (?1,?2,NULL,?3,?4,?5,?6,?7,0,0,?8,?8,?8)"),
            rusqlite::params![id, name, root_s, remote, branch, to_json(&stack), to_json(&commands), now],
        )?;
        self.emit(
            Some(&id),
            None,
            None,
            "project.registered",
            serde_json::json!({"root": root_s}),
        )?;
        Ok(RegisterOutcome {
            project: self.get_project(&id)?,
            action: "created".into(),
        })
    }

    /// A project, with its commands refreshed from `.evyro/project.toml`.
    pub fn get_project(&self, id: &str) -> Result<Project> {
        let p = self.get_project_stored(id)?;
        self.sync_project_file(p)
    }

    /// A project as stored, without re-reading `.evyro/project.toml`.
    pub fn get_project_stored(&self, id: &str) -> Result<Project> {
        self.conn
            .query_row(
                &format!("SELECT {PROJECT_COLS} FROM projects WHERE id = ?1 OR alias = ?1"),
                [id],
                row_to_project,
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Error::NotFound(format!("project {id}")),
                e => e.into(),
            })
    }

    pub fn find_project_by_root(&self, root: &str) -> Result<Option<Project>> {
        match self.conn.query_row(
            &format!("SELECT {PROJECT_COLS} FROM projects WHERE root_path = ?1"),
            [root],
            row_to_project,
        ) {
            Ok(p) => Ok(Some(p)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Resolve the project that contains `cwd` (walking up to the registered root).
    pub fn project_for_path(&self, cwd: &Path) -> Result<Project> {
        let start = util::canonical(cwd)?;
        let mut cur: Option<&Path> = Some(&start);
        while let Some(dir) = cur {
            if let Some(p) = self.find_project_by_root(&dir.to_string_lossy())? {
                return self.sync_project_file(p);
            }
            if let Some(pf) = read_project_file(dir) {
                if let Ok(p) = self.get_project(&pf.id) {
                    return Ok(p);
                }
            }
            cur = dir.parent();
        }
        // Linked git worktrees: resolve via the main working tree.
        if let Ok(main) = git::main_worktree(&start) {
            if let Some(p) = self.find_project_by_root(&main.to_string_lossy())? {
                return self.sync_project_file(p);
            }
        }
        Err(Error::NotFound(format!(
            "no registered project contains {}; run `{} init` there",
            start.display(),
            brand::CLI_NAME
        )))
    }

    /// `.evyro/project.toml` is the portable source of truth for commands.
    pub fn sync_project_file(&self, p: Project) -> Result<Project> {
        if let Some(pf) = read_project_file(&p.root()) {
            if pf.id == p.id && pf.commands != p.commands {
                self.set_project_commands(&p.id, &pf.commands)?;
                return self.get_project_stored(&p.id);
            }
        }
        Ok(p)
    }

    pub fn list_projects(
        &self,
        include_archived: bool,
        query: Option<&str>,
    ) -> Result<Vec<Project>> {
        let mut st = self.conn.prepare(&format!(
            "SELECT {PROJECT_COLS} FROM projects WHERE (?1 OR archived = 0)
             AND (?2 IS NULL OR name LIKE '%' || ?2 || '%' OR root_path LIKE '%' || ?2 || '%' OR alias LIKE '%' || ?2 || '%')
             ORDER BY favorite DESC, COALESCE(last_activity_at, created_at) DESC"
        ))?;
        let rows = st.query_map(rusqlite::params![include_archived, query], row_to_project)?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn set_project_flags(
        &self,
        id: &str,
        alias: Option<&str>,
        archived: Option<bool>,
        favorite: Option<bool>,
    ) -> Result<Project> {
        let p = self.get_project(id)?;
        let now = util::now();
        if let Some(a) = alias {
            self.conn.execute(
                "UPDATE projects SET alias = NULLIF(?1, ''), updated_at = ?2 WHERE id = ?3",
                rusqlite::params![a, now, p.id],
            )?;
        }
        if let Some(a) = archived {
            self.conn.execute(
                "UPDATE projects SET archived = ?1, updated_at = ?2 WHERE id = ?3",
                rusqlite::params![a, now, p.id],
            )?;
        }
        if let Some(f) = favorite {
            self.conn.execute(
                "UPDATE projects SET favorite = ?1, updated_at = ?2 WHERE id = ?3",
                rusqlite::params![f, now, p.id],
            )?;
        }
        self.get_project(&p.id)
    }

    pub fn set_project_commands(&self, id: &str, commands: &Commands) -> Result<()> {
        self.conn.execute(
            "UPDATE projects SET commands = ?1, updated_at = ?2 WHERE id = ?3",
            rusqlite::params![to_json(commands), util::now(), id],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod command_shape_tests {
    use super::*;
    fn test_cmds(toml_text: &str) -> Vec<Vec<String>> {
        let f: ProjectFile = toml::from_str(&format!(
            "schema_version = 1\nid = \"p\"\nname = \"n\"\n[commands]\n{toml_text}\n"
        ))
        .unwrap();
        f.commands.test
    }
    fn v(a: &[&[&str]]) -> Vec<Vec<String>> {
        a.iter()
            .map(|x| x.iter().map(|s| s.to_string()).collect())
            .collect()
    }
    #[test]
    fn every_reasonable_shape_is_read() {
        assert_eq!(
            test_cmds("test = [[\"cargo\", \"test\"]]"),
            v(&[&["cargo", "test"]])
        );
        assert_eq!(
            test_cmds("test = \"cargo test --all\""),
            v(&[&["cargo", "test", "--all"]])
        );
        assert_eq!(test_cmds("test = [\"./test.sh\"]"), v(&[&["./test.sh"]]));
        assert_eq!(
            test_cmds("test = [\"cargo\", \"test\"]"),
            v(&[&["cargo", "test"]])
        );
        assert_eq!(
            test_cmds("test = [\"cargo test\", \"npm test\"]"),
            v(&[&["cargo", "test"], &["npm", "test"]])
        );
        assert_eq!(
            test_cmds("test = [[\"cargo\", \"test\"], \"npm test\"]"),
            v(&[&["cargo", "test"], &["npm", "test"]])
        );
        assert_eq!(test_cmds("test = []"), Vec::<Vec<String>>::new());
        assert_eq!(test_cmds("test = \"\""), Vec::<Vec<String>>::new());
    }
    #[test]
    fn unreadable_file_is_reported() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(d.path().join(brand::PROJECT_DIR)).unwrap();
        assert!(matches!(project_file_problem(d.path()), Ok(None)));
        std::fs::write(
            d.path().join(brand::PROJECT_DIR).join("project.toml"),
            "test = = 1",
        )
        .unwrap();
        let e = project_file_problem(d.path()).unwrap_err();
        assert!(e.contains("project.toml could not be read"), "{e}");
    }
}
