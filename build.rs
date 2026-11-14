//! Embeds the rule packs under rules/ into the binary.
//!
//! Each folder `rules/<module>/` is the pack for the Python library imported as `<module>`, or
//! for Python itself when its manifest says `everyFile: true`.
//! `pack.yml` is the pack's manifest, and every other file is `<rule id>.yml`, the documents of
//! one rule. This script writes `OUT_DIR/packs.rs`, which src/pack.rs includes, so a new rule or
//! a new pack needs no Rust change.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};

/// The file in a pack folder that is the manifest and not a rule.
const MANIFEST: &str = "pack.yml";

fn main() -> Result<()> {
    let out = env::var_os("OUT_DIR").context("cargo sets OUT_DIR for build scripts")?;
    let root = Path::new("rules");
    // Cargo rescans a directory given here for changes to any file inside it.
    println!("cargo:rerun-if-changed={}", root.display());
    let packs = sorted_entries(root)?
        .iter()
        .filter(|path| path.is_dir())
        .map(|pack| pack_source(pack))
        .collect::<Result<Vec<String>>>()?;
    let target = PathBuf::from(out).join("packs.rs");
    fs::write(&target, format!("&[\n{}]", packs.concat()))
        .with_context(|| format!("cannot write {}", target.display()))
}

/// The `PackSource` literal for one pack folder.
fn pack_source(pack: &Path) -> Result<String> {
    let module = utf8_name(pack)?;
    ensure!(
        is_module_name(module),
        "rules/{module}: the folder name must be a Python module name"
    );
    let mut manifest = String::from("None");
    let mut rules = Vec::new();
    for file in sorted_entries(pack)? {
        let name = utf8_name(&file)?;
        // Hidden files, such as the ones a file manager leaves, are not part of the pack.
        if name.starts_with('.') {
            continue;
        }
        let include = format!(
            "include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"), {:?}))",
            format!("/rules/{module}/{name}")
        );
        if name == MANIFEST {
            manifest = format!("Some({include})");
        } else {
            let id = name.strip_suffix(".yml").with_context(|| {
                format!("rules/{module}/{name}: a pack holds {MANIFEST} and <rule id>.yml files")
            })?;
            let yaml = fs::read_to_string(&file)
                .with_context(|| format!("cannot read {}", file.display()))?;
            let mut ids = yaml.lines().filter_map(|line| line.strip_prefix("id: "));
            ensure!(
                ids.all(|found| found.trim() == id),
                "rules/{module}/{name}: every rule document in the file must have the id {id:?}"
            );
            rules.push(format!(
                "            RuleSource {{ id: {id:?}, yaml: {include} }},\n"
            ));
        }
    }
    ensure!(!rules.is_empty(), "rules/{module}: the pack has no rules");
    Ok(format!(
        "    PackSource {{\n        module: {module:?},\n        manifest: {manifest},\n        rules: &[\n{}        ],\n    }},\n",
        rules.concat()
    ))
}

/// The entries of `dir` in name order, so the binary does not depend on the file system's order.
fn sorted_entries(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)
        .and_then(|entries| entries.map(|entry| entry.map(|e| e.path())).collect())
        .with_context(|| format!("cannot list {}", dir.display()))?;
    entries.sort();
    Ok(entries)
}

fn utf8_name(path: &Path) -> Result<&str> {
    path.file_name()
        .and_then(|name| name.to_str())
        .with_context(|| format!("{} has no UTF-8 name", path.display()))
}

/// Module names also go into a regex and a dist-info lookup, so only identifiers are allowed.
fn is_module_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_lowercase() || c == '_')
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}
