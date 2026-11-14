//! Walks paths, fixes and lints each Python file with the rule packs that apply to it.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use ast_grep_core::source::Edit;
use ast_grep_core::tree_sitter::StrDoc;
use ast_grep_core::{AstGrep, NodeMatch};
use ast_grep_language::SupportLang;

use crate::pack::Pack;
use crate::report::{Issue, Position};
use crate::rule::{Applicability, Rule};

/// A parsed Python file.
pub(crate) type Root = AstGrep<StrDoc<SupportLang>>;

/// Parses Python source. A syntax error gives a tree with error nodes, not a failure.
pub(crate) fn parse(src: &str) -> Root {
    use ast_grep_core::tree_sitter::LanguageExt;

    SupportLang::Python.ast_grep(src)
}

/// One `import <module> as <alias>` in a file.
#[derive(Debug)]
struct AliasedImport {
    module: String,
    alias: String,
}

/// Whether `path` has the `.py` extension.
pub(crate) fn is_python(path: &Path) -> bool {
    path.extension()
        .is_some_and(|x| x.eq_ignore_ascii_case("py"))
}

fn cannot_read(path: &Path) -> String {
    format!("cannot read {}", path.display())
}

/// The Python files under `paths`, each once. Folders respect `.gitignore`.
pub(crate) fn collect_files(paths: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let mut seen = HashSet::new();
    let mut add = |path: &Path| -> Result<()> {
        let real = fs::canonicalize(path).with_context(|| cannot_read(path))?;
        if seen.insert(real) {
            files.push(path.to_path_buf());
        }
        Ok(())
    };
    for p in paths {
        let meta = fs::metadata(p).with_context(|| cannot_read(p))?;
        if meta.is_file() {
            add(p)?;
            continue;
        }
        for entry in ignore::WalkBuilder::new(p).build() {
            let entry = entry.with_context(|| cannot_read(p))?;
            if entry.file_type().is_some_and(|t| t.is_file()) && is_python(entry.path()) {
                add(entry.path())?;
            }
        }
    }
    Ok(files)
}

/// What a scan found, after any fixes it wrote.
#[derive(Debug)]
pub(crate) struct Scanned {
    pub(crate) issues: Vec<Issue>,
    /// Number of fixes written to disk.
    pub(crate) fixed: usize,
}

/// A fix can create a match for another fix, so fixing repeats until nothing changes, at
/// most this many times per file.
const MAX_FIX_PASSES: usize = 100;

/// Lints each file, after applying fixes up to `fix` when it is set and writing them back.
pub(crate) fn scan(
    files: &[PathBuf],
    packs: &[Pack],
    fix: Option<Applicability>,
) -> Result<Scanned> {
    use rayon::iter::{IntoParallelRefIterator, ParallelIterator};

    let per_file: Vec<(Vec<Issue>, usize)> = files
        .par_iter()
        .map(|path| {
            let mut src = fs::read_to_string(path).with_context(|| cannot_read(path))?;
            let mut fixed = 0;
            if let Some(allowed) = fix {
                for _ in 0..MAX_FIX_PASSES {
                    let (next, applied) = fix_pass(&src, packs, allowed);
                    if applied == 0 {
                        break;
                    }
                    src = next;
                    fixed += applied;
                }
                if fixed > 0 {
                    fs::write(path, &src)
                        .with_context(|| format!("cannot write {}", path.display()))?;
                }
            }
            Ok((lint(path, &src, packs), fixed))
        })
        .collect::<Result<_>>()?;
    let fixed = per_file.iter().map(|(_, n)| n).sum();
    let mut issues: Vec<Issue> = per_file.into_iter().flat_map(|(i, _)| i).collect();
    issues.sort_by(|a, b| (&a.file, a.position).cmp(&(&b.file, b.position)));
    Ok(Scanned { issues, fixed })
}

/// The packs whose rules run on the file.
fn applying<'p>(root: &'p Root, src: &'p str, packs: &'p [Pack]) -> impl Iterator<Item = &'p Pack> {
    packs.iter().filter(move |pack| pack.applies(root, src))
}

/// The issues in one file's source.
fn lint(path: &Path, src: &str, packs: &[Pack]) -> Vec<Issue> {
    if !packs.iter().any(|pack| pack.may_apply(src)) {
        return Vec::new();
    }
    let root = parse(src);
    let imports = aliased_imports(&root);
    let mut found = Vec::new();
    for pack in applying(&root, src, packs) {
        let unbound = unbound_aliases(pack, &imports);
        for rule in &pack.rules {
            for m in root.root().find_all(&rule.config.matcher) {
                let start = m.start_pos();
                let fix = rule
                    .fix
                    .filter(|_| usable_edit(rule, &m, &unbound, src).is_some());
                found.push(Issue {
                    package: pack.module,
                    rule: rule.config.id.clone(),
                    file: path.to_path_buf(),
                    position: Some(Position {
                        line: start.line() + 1,
                        column: start.column(&m) + 1,
                    }),
                    severity: rule.config.severity.clone(),
                    message: rule.config.get_message(&m),
                    fix,
                });
            }
        }
    }
    found
}

/// Applies every fix up to `allowed` that does not overlap an earlier one, and returns the new
/// source with the number of fixes applied. Overlapping fixes wait for the next pass.
fn fix_pass(src: &str, packs: &[Pack], allowed: Applicability) -> (String, usize) {
    if !packs.iter().any(|pack| pack.may_apply(src)) {
        return (src.to_owned(), 0);
    }
    let root = parse(src);
    let imports = aliased_imports(&root);
    let mut edits = Vec::new();
    for pack in applying(&root, src, packs) {
        let unbound = unbound_aliases(pack, &imports);
        for rule in &pack.rules {
            if rule.fix.is_none_or(|a| a > allowed) {
                continue;
            }
            for m in root.root().find_all(&rule.config.matcher) {
                edits.extend(usable_edit(rule, &m, &unbound, src));
            }
        }
    }
    edits.sort_by_key(|e| e.position);
    let mut out = Vec::with_capacity(src.len());
    let mut copied = 0;
    let mut applied = 0;
    for edit in edits {
        if edit.position < copied {
            continue;
        }
        out.extend_from_slice(&src.as_bytes()[copied..edit.position]);
        out.extend_from_slice(&edit.inserted_text);
        copied = edit.position + edit.deleted_length;
        applied += 1;
    }
    out.extend_from_slice(&src.as_bytes()[copied..]);
    let out = String::from_utf8(out).expect("fixes splice UTF-8 text at node boundaries");
    (out, applied)
}

/// Every aliased import in the file, at any depth.
fn aliased_imports(root: &Root) -> Vec<AliasedImport> {
    root.root()
        .dfs()
        .filter(|node| node.kind() == "aliased_import")
        .filter_map(|node| {
            Some(AliasedImport {
                module: node.field("name")?.text().into_owned(),
                alias: node.field("alias")?.text().into_owned(),
            })
        })
        .collect()
}

/// The pack's aliases that the file does not bind to their module. A fix that writes `pd.`
/// applies only in a file with `import pandas as pd`.
fn unbound_aliases<'p>(pack: &'p Pack, imports: &[AliasedImport]) -> Vec<&'p str> {
    pack.aliases
        .iter()
        .filter(|(alias, module)| {
            !imports
                .iter()
                .any(|import| import.module == **module && import.alias == **alias)
        })
        .map(|(alias, _)| alias.as_str())
        .collect()
}

/// The edit `rule` makes for `m` in `src`, or `None` when the rule has no fix, its fix writes
/// one of the `unbound` aliases, or its fix leaves the text as it is.
fn usable_edit(
    rule: &Rule,
    m: &NodeMatch<'_, StrDoc<SupportLang>>,
    unbound: &[&str],
    src: &str,
) -> Option<Edit<String>> {
    let fixer = rule.config.fixer.first()?;
    let edit = m.make_edit(&rule.config.matcher, fixer);
    let writes_unbound = unbound
        .iter()
        .any(|alias| writes_name(&edit.inserted_text, alias));
    let old = &src.as_bytes()[edit.position..edit.position + edit.deleted_length];
    (!writes_unbound && edit.inserted_text != old).then_some(edit)
}

/// Whether `text` uses `name` as the start of a dotted name, such as `pd.col`.
fn writes_name(text: &[u8], name: &str) -> bool {
    let needle = format!("{name}.");
    text.windows(needle.len()).enumerate().any(|(i, w)| {
        w == needle.as_bytes()
            && (i == 0
                || !(text[i - 1].is_ascii_alphanumeric() || matches!(text[i - 1], b'_' | b'.')))
    })
}
