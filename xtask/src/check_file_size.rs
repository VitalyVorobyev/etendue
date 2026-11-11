//! `cargo xtask check-file-size [--update]`: no source file grows past [`LIMIT`] lines.
//!
//! A file is split by responsibility long before it reaches the limit (`.claude/CLAUDE.md`,
//! Conventions). Files that were over it when the rule came in are listed in [`ALLOWANCES`] with
//! their length then; an allowance only shrinks. The check fails when:
//! - a file without an allowance is over the limit;
//! - a file is longer than its allowance;
//! - an allowance is stale: its file is shorter than it says, at or under the limit, or gone.
//!
//! `--update` lowers stale allowances to the files' lengths and drops those no longer needed. It
//! never raises one: that is a hand edit a reviewer sees.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail, ensure};

/// The most lines a source file may have, its tests included.
pub const LIMIT: usize = 800;

/// The allowances, `<lines> <path>` per line, relative to the workspace root.
const ALLOWANCES: &str = "xtask/file-size-allowances.txt";

/// Source files: what people write and review. Generated files are not.
fn is_source(path: &str) -> bool {
    let generated = path.ends_with(".d.ts")
        || path.starts_with("crates/etendue-wasm/js/types/")
        || path.starts_with("crates/etendue-wasm/pkg/");
    let source = [".rs", ".ts", ".tsx", ".js", ".mjs", ".py"]
        .iter()
        .any(|e| path.ends_with(e));
    source && !generated
}

fn tracked(root: &Path) -> Result<Vec<String>> {
    let out = Command::new("git")
        .arg("ls-files")
        .current_dir(root)
        .output()
        .context("running git ls-files")?;
    ensure!(out.status.success(), "git ls-files failed");
    Ok(String::from_utf8(out.stdout)?
        .lines()
        .filter(|p| is_source(p))
        .map(str::to_owned)
        .collect())
}

fn read_allowances(root: &Path) -> Result<BTreeMap<String, usize>> {
    let text = std::fs::read_to_string(root.join(ALLOWANCES))
        .with_context(|| format!("reading {ALLOWANCES}"))?;
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| {
            let (n, path) = l
                .split_once(' ')
                .with_context(|| format!("{ALLOWANCES}: `{l}` is not `<lines> <path>`"))?;
            Ok((path.trim().to_owned(), n.parse()?))
        })
        .collect()
}

fn write_allowances(root: &Path, allow: &BTreeMap<String, usize>) -> Result<()> {
    let mut text = String::from(
        "# Source files over the line limit when the rule came in (`cargo xtask check-file-size`).\n\
         # Each number only goes down: split the file, then lower it (`--update`). Do not add lines.\n",
    );
    for (path, n) in allow {
        text.push_str(&format!("{n} {path}\n"));
    }
    std::fs::write(root.join(ALLOWANCES), text).with_context(|| format!("writing {ALLOWANCES}"))
}

pub fn run(root: &Path, update: bool) -> Result<()> {
    let mut allow = read_allowances(root)?;
    let mut lengths = BTreeMap::new();
    for path in tracked(root)? {
        let text =
            std::fs::read_to_string(root.join(&path)).with_context(|| format!("reading {path}"))?;
        lengths.insert(path, text.lines().count());
    }
    let mut problems = Vec::new();
    for (path, &n) in &lengths {
        match allow.get(path) {
            None if n > LIMIT => problems.push(format!(
                "{path}: {n} lines, over the limit of {LIMIT}; split it by responsibility"
            )),
            Some(&a) if n > a => problems.push(format!(
                "{path}: {n} lines, over its allowance of {a}; split it instead of growing it"
            )),
            _ => {}
        }
    }
    let stale: Vec<(String, Option<usize>)> = allow
        .iter()
        .filter_map(|(path, &a)| match lengths.get(path) {
            None => Some((path.clone(), None)),
            Some(&n) if n <= LIMIT || n < a => Some((path.clone(), Some(n))),
            _ => None,
        })
        .collect();
    for (path, n) in &stale {
        if update {
            match n {
                Some(n) if *n > LIMIT => {
                    allow.insert(path.clone(), *n);
                }
                _ => {
                    allow.remove(path);
                }
            }
        } else {
            problems.push(match n {
                None => format!("{ALLOWANCES}: `{path}` is gone; drop its allowance (--update)"),
                Some(n) if *n <= LIMIT => format!(
                    "{ALLOWANCES}: `{path}` is {n} lines, within the limit; drop its allowance (--update)"
                ),
                Some(n) => format!(
                    "{ALLOWANCES}: `{path}` is down to {n} lines; lower its allowance (--update)"
                ),
            });
        }
    }
    if update && !stale.is_empty() {
        write_allowances(root, &allow)?;
        println!(
            "{ALLOWANCES}: {} allowance(s) lowered or dropped",
            stale.len()
        );
    }
    if !problems.is_empty() {
        bail!("file sizes:\n  {}", problems.join("\n  "));
    }
    println!(
        "file sizes: {} source files, the largest without an allowance within {LIMIT} lines; {} allowance(s)",
        lengths.len(),
        allow.len()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_files_are_not_sources() {
        assert!(is_source("crates/etendue-cli/src/calibrate.rs"));
        assert!(is_source("web/apps/studio/src/App.tsx"));
        assert!(!is_source("crates/etendue-wasm/js/types/library.d.ts"));
        assert!(!is_source("crates/etendue-wasm/pkg/index.js"));
        assert!(!is_source("Cargo.lock"));
    }
}
