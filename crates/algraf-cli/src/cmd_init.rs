//! `algraf init` - create project-level agent guidance files.

use std::fs;
use std::path::{Path, PathBuf};

use clap::Args;

use crate::error::CliError;

const LANGUAGE_FILE: &str = "ALGRAF_LANG.md";
const LANGUAGE_TEMPLATE: &str = include_str!("../templates/ALGRAF_LANGUAGE.md");
const TOOLING_TEMPLATE: &str = include_str!("../templates/ALGRAF_TOOLING.md");
const MARKER: &str = "<!-- algraf-init-language-reference -->";

#[derive(Args)]
pub(crate) struct InitArgs {
    #[arg(default_value = ".")]
    pub(crate) dir: PathBuf,
    #[arg(long)]
    pub(crate) codex: bool,
    #[arg(long)]
    pub(crate) claude: bool,
    #[arg(long)]
    pub(crate) agy: bool,
}

pub(crate) fn init_cmd(args: InitArgs) -> Result<(), CliError> {
    let actions = init_agent_files(&args.dir, args.codex, args.claude, args.agy)?;
    for action in actions {
        println!("{action}");
    }
    Ok(())
}

fn init_agent_files(
    dir: &Path,
    codex: bool,
    claude: bool,
    agy: bool,
) -> Result<Vec<String>, CliError> {
    if !codex && !claude && !agy {
        return Err(CliError::Usage(
            "choose at least one agent target: --codex, --claude, or --agy".to_string(),
        ));
    }
    if dir.exists() && !dir.is_dir() {
        return Err(CliError::Usage(format!(
            "`{}` is not a directory",
            dir.display()
        )));
    }
    fs::create_dir_all(dir)
        .map_err(|error| CliError::Io(format!("could not create `{}`: {error}", dir.display())))?;

    let mut actions = Vec::new();
    let language_reference = composed_language_reference();
    actions.push(ensure_exact_file(
        &dir.join(LANGUAGE_FILE),
        &language_reference,
    )?);

    if codex || agy {
        actions.push(ensure_agent_reference(
            &dir.join("AGENTS.md"),
            "Agent Instructions",
        )?);
    }
    if claude {
        actions.push(ensure_agent_reference(
            &dir.join("CLAUDE.md"),
            "Claude Instructions",
        )?);
    }

    Ok(actions)
}

fn ensure_exact_file(path: &Path, content: &str) -> Result<String, CliError> {
    if path.exists() {
        let existing = fs::read_to_string(path).map_err(|error| {
            CliError::Io(format!("could not read `{}`: {error}", path.display()))
        })?;
        if existing == content {
            Ok(format!("unchanged {}", path.display()))
        } else {
            Err(CliError::Usage(format!(
                "refusing to overwrite existing `{}`; move it aside or merge it manually",
                path.display()
            )))
        }
    } else {
        fs::write(path, content).map_err(|error| {
            CliError::Io(format!("could not write `{}`: {error}", path.display()))
        })?;
        Ok(format!("wrote {}", path.display()))
    }
}

fn composed_language_reference() -> String {
    format!("{LANGUAGE_TEMPLATE}\n{TOOLING_TEMPLATE}")
}

fn ensure_agent_reference(path: &Path, title: &str) -> Result<String, CliError> {
    let block = reference_block();
    if path.exists() {
        let mut existing = fs::read_to_string(path).map_err(|error| {
            CliError::Io(format!("could not read `{}`: {error}", path.display()))
        })?;
        if existing.contains(MARKER) || existing.contains(LANGUAGE_FILE) {
            return Ok(format!("unchanged {}", path.display()));
        }
        if !existing.ends_with('\n') {
            existing.push('\n');
        }
        existing.push_str(block);
        fs::write(path, existing).map_err(|error| {
            CliError::Io(format!("could not write `{}`: {error}", path.display()))
        })?;
        Ok(format!("updated {}", path.display()))
    } else {
        let content = format!("# {title}\n{block}");
        fs::write(path, content).map_err(|error| {
            CliError::Io(format!("could not write `{}`: {error}", path.display()))
        })?;
        Ok(format!("wrote {}", path.display()))
    }
}

fn reference_block() -> &'static str {
    "\n<!-- algraf-init-language-reference -->\n## Algraf Language Reference\n\nThis project uses Algraf. Before creating or editing `.ag` files, read `ALGRAF_LANG.md` at the project root. Use `algraf check chart.ag` for diagnostics and `algraf format chart.ag` before handing code back.\n<!-- /algraf-init-language-reference -->\n"
}
