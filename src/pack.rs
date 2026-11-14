//! Rule packs, one per Python library, embedded at compile time by build.rs.

use std::collections::BTreeMap;
use std::fmt;

use anyhow::{Context, Error, Result};
use ast_grep_config::{GlobalRules, RuleConfig, Severity};
use ast_grep_language::SupportLang;
use serde::Deserialize;

use crate::package_check::PackageCheck;
use crate::rule::Rule;
use crate::scan::Root;
use crate::venv::Distributions;
use crate::version::Version;

/// One pack as build.rs found it under `rules/<module>/`.
#[derive(Debug)]
struct PackSource {
    module: &'static str,
    /// The text of `pack.yml`, when the pack has one.
    manifest: Option<&'static str>,
    rules: &'static [RuleSource],
}

/// One rule file, `rules/<module>/<id>.yml`. build.rs checks that its documents have that id.
#[derive(Debug)]
struct RuleSource {
    id: &'static str,
    yaml: &'static str,
}

/// Every pack under `rules/`, in name order.
const SOURCES: &[PackSource] = include!(concat!(env!("OUT_DIR"), "/packs.rs"));

/// A pack's `pack.yml`: what belongs to the library as a whole and not to one rule.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Manifest {
    #[serde(default)]
    aliases: BTreeMap<String, String>,
    #[serde(default)]
    packages: Vec<PackageCheck>,
    /// The pack is about Python itself, so its rules run on every file.
    #[serde(default)]
    every_file: bool,
}

/// Which files a pack's rules run on.
pub(crate) enum Gate {
    /// Files that import the pack's module. The rule matches such an import.
    Import(Box<RuleConfig<SupportLang>>),
    /// Every file.
    EveryFile,
}

/// One library's rules.
pub(crate) struct Pack {
    /// The name the library is imported by, such as `pandas`.
    pub(crate) module: &'static str,
    gate: Gate,
    pub(crate) rules: Vec<Rule>,
    /// Short names a fix may write, each with the module it must be imported as, such as `pd`
    /// for `pandas`.
    pub(crate) aliases: BTreeMap<String, String>,
    /// Packages the library works better with.
    pub(crate) package_checks: Vec<PackageCheck>,
}

impl TryFrom<&PackSource> for Pack {
    type Error = Error;

    /// Parses the pack's rule files and its manifest.
    fn try_from(source: &PackSource) -> Result<Self> {
        let globals = GlobalRules::default();
        let mut rules = Vec::new();
        for file in source.rules {
            let context = || format!("{}.yml", file.id);
            for config in
                ast_grep_config::from_yaml_string(file.yaml, &globals).with_context(context)?
            {
                if config.severity != Severity::Off {
                    rules.push(Rule::try_from(config).with_context(context)?);
                }
            }
        }
        let manifest: Manifest = match source.manifest {
            Some(yaml) => ast_grep_config::from_str(yaml).context("pack.yml")?,
            None => Manifest::default(),
        };
        let gate = if manifest.every_file {
            Gate::EveryFile
        } else {
            let import = ast_grep_config::from_yaml_string(&gate_yaml(source.module), &globals)?
                .pop()
                .context("the import rule did not parse")?;
            Gate::Import(Box::new(import))
        };
        Ok(Self {
            module: source.module,
            gate,
            rules,
            aliases: manifest.aliases,
            package_checks: manifest.packages,
        })
    }
}

impl Pack {
    /// Whether the pack's rules can run on `src`, from its text alone. A file that never names
    /// the module cannot import it, so it needs no parse for this pack.
    pub(crate) fn may_apply(&self, src: &str) -> bool {
        match &self.gate {
            Gate::Import(_) => src.contains(self.module),
            Gate::EveryFile => true,
        }
    }

    /// Whether the pack's rules run on the parsed file. Rules for a library run only on files
    /// that import it, because they match names such as `merge` and `inplace` that other
    /// libraries share.
    pub(crate) fn applies(&self, root: &Root, src: &str) -> bool {
        match &self.gate {
            Gate::Import(import) => {
                src.contains(self.module) && root.root().find(&import.matcher).is_some()
            }
            Gate::EveryFile => true,
        }
    }

    /// Drops the rules that need a newer version of the library than `installed`.
    fn keep_rules_for(&mut self, installed: &Version) {
        self.rules.retain(|rule| rule.applies_to(installed));
    }
}

impl fmt::Debug for Pack {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Pack")
            .field("module", &self.module)
            .field("rules", &self.rules)
            .field("aliases", &self.aliases)
            .field("package_checks", &self.package_checks)
            .finish_non_exhaustive()
    }
}

/// Every pack, each with the rules for the `installed` version of its library. A pack whose
/// library is not installed, or a run without a venv, keeps every rule, so the project gets the
/// newest advice.
pub(crate) fn load(installed: Option<&Distributions>) -> Result<Vec<Pack>> {
    SOURCES
        .iter()
        .map(|source| {
            let mut pack =
                Pack::try_from(source).with_context(|| format!("rules/{}", source.module))?;
            if let Some(version) = installed.and_then(|found| found.version(pack.module)) {
                pack.keep_rules_for(version);
            }
            Ok(pack)
        })
        .collect()
}

/// An ast-grep rule matching `import <module>`, `import <module>.sub as x` and
/// `from <module> import x`. It checks only the imported module, so `from other import <module>`
/// and `import os as <module>` don't match.
fn gate_yaml(module: &str) -> String {
    format!(
        "id: {module}-import
language: python
rule:
  any:
    - kind: import_statement
      has:
        any:
          - kind: dotted_name
            regex: ^{module}(\\.|$)
          - kind: aliased_import
            has:
              field: name
              regex: ^{module}(\\.|$)
    - kind: import_from_statement
      has:
        field: module_name
        regex: ^{module}(\\.|$)
"
    )
}
