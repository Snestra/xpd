//! The command line: its arguments, and what each command prints and exits with.

use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use serde_json::Value;

use crate::graph::Graph;
use crate::pack;
use crate::package_check;
use crate::report::Report;
use crate::rule::Applicability;
use crate::scan;
use crate::venv::{self, Venv};

/// The agent skill that `xpd skill` prints.
const SKILL: &str = include_str!("../skills/xpd/SKILL.md");
/// The scripts that `profile` and `equiv` hand to the project's Python.
const PROFILE_PY: &str = include_str!("../py/profile.py");
const EQUIV_PY: &str = include_str!("../py/equiv.py");

/// Exit code when xpd itself fails, as opposed to finding issues.
const CHECK_FAILED: u8 = 2;
/// Claude Code shows stderr to the agent only when a hook exits 2.
const HOOK_BLOCK: u8 = 2;
/// Any other non-zero hook exit shows stderr to the user without blocking the agent.
const HOOK_FAILED: u8 = 1;
/// The shell's code for a command that could not be started.
const PYTHON_NOT_STARTED: u8 = 127;

/// xpd: fast linter for Python library habits, for humans and coding agents.
#[derive(Debug, Parser)]
#[command(name = "xpd", version, about)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

/// How `check` and `graph` print what they found.
#[derive(Debug, Clone, Copy, ValueEnum)]
enum Format {
    Text,
    Json,
}

#[derive(Debug, Subcommand)]
enum Cmd {
    /// Lint Python files. Exit 0 when clean, 1 on any error-severity issue, 2 if xpd can't run.
    ///
    /// A library's rules run only on files that import it, and the Python rules run on every file.
    /// `[*]` marks an issue `--fix` fixes.
    #[command(after_long_help = "Examples:
  xpd check src/
  xpd check analysis.py --format json
  xpd check . --fix
  xpd check . --fix --unsafe-fixes")]
    Check {
        /// Files or directories (directories respect .gitignore).
        #[arg(default_value = ".")]
        paths: Vec<PathBuf>,
        /// Output format. Default: text on a terminal, json otherwise.
        #[arg(long, value_enum)]
        format: Option<Format>,
        /// Apply safe fixes, and report what is left.
        #[arg(long)]
        fix: bool,
        /// Also count unsafe fixes, which may change behavior: apply them with --fix, or mark
        /// them as fixable without it.
        #[arg(long)]
        unsafe_fixes: bool,
    },
    /// Claude Code `PostToolUse` hook: reads hook JSON on stdin and lints the edited .py file.
    ///
    /// A file with a pipeline of steps also gets the errors and warnings of `xpd graph`.
    ///
    /// Exit 2 with issues on stderr so the agent sees them, 1 if xpd can't run, 0 otherwise.
    Hook,
    /// Profile a data file (csv, parquet, feather, json, xlsx). Needs pandas.
    Profile { file: PathBuf },
    /// Check that two scripts produce an equal `result` (`DataFrame` or Series). Needs pandas.
    Equiv { old: PathBuf, new: PathBuf },
    /// Print the dataflow of step code or Hamilton modules: what reads what, and what does not
    /// connect.
    ///
    /// A file with a function that passes a frame through other functions, as
    /// `frame.pipe(a).pipe(b)`, is read as step code. The other files are read as the Hamilton
    /// modules of one Driver. Nothing is imported or run. Exit 1 when the code would raise or
    /// Python drops a function, 2 if xpd can't run.
    #[command(after_long_help = "Examples:
  xpd graph features.py
  xpd graph loaders.py features.py --format json")]
    Graph {
        /// Files or directories (directories respect .gitignore).
        #[arg(required = true)]
        paths: Vec<PathBuf>,
        /// Output format. Default: text on a terminal, json otherwise.
        #[arg(long, value_enum)]
        format: Option<Format>,
    },
    /// Print the agent skill (SKILL.md) for this xpd version.
    Skill,
}

/// Runs the command the arguments name.
pub(crate) fn run() -> ExitCode {
    use std::io::IsTerminal;

    let default_format = if io::stdout().is_terminal() {
        Format::Text
    } else {
        Format::Json
    };
    match Cli::parse().cmd {
        Cmd::Check {
            paths,
            format,
            fix,
            unsafe_fixes,
        } => {
            let format = format.unwrap_or(default_format);
            let allowed = if unsafe_fixes {
                Applicability::Unsafe
            } else {
                Applicability::Safe
            };
            check(&paths, format, fix.then_some(allowed), allowed)
        }
        Cmd::Hook => hook(),
        Cmd::Graph { paths, format } => graph(&paths, format.unwrap_or(default_format)),
        Cmd::Profile { file } => run_py(PROFILE_PY, &[file]),
        Cmd::Equiv { old, new } => run_py(EQUIV_PY, &[old, new]),
        Cmd::Skill => {
            print!("{SKILL}");
            ExitCode::SUCCESS
        }
    }
}

/// Lints `paths` with the rules for the library versions in the project's venv, after writing
/// fixes up to `fix` when it is set.
fn lint(paths: &[PathBuf], fix: Option<Applicability>, scope: Scope) -> Result<Report> {
    let venv = Venv::find();
    let installed = venv.as_ref().map(Venv::distributions);
    let packs = pack::load(installed.as_ref()).context("invalid embedded rules")?;
    let files = scan::collect_files(paths)?;
    let scanned = scan::scan(&files, &packs, fix)?;
    let mut issues = scanned.issues;
    if let (Scope::Project, Some(venv), Some(installed)) = (scope, &venv, &installed) {
        issues.extend(package_check::missing(&packs, venv, installed));
    }
    Ok(Report {
        issues,
        fixed: scanned.fixed,
    })
}

/// How much a lint run looks at.
#[derive(Debug, Clone, Copy)]
enum Scope {
    /// The given files only. The hook runs after every edit, so it stops here.
    Files,
    /// The files, and the packages the project's venv lacks.
    Project,
}

/// `allowed` is the fix level that `[*]` and the summary count as fixable.
fn check(
    paths: &[PathBuf],
    format: Format,
    fix: Option<Applicability>,
    allowed: Applicability,
) -> ExitCode {
    let report = match lint(paths, fix, Scope::Project) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("xpd: {e:#}");
            return ExitCode::from(CHECK_FAILED);
        }
    };
    match format {
        Format::Json => println!("{}", report.to_json()),
        Format::Text => {
            for i in &report.issues {
                println!("{}", i.line(allowed));
            }
            eprintln!("xpd: {}", report.summary(allowed));
        }
    }
    if report.has_errors() {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

/// Prints the dataflow of the files under `paths`.
fn graph(paths: &[PathBuf], format: Format) -> ExitCode {
    let graph = match scan::collect_files(paths).and_then(|files| Graph::read(&files)) {
        Ok(graph) => graph,
        Err(e) => {
            eprintln!("xpd: {e:#}");
            return ExitCode::from(CHECK_FAILED);
        }
    };
    match format {
        Format::Json => println!("{}", graph.to_json()),
        Format::Text => {
            println!("{}", graph.to_text());
            eprintln!("xpd: {}", graph.summary());
        }
    }
    if graph.has_errors() {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

/// Lints the file named in the hook JSON on stdin, and prints its issues to stderr.
fn hook() -> ExitCode {
    use std::io::Read;

    let mut input = String::new();
    if let Err(e) = io::stdin().read_to_string(&mut input) {
        eprintln!("xpd hook: cannot read stdin: {e}");
        return ExitCode::from(HOOK_FAILED);
    }
    let value: Value = match serde_json::from_str(&input) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("xpd hook: stdin is not hook JSON: {e}");
            return ExitCode::from(HOOK_FAILED);
        }
    };
    let Some(path) = value
        .pointer("/tool_input/file_path")
        .and_then(Value::as_str)
    else {
        return ExitCode::SUCCESS;
    };
    if !scan::is_python(Path::new(path)) {
        return ExitCode::SUCCESS;
    }
    let report = match lint(&[PathBuf::from(path)], None, Scope::Files) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("xpd hook: {e:#}");
            return ExitCode::from(HOOK_FAILED);
        }
    };
    // A file with a pipeline of steps also gets the errors and warnings of `xpd graph`.
    let steps = std::fs::read_to_string(path)
        .map(|src| Graph::step_lines(Path::new(path), &src))
        .unwrap_or_default();
    if report.issues.is_empty() && steps.is_empty() {
        return ExitCode::SUCCESS;
    }
    for i in &report.issues {
        eprintln!("{}", i.line(Applicability::Safe));
    }
    for line in &steps {
        eprintln!("{line}");
    }
    ExitCode::from(HOOK_BLOCK)
}

/// Runs `script` with the project's Python and passes on its exit code.
fn run_py(script: &str, args: &[PathBuf]) -> ExitCode {
    match Command::new(venv::project_python())
        .arg("-c")
        .arg(script)
        .args(args)
        .status()
    {
        Ok(status) => status
            .code()
            .and_then(|c| u8::try_from(c).ok())
            .map_or(ExitCode::FAILURE, ExitCode::from),
        Err(e) => {
            eprintln!("xpd: could not start python: {e}");
            ExitCode::from(PYTHON_NOT_STARTED)
        }
    }
}
