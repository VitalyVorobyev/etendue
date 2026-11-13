//! `etendue library …`: component libraries (a user bank, a project's library, the built-ins).

use std::path::PathBuf;

use anyhow::Result;
use clap::Subcommand;

/// `etendue library …`.
#[derive(Subcommand)]
pub enum LibraryCommand {
    /// List the items and robots of up to three layers, each once, from the highest layer that
    /// has it: project, then bank, then built-in.
    List {
        /// A project folder.
        #[arg(long)]
        project: Option<PathBuf>,
        /// A user bank.
        #[arg(long)]
        bank: Option<PathBuf>,
        /// The built-in library (a folder with `library/` and `robots/`, such as `examples`).
        #[arg(long)]
        builtin: Option<PathBuf>,
    },
    /// Check a bank (or any folder with `library/` and `robots/`): every item and robot.
    Check {
        /// The folder.
        dir: PathBuf,
    },
    /// Import a library item file, or a robot folder (`robot.json`, its URDF and meshes).
    Import {
        /// The item file or robot folder.
        source: PathBuf,
        /// The bank or project to import into.
        #[arg(long)]
        into: PathBuf,
        /// Replace an item or robot with the same id.
        #[arg(long)]
        force: bool,
    },
    /// Make a folder a user bank (write its `etendue-bank.json`).
    Init {
        /// The folder.
        dir: PathBuf,
        /// Its name.
        #[arg(long, default_value = "My bank")]
        name: String,
    },
}

pub fn run(command: LibraryCommand) -> Result<()> {
    use etendue_cli::library;
    match command {
        LibraryCommand::List {
            project,
            bank,
            builtin,
        } => {
            let layers: Vec<library::LayerDir> =
                [("project", project), ("bank", bank), ("builtin", builtin)]
                    .into_iter()
                    .filter_map(|(name, dir)| dir.map(|dir| library::LayerDir { name, dir }))
                    .collect();
            anyhow::ensure!(
                !layers.is_empty(),
                "name at least one of --project, --bank, --builtin"
            );
            let (listed, problems) = library::list(&layers);
            for l in &listed {
                let hides = if l.shadows.is_empty() {
                    String::new()
                } else {
                    format!("  (hides {})", l.shadows.join(", "))
                };
                println!("{:<8} {:<40} {}{hides}", l.layer, l.reference, l.name);
            }
            for p in &problems {
                eprintln!("problem: {p}");
            }
            println!(
                "{} item(s) and robot(s), {} problem(s)",
                listed.len(),
                problems.len()
            );
            Ok(())
        }
        LibraryCommand::Check { dir } => {
            let c = library::check(&dir)?;
            let counts: Vec<String> = c.counts().iter().map(|(f, n)| format!("{n} {f}")).collect();
            println!(
                "library ok: {}: {}, {} robot(s)",
                dir.display(),
                if counts.is_empty() {
                    "no items".to_owned()
                } else {
                    counts.join(", ")
                },
                c.robots.len()
            );
            Ok(())
        }
        LibraryCommand::Import {
            source,
            into,
            force,
        } => {
            let done = library::import(&source, &into, force)?;
            println!("imported → {}", into.join(done.path).display());
            Ok(())
        }
        LibraryCommand::Init { dir, name } => {
            library::init_bank(&dir, &name)?;
            println!("bank `{name}` → {}", dir.display());
            Ok(())
        }
    }
}
