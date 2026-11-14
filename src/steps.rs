//! Reads pipelines of steps: functions that take a frame and return it with columns added.
//!
//! A pipeline is a function that passes one frame through other functions of the same module,
//! as `frame.pipe(a).pipe(b)` or as `frame = a(frame)` followed by `frame = b(frame)`. Each of
//! those functions is a step. A step reads columns with `frame["name"]` and adds them with
//! `.assign(name=...)`, with `frame.join(...)`, or with `frame["name"] = ...`. The names are
//! read from the source, so the columns each step needs from an earlier step are known without
//! running anything.

use std::collections::{BTreeMap, BTreeSet};

use ast_grep_core::Node;
use ast_grep_core::tree_sitter::StrDoc;
use ast_grep_language::SupportLang;

use crate::nodes::{Finding, Level, literal};
use crate::scan;

/// A node of the syntax tree of a Python file.
type Syntax<'r> = Node<'r, StrDoc<SupportLang>>;

/// Methods that keep the columns of the frame they are called on.
const KEEPS_COLUMNS: &[&str] = &["astype", "clip", "fillna", "mask", "round", "where"];

/// How deep a name is followed through the names it stands for.
const DEEPEST_NAME: usize = 16;

/// How much of a formula a message quotes.
const QUOTED_CHARS: usize = 90;

/// One pipeline and its steps, in the order they run.
#[derive(Debug)]
pub(crate) struct Pipeline {
    /// The function that runs the steps.
    pub(crate) name: String,
    pub(crate) line: usize,
    pub(crate) steps: Vec<Step>,
    pub(crate) links: Vec<Link>,
    pub(crate) findings: Vec<Finding>,
}

impl Pipeline {
    /// The steps that share no column with another step, so they can run anywhere. A step with
    /// computed names is left out, because not all of its columns are known.
    pub(crate) fn free_steps(&self) -> Vec<&str> {
        let shares = |a: &Step, b: &Step| {
            a.writes
                .iter()
                .any(|w| b.reads.contains(w) || b.writes.contains(w))
                || b.writes.iter().any(|w| a.reads.contains(w))
        };
        self.steps
            .iter()
            .filter(|step| !step.computed)
            .filter(|step| {
                !self
                    .steps
                    .iter()
                    .any(|other| other.name != step.name && shares(step, other))
            })
            .map(|step| step.name.as_str())
            .collect()
    }
}

/// One step of a pipeline.
#[derive(Debug)]
pub(crate) struct Step {
    pub(crate) name: String,
    pub(crate) line: usize,
    /// The columns the step reads by name.
    pub(crate) reads: Vec<String>,
    /// The columns the step adds by name.
    pub(crate) writes: Vec<String>,
    /// The step also reads or adds columns whose names the file computes.
    pub(crate) computed: bool,
    /// Names read as `frame.name`, which are columns when another step adds them.
    attributes: Vec<String>,
}

/// A step that reads columns an earlier step adds.
#[derive(Debug)]
pub(crate) struct Link {
    pub(crate) from: String,
    pub(crate) to: String,
    pub(crate) columns: Vec<String>,
}

/// How a step adds a column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Way {
    /// `frame.join(new)`, which raises when the column exists.
    Join,
    /// `frame.assign(name=...)` or a write to a copy, which replaces a column that exists.
    Replace,
    /// `frame["name"] = ...` on the frame the step was given.
    InPlace,
}

/// One column a step adds.
#[derive(Debug)]
struct Write {
    name: String,
    line: usize,
    way: Way,
}

/// One named formula in a step: a column it adds, or a local name.
#[derive(Debug)]
struct Formula {
    name: String,
    line: usize,
    is_column: bool,
    /// The formula with local names replaced and the terms of each sum sorted.
    text: String,
    /// The formula as the file writes it, on one line.
    source: String,
}

/// What a formula names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Column,
    Local,
}

/// What reading one step function has found so far.
#[derive(Debug, Default)]
struct Found {
    reads: BTreeSet<String>,
    writes: Vec<Write>,
    formulas: Vec<Formula>,
    findings: Vec<Finding>,
    /// The step reads or adds columns whose names the file computes.
    computed: bool,
}

/// What one step function does, before the steps are compared.
#[derive(Debug)]
struct StepBody {
    step: Step,
    writes: Vec<Write>,
    formulas: Vec<Formula>,
    findings: Vec<Finding>,
}

/// The pipelines of the module in `src`. Empty when the module has none.
pub(crate) fn read(src: &str) -> Vec<Pipeline> {
    let root = scan::parse(src);
    let module = root.root();
    let mut functions: BTreeMap<String, Syntax<'_>> = BTreeMap::new();
    let mut constants: BTreeMap<String, Syntax<'_>> = BTreeMap::new();
    for statement in module.children() {
        let statement = if statement.kind() == "decorated_definition" {
            statement.field("definition").unwrap_or(statement)
        } else {
            statement
        };
        if statement.kind() == "function_definition" {
            if let Some(name) = statement.field("name") {
                functions.insert(name.text().into_owned(), statement);
            }
        } else if let Some((name, value)) = simple_assignment(&statement) {
            constants.insert(name, value);
        }
    }
    let tree = docstring_tree(&module);
    let mut pipelines: Vec<Pipeline> = functions
        .iter()
        .filter_map(|(name, function)| {
            let order = step_order(function, &functions);
            (order.len() >= 2).then(|| pipeline(name, function, &order, &functions, &constants))
        })
        .collect();
    pipelines.sort_by_key(|p| p.line);
    if let ([only], Some(tree)) = (pipelines.as_mut_slice(), tree) {
        let ran: Vec<&str> = only.steps.iter().map(|s| s.name.as_str()).collect();
        if tree != ran {
            only.findings.push(Finding {
                line: only.line,
                level: Level::Warning,
                message: format!(
                    "The tree in the module docstring lists {} and {}() runs {}. Change the one that is out of date.",
                    tree.join(", "),
                    only.name,
                    ran.join(", ")
                ),
            });
        }
    }
    pipelines
}

/// `name = value` as a statement of its own.
fn simple_assignment<'r>(statement: &Syntax<'r>) -> Option<(String, Syntax<'r>)> {
    if statement.kind() != "expression_statement" {
        return None;
    }
    let assignment = statement.children().find(|c| c.kind() == "assignment")?;
    let left = assignment
        .field("left")
        .filter(|l| l.kind() == "identifier")?;
    Some((left.text().into_owned(), assignment.field("right")?))
}

/// The step names in a tree drawn in the module docstring, as in `├── step()  what it does`.
fn docstring_tree(module: &Syntax<'_>) -> Option<Vec<String>> {
    let first = module.children().find(Node::is_named)?;
    let docstring = first.children().find(|c| c.kind() == "string")?;
    let text = docstring.text();
    let names: Vec<String> = text
        .lines()
        .filter_map(|line| {
            let rest = line.split_once("── ")?.1;
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            (!name.is_empty() && rest[name.len()..].starts_with('(')).then_some(name)
        })
        .collect();
    (!names.is_empty()).then_some(names)
}

/// The module functions that `function` passes a frame through, in the order they run.
fn step_order(function: &Syntax<'_>, functions: &BTreeMap<String, Syntax<'_>>) -> Vec<String> {
    let own = function.field("name").map(|n| n.text().into_owned());
    let is_step = |name: &str| functions.contains_key(name) && own.as_deref() != Some(name);
    let mut found: Vec<(usize, String)> = Vec::new();
    let Some(body) = function.field("body") else {
        return Vec::new();
    };
    for call in body.dfs().filter(|n| n.kind() == "call") {
        let Some(target) = call.field("function") else {
            continue;
        };
        let first = call
            .field("arguments")
            .and_then(|a| a.children().find(Node::is_named));
        if target.kind() == "attribute" {
            // frame.pipe(step, ...)
            let piped = target
                .field("attribute")
                .is_some_and(|a| a.text() == "pipe");
            if let Some(step) = first.filter(|f| piped && f.kind() == "identifier")
                && is_step(&step.text())
            {
                found.push((step.range().start, step.text().into_owned()));
            }
        } else if target.kind() == "identifier" && is_step(&target.text()) {
            // step(frame) at the head of a pipe chain, or frame = step(frame).
            let heads_chain = call.parent().is_some_and(|p| {
                p.kind() == "attribute" && p.field("attribute").is_some_and(|a| a.text() == "pipe")
            });
            let reassigns = call.parent().is_some_and(|p| {
                let left = p.field("left").filter(|l| l.kind() == "identifier");
                let passed = first.as_ref().filter(|f| f.kind() == "identifier");
                p.kind() == "assignment"
                    && left.zip(passed).is_some_and(|(l, f)| l.text() == f.text())
            });
            if heads_chain || reassigns {
                found.push((target.range().start, target.text().into_owned()));
            }
        }
    }
    found.sort();
    let mut order: Vec<String> = Vec::new();
    for (_, name) in found {
        if !order.contains(&name) {
            order.push(name);
        }
    }
    order
}

/// Reads the steps of one pipeline and compares them.
fn pipeline<'r>(
    name: &str,
    function: &Syntax<'r>,
    order: &[String],
    functions: &BTreeMap<String, Syntax<'r>>,
    constants: &BTreeMap<String, Syntax<'r>>,
) -> Pipeline {
    let mut bodies: Vec<StepBody> = order
        .iter()
        .filter_map(|step| Reader::new(step, functions.get(step)?, constants).map(Reader::read))
        .collect();
    let added: BTreeSet<String> = bodies
        .iter()
        .flat_map(|b| b.writes.iter().map(|w| w.name.clone()))
        .collect();
    for body in &mut bodies {
        let step = &mut body.step;
        for name in step.attributes.iter().filter(|n| added.contains(*n)) {
            if !step.reads.contains(name) {
                step.reads.push(name.clone());
            }
        }
        step.reads.sort();
    }
    let mut findings: Vec<Finding> = Vec::new();
    let mut links: BTreeMap<(usize, usize), BTreeSet<String>> = BTreeMap::new();
    for (index, body) in bodies.iter().enumerate() {
        for column in &body.step.reads {
            let writers: Vec<(usize, &Write)> = bodies
                .iter()
                .enumerate()
                .flat_map(|(at, b)| b.writes.iter().map(move |w| (at, w)))
                .filter(|(at, w)| w.name == *column && *at != index)
                .collect();
            if let Some((from, _)) = writers.iter().rev().find(|(at, _)| *at < index) {
                links
                    .entry((*from, index))
                    .or_default()
                    .insert(column.clone());
            } else if let Some((later, write)) = writers.first()
                && write.way == Way::Join
            {
                // A joined column cannot exist before its step, so this read finds nothing.
                findings.push(Finding {
                    line: body.step.line,
                    level: Level::Error,
                    message: format!(
                        "{}() reads the column {column}, which {}() adds later in {name}(). The read raises KeyError. Move {}() after {}().",
                        body.step.name, bodies[*later].step.name, body.step.name, bodies[*later].step.name
                    ),
                });
            }
        }
        findings.extend(joined_again(index, &bodies));
    }
    findings.extend(repeated_formulas(&bodies));
    let links = links
        .into_iter()
        .map(|((from, to), columns)| Link {
            from: bodies[from].step.name.clone(),
            to: bodies[to].step.name.clone(),
            columns: columns.into_iter().collect(),
        })
        .collect();
    let mut steps = Vec::new();
    for body in bodies {
        findings.extend(body.findings);
        steps.push(body.step);
    }
    findings.sort_by_key(|f| (f.line, f.level));
    Pipeline {
        name: name.to_owned(),
        line: function.start_pos().line() + 1,
        steps,
        links,
        findings,
    }
}

/// A column that the step at `index` joins and an earlier step added already.
fn joined_again(index: usize, bodies: &[StepBody]) -> Vec<Finding> {
    let body = &bodies[index];
    let mut findings = Vec::new();
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for write in body.writes.iter().filter(|w| w.way == Way::Join) {
        let earlier = bodies[..index]
            .iter()
            .find(|b| b.writes.iter().any(|w| w.name == write.name));
        if let Some(earlier) = earlier.filter(|_| seen.insert(&write.name)) {
            findings.push(Finding {
                line: write.line,
                level: Level::Error,
                message: format!(
                    "{}() joins the column {}, which {}() added already. The join raises \"columns overlap\".",
                    body.step.name, write.name, earlier.step.name
                ),
            });
        }
    }
    findings
}

/// The same formula under two names, or written out in two steps.
fn repeated_formulas(bodies: &[StepBody]) -> Vec<Finding> {
    let mut by_text: BTreeMap<&str, Vec<(usize, &Formula)>> = BTreeMap::new();
    for (index, body) in bodies.iter().enumerate() {
        for formula in &body.formulas {
            by_text
                .entry(&formula.text)
                .or_default()
                .push((index, formula));
        }
    }
    let mut findings = Vec::new();
    for places in by_text.into_values() {
        let columns: Vec<&(usize, &Formula)> = places.iter().filter(|(_, f)| f.is_column).collect();
        let first_at = places[0].0;
        let step = |at: usize| bodies[at].step.name.as_str();
        if let Some((at, twin)) = columns.iter().find(|(_, f)| f.name != columns[0].1.name) {
            findings.push(Finding {
                line: twin.line,
                level: Level::Warning,
                message: format!(
                    "The column {} from {}() has the same formula as {} from {}(), so the result holds one column twice. Keep one of them.",
                    twin.name, step(*at), columns[0].1.name, step(columns[0].0)
                ),
            });
        } else if let Some((at, column)) = columns.first() {
            for (other, again) in places.iter().filter(|(o, f)| o != at && !f.is_column) {
                let advice = if at < other {
                    format!("Read the column {} there.", column.name)
                } else {
                    format!(
                        "{}() adds it as the column {} later.",
                        step(*at),
                        column.name
                    )
                };
                findings.push(Finding {
                    line: again.line,
                    level: Level::Warning,
                    message: format!(
                        "{}() computes {} with the formula of the column {} from {}(). {advice}",
                        step(*other),
                        again.name,
                        column.name,
                        step(*at)
                    ),
                });
            }
        } else if let Some((other, again)) = places.iter().find(|(at, _)| *at != first_at) {
            findings.push(Finding {
                line: again.line,
                level: Level::Warning,
                message: format!(
                    "{}() and {}() both compute {}. Compute it in one step and read it in the other.",
                    step(first_at), step(*other), quoted(&again.source)
                ),
            });
        }
    }
    findings
}

/// The start of a formula, for a message.
fn quoted(text: &str) -> String {
    if text.chars().count() <= QUOTED_CHARS {
        return text.to_owned();
    }
    let short: String = text.chars().take(QUOTED_CHARS).collect();
    format!("{short}...")
}

/// Reads one step function.
struct Reader<'r, 'a> {
    name: &'a str,
    function: &'a Syntax<'r>,
    constants: &'a BTreeMap<String, Syntax<'r>>,
    /// The frame parameter.
    frame: String,
    /// The frame parameter and the local names that stand for it or for a copy of it.
    frames: BTreeSet<String>,
    /// Local names with one assignment, and their values.
    locals: BTreeMap<String, Syntax<'r>>,
    /// The step gives the frame parameter a new value, so later writes go to that value.
    rebinds_frame: bool,
}

impl<'r, 'a> Reader<'r, 'a> {
    fn new(
        name: &'a str,
        function: &'a Syntax<'r>,
        constants: &'a BTreeMap<String, Syntax<'r>>,
    ) -> Option<Self> {
        let parameters = function.field("parameters")?;
        let frame = parameters
            .children()
            .filter(Node::is_named)
            .find_map(|p| match p.kind().as_ref() {
                "identifier" => Some(p.text().into_owned()),
                "typed_parameter" => p
                    .children()
                    .find(|c| c.kind() == "identifier")
                    .map(|c| c.text().into_owned()),
                "default_parameter" | "typed_default_parameter" => {
                    p.field("name").map(|n| n.text().into_owned())
                }
                _ => None,
            })
            .filter(|first| first != "self" && first != "cls")?;
        let body = function.field("body")?;
        let mut assigned: BTreeMap<String, Vec<Syntax<'r>>> = BTreeMap::new();
        for assignment in body.dfs().filter(|n| n.kind() == "assignment") {
            if let (Some(left), Some(right)) = (assignment.field("left"), assignment.field("right"))
                && left.kind() == "identifier"
            {
                assigned
                    .entry(left.text().into_owned())
                    .or_default()
                    .push(right);
            }
        }
        let rebinds_frame = assigned.contains_key(&frame);
        let mut frames = BTreeSet::from([frame.clone()]);
        let mut locals = BTreeMap::new();
        for (local, mut values) in assigned {
            let Some(value) = values.pop().filter(|_| values.is_empty()) else {
                continue;
            };
            let text = value.text();
            if local != frame && (text == frame || text == format!("{frame}.copy()")) {
                frames.insert(local);
            } else if local != frame {
                locals.insert(local, value);
            }
        }
        Some(Self {
            name,
            function,
            constants,
            frame,
            frames,
            locals,
            rebinds_frame,
        })
    }

    fn line(node: &Syntax<'_>) -> usize {
        node.start_pos().line() + 1
    }

    /// The column names an index stands for, or `None` when the file computes them.
    fn names(&self, node: &Syntax<'r>, depth: usize) -> Option<Vec<String>> {
        if depth > DEEPEST_NAME {
            return None;
        }
        match node.kind().as_ref() {
            "string" => literal(node).map(|name| vec![name]),
            "identifier" => {
                let name = node.text();
                let value = self
                    .locals
                    .get(name.as_ref())
                    .or_else(|| self.constants.get(name.as_ref()))?;
                self.names(value, depth + 1)
            }
            "parenthesized_expression" => {
                let inner = node.children().find(Node::is_named)?;
                self.names(&inner, depth + 1)
            }
            "list" => {
                let mut all = Vec::new();
                for item in node.children().filter(Node::is_named) {
                    all.extend(self.names(&item, depth + 1)?);
                }
                Some(all)
            }
            "binary_operator" if node.field("operator").is_some_and(|o| o.text() == "+") => {
                let mut all = self.names(&node.field("left")?, depth + 1)?;
                all.extend(self.names(&node.field("right")?, depth + 1)?);
                Some(all)
            }
            _ => None,
        }
    }

    /// The column an indexed frame names: `frame[...]`, or `frame.loc[..., column]`.
    fn frame_index(&self, subscript: &Syntax<'r>) -> Option<Syntax<'r>> {
        let value = subscript.field("value")?;
        let index = subscript.field("subscript")?;
        if value.kind() == "identifier" && self.frames.contains(value.text().as_ref()) {
            return Some(index);
        }
        let through_indexer = value.kind() == "attribute"
            && value
                .field("attribute")
                .is_some_and(|a| matches!(a.text().as_ref(), "loc" | "at"))
            && value
                .field("object")
                .is_some_and(|o| self.frames.contains(o.text().as_ref()));
        if !through_indexer {
            return None;
        }
        // frame.loc[rows, column]: the column is the last of the two indexes.
        subscript.field_children("subscript").last()
    }

    /// Whether an index picks rows, as a mask or a slice does, and so names no column.
    fn selects_rows(&self, index: &Syntax<'r>) -> bool {
        match index.kind().as_ref() {
            "slice"
            | "comparison_operator"
            | "boolean_operator"
            | "not_operator"
            | "unary_operator" => true,
            "binary_operator" => index
                .field("operator")
                .is_some_and(|o| matches!(o.text().as_ref(), "&" | "|" | "^")),
            "identifier" => self
                .locals
                .get(index.text().as_ref())
                .is_some_and(|value| self.selects_rows(value)),
            "parenthesized_expression" => index
                .children()
                .find(Node::is_named)
                .is_some_and(|inner| self.selects_rows(&inner)),
            _ => false,
        }
    }

    fn read(self) -> StepBody {
        let line = Self::line(self.function);
        let mut found = Found::default();
        if let Some(body) = self.function.field("body") {
            let targets = self.writes_in_place(&body, &mut found);
            self.column_reads(&body, &targets, &mut found);
            let joined = self.returned_columns(&body, &mut found);
            self.unjoined_columns(&body, &joined, &mut found);
            self.assigned_twice(line, &mut found);
        }
        // One formula per column name is enough to compare steps.
        let mut kept: BTreeSet<(String, String)> = BTreeSet::new();
        found
            .formulas
            .retain(|f| kept.insert((f.name.clone(), f.text.clone())));
        for (local, value) in &self.locals {
            found.formulas.push(self.formula(local, value, Kind::Local));
        }
        found.formulas.retain(|f| is_computation(&f.text));
        let mut names: Vec<String> = Vec::new();
        for write in &found.writes {
            if !names.contains(&write.name) {
                names.push(write.name.clone());
            }
        }
        StepBody {
            step: Step {
                name: self.name.to_owned(),
                line,
                reads: found.reads.into_iter().collect(),
                writes: names,
                computed: found.computed,
                attributes: self.attributes(),
            },
            writes: found.writes,
            formulas: found.formulas,
            findings: found.findings,
        }
    }

    /// Records `frame["name"] = value` and `frame.loc[rows, "name"] = value`, and returns where
    /// those targets start, so that they are not read as reads.
    fn writes_in_place(&self, body: &Syntax<'r>, found: &mut Found) -> BTreeSet<usize> {
        let mut targets: BTreeSet<usize> = BTreeSet::new();
        let mut warned = false;
        for assignment in body
            .dfs()
            .filter(|n| matches!(n.kind().as_ref(), "assignment" | "augmented_assignment"))
        {
            let Some(left) = assignment.field("left").filter(|l| l.kind() == "subscript") else {
                continue;
            };
            let Some(index) = self.frame_index(&left) else {
                continue;
            };
            targets.insert(left.range().start);
            let on_given = left
                .dfs()
                .find(|n| n.kind() == "identifier")
                .is_some_and(|n| n.text() == self.frame.as_str())
                && !self.rebinds_frame;
            if on_given && !warned {
                warned = true;
                found.findings.push(Finding {
                    line: Self::line(&assignment),
                    level: Level::Warning,
                    message: format!(
                        "{}() writes to the frame it was given. Called as {}({}), it changes the caller's frame too. Return {}.assign(...) or write to {}.copy().",
                        self.name, self.name, self.frame, self.frame, self.frame
                    ),
                });
            }
            let way = if on_given { Way::InPlace } else { Way::Replace };
            let Some(names) = self.names(&index, 0) else {
                found.computed = true;
                continue;
            };
            for name in names {
                if assignment.kind() == "augmented_assignment" {
                    found.reads.insert(name.clone());
                } else if let Some(value) = assignment.field("right") {
                    found
                        .formulas
                        .push(self.formula(&name, &value, Kind::Column));
                }
                found.writes.push(Write {
                    name,
                    line: Self::line(&assignment),
                    way,
                });
            }
        }
        targets
    }

    /// Records every `frame["name"]` that is no write target.
    fn column_reads(&self, body: &Syntax<'r>, targets: &BTreeSet<usize>, found: &mut Found) {
        for subscript in body.dfs().filter(|n| n.kind() == "subscript") {
            if targets.contains(&subscript.range().start) {
                continue;
            }
            let Some(index) = self.frame_index(&subscript) else {
                continue;
            };
            match self.names(&index, 0) {
                Some(names) => found.reads.extend(names),
                None => found.computed |= !self.selects_rows(&index),
            }
        }
        // frame.filter(...) picks columns by a pattern.
        found.computed |= body.dfs().any(|n| {
            n.kind() == "call"
                && n.field("function").is_some_and(|f| {
                    f.kind() == "attribute"
                        && f.field("attribute").is_some_and(|a| a.text() == "filter")
                        && f.field("object")
                            .is_some_and(|o| self.frames.contains(o.text().as_ref()))
                })
        });
    }

    /// Records the columns the returned frame gets from `.assign(...)` and `.join(...)`, and
    /// returns where those `.assign(...)` calls start.
    fn returned_columns(&self, body: &Syntax<'r>, found: &mut Found) -> BTreeSet<usize> {
        let mut joined: BTreeSet<usize> = BTreeSet::new();
        for returned in body
            .dfs()
            .filter(|n| n.kind() == "return_statement")
            .filter_map(|r| r.children().find(Node::is_named))
        {
            let computed = self.returned(&returned, &mut joined, found, 0);
            found.computed |= computed;
        }
        joined
    }

    /// A frame of new columns that never joins the result under those names.
    fn unjoined_columns(&self, body: &Syntax<'r>, joined: &BTreeSet<usize>, found: &mut Found) {
        for assign in body.dfs().filter(|n| is_method_call(n, "assign")) {
            if joined.contains(&assign.range().start) || !is_new_frame(&assign) {
                continue;
            }
            let names: Vec<String> = keywords(&assign)
                .into_iter()
                .map(|(name, _)| name)
                .filter(|name| !found.writes.iter().any(|w| w.name == *name))
                .collect();
            if !names.is_empty() {
                found.findings.push(Finding {
                    line: Self::line(&assign),
                    level: Level::Note,
                    message: format!(
                        "{}() assigns {} to a frame that it does not join to the result, so the result has no column of that name.",
                        self.name,
                        names.join(", ")
                    ),
                });
            }
            for (name, value) in keywords(&assign) {
                found
                    .formulas
                    .push(self.formula(&name, &value, Kind::Column));
            }
        }
    }

    /// A column assigned twice in one step with one formula. Two formulas are a value built in
    /// two stages, which is left alone.
    fn assigned_twice(&self, line: usize, found: &mut Found) {
        let mut counted: BTreeMap<(&str, &str), usize> = BTreeMap::new();
        for formula in found.formulas.iter().filter(|f| f.is_column) {
            *counted.entry((&formula.name, &formula.text)).or_default() += 1;
        }
        let mut findings = Vec::new();
        for ((column, text), times) in counted.into_iter().filter(|(_, times)| *times > 1) {
            let at = found
                .formulas
                .iter()
                .rfind(|f| f.is_column && f.name == column && f.text == text);
            findings.push(Finding {
                line: at.map_or(line, |f| f.line),
                level: Level::Warning,
                message: format!(
                    "{}() assigns the column {column} {times} times with the same formula. Assign it once.",
                    self.name
                ),
            });
        }
        found.findings.extend(findings);
    }

    /// The names read as `frame.name`, without the methods the step calls.
    fn attributes(&self) -> Vec<String> {
        let Some(body) = self.function.field("body") else {
            return Vec::new();
        };
        let names: BTreeSet<String> = body
            .dfs()
            .filter(|n| n.kind() == "attribute")
            .filter(|n| {
                n.field("object")
                    .is_some_and(|o| self.frames.contains(o.text().as_ref()))
            })
            .filter(|n| {
                !n.parent().is_some_and(|p| {
                    p.kind() == "call"
                        && p.field("function").is_some_and(|f| f.range() == n.range())
                })
            })
            .filter_map(|n| n.field("attribute").map(|a| a.text().into_owned()))
            .collect();
        names.into_iter().collect()
    }

    /// Walks the returned expression from its last method call back to the frame, and records
    /// the columns that `.assign(...)` and `.join(...)` add. Returns whether a part of it adds
    /// columns with names the file computes.
    fn returned(
        &self,
        node: &Syntax<'r>,
        joined: &mut BTreeSet<usize>,
        found: &mut Found,
        depth: usize,
    ) -> bool {
        if depth > DEEPEST_NAME {
            return true;
        }
        match node.kind().as_ref() {
            "parenthesized_expression" => node
                .children()
                .find(Node::is_named)
                .is_some_and(|inner| self.returned(&inner, joined, found, depth + 1)),
            "identifier" => {
                let name = node.text();
                if self.frames.contains(name.as_ref()) {
                    return false;
                }
                // A local name stands for its value. Any other name is a frame from elsewhere.
                self.locals
                    .get(name.as_ref())
                    .is_none_or(|value| self.returned(value, joined, found, depth + 1))
            }
            "call" => {
                let Some(target) = node.field("function").filter(|f| f.kind() == "attribute")
                else {
                    // step(frame) in a pipeline returns what that function returns.
                    return true;
                };
                let (Some(object), Some(method)) =
                    (target.field("object"), target.field("attribute"))
                else {
                    return true;
                };
                let mut computed = self.returned(&object, joined, found, depth + 1);
                match method.text().as_ref() {
                    "assign" => {
                        joined.insert(node.range().start);
                        self.record(node, Way::Replace, found);
                    }
                    "join" => {
                        let argument = node
                            .field("arguments")
                            .and_then(|a| a.children().find(Node::is_named));
                        computed |=
                            argument.is_none_or(|a| self.joined_frame(&a, joined, found, 0));
                    }
                    _ => {}
                }
                computed
            }
            _ => true,
        }
    }

    /// The frame passed to `.join(...)`: a frame of new columns from `.assign(...)`, or
    /// something whose column names the file computes.
    fn joined_frame(
        &self,
        node: &Syntax<'r>,
        joined: &mut BTreeSet<usize>,
        found: &mut Found,
        depth: usize,
    ) -> bool {
        if depth > DEEPEST_NAME {
            return true;
        }
        if node.kind() == "identifier" {
            return self
                .locals
                .get(node.text().as_ref())
                .is_none_or(|value| self.joined_frame(value, joined, found, depth + 1));
        }
        if node.kind() == "parenthesized_expression" {
            return node
                .children()
                .find(Node::is_named)
                .is_none_or(|inner| self.joined_frame(&inner, joined, found, depth + 1));
        }
        if is_method_call(node, "assign") && is_new_frame(node) {
            joined.insert(node.range().start);
            self.record(node, Way::Join, found);
            return false;
        }
        // frame.where(mask) and the like keep the columns of the frame they are called on.
        let kept = node
            .field("function")
            .filter(|f| f.kind() == "attribute")
            .filter(|f| {
                f.field("attribute")
                    .is_some_and(|a| KEEPS_COLUMNS.contains(&a.text().as_ref()))
            })
            .and_then(|f| f.field("object"));
        kept.is_none_or(|object| self.joined_frame(&object, joined, found, depth + 1))
    }

    /// Records the keyword names of one `.assign(...)` call as columns.
    fn record(&self, assign: &Syntax<'r>, way: Way, found: &mut Found) {
        for (name, value) in keywords(assign) {
            found
                .formulas
                .push(self.formula(&name, &value, Kind::Column));
            found.writes.push(Write {
                name,
                line: Self::line(&value),
                way,
            });
        }
    }

    fn formula(&self, name: &str, value: &Syntax<'r>, kind: Kind) -> Formula {
        Formula {
            name: name.to_owned(),
            line: Self::line(value),
            is_column: kind == Kind::Column,
            text: self.canonical(value, 0),
            source: value
                .text()
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" "),
        }
    }

    /// The formula as text that is equal for equal formulas: local names are replaced by what
    /// they stand for, a column read is written `col[name]`, and the terms of a sum are sorted.
    fn canonical(&self, node: &Syntax<'r>, depth: usize) -> String {
        if depth > DEEPEST_NAME {
            return node.text().into_owned();
        }
        match node.kind().as_ref() {
            "comment" => String::new(),
            "identifier" => match self.locals.get(node.text().as_ref()) {
                Some(value) => self.canonical(value, depth + 1),
                None => node.text().into_owned(),
            },
            "parenthesized_expression" => node
                .children()
                .find(Node::is_named)
                .map_or_else(String::new, |inner| self.canonical(&inner, depth + 1)),
            "subscript" => match self.frame_index(node).and_then(|i| self.names(&i, 0)) {
                Some(names) => format!("col[{}]", names.join(",")),
                None => self.joined_children(node, depth),
            },
            "binary_operator" => {
                let operator = node.field("operator").map(|o| o.text().into_owned());
                // a + b equals b + a, and a * b equals b * a, so their terms are sorted.
                if let Some(commutative) = operator.as_deref().filter(|o| matches!(*o, "+" | "*")) {
                    let mut terms = Vec::new();
                    self.terms(node, commutative, depth, &mut terms);
                    terms.sort();
                    return format!("({})", terms.join(&format!(" {commutative} ")));
                }
                match (node.field("left"), node.field("right"), operator) {
                    (Some(left), Some(right), Some(operator)) => format!(
                        "({} {operator} {})",
                        self.canonical(&left, depth + 1),
                        self.canonical(&right, depth + 1)
                    ),
                    _ => self.joined_children(node, depth),
                }
            }
            _ if node.is_leaf() || node.kind() == "string" => node.text().into_owned(),
            _ => self.joined_children(node, depth),
        }
    }

    fn joined_children(&self, node: &Syntax<'r>, depth: usize) -> String {
        node.children()
            .map(|child| self.canonical(&child, depth + 1))
            .collect()
    }

    /// The terms of `a + b + c`, or of `a * b * c`, each in canonical form.
    fn terms(&self, node: &Syntax<'r>, operator: &str, depth: usize, terms: &mut Vec<String>) {
        let inner = if node.kind() == "parenthesized_expression" {
            node.children().find(Node::is_named)
        } else {
            None
        };
        let node = inner.as_ref().unwrap_or(node);
        let local = (node.kind() == "identifier")
            .then(|| self.locals.get(node.text().as_ref()))
            .flatten();
        let node = local.unwrap_or(node);
        let same = node.kind() == "binary_operator"
            && node.field("operator").is_some_and(|o| o.text() == operator);
        match (same, node.field("left"), node.field("right")) {
            (true, Some(left), Some(right)) if depth <= DEEPEST_NAME => {
                self.terms(&left, operator, depth + 1, terms);
                self.terms(&right, operator, depth + 1, terms);
            }
            _ => terms.push(self.canonical(node, depth + 1)),
        }
    }
}

/// Whether `node` is the call `something.method(...)`.
fn is_method_call(node: &Syntax<'_>, method: &str) -> bool {
    node.kind() == "call"
        && node.field("function").is_some_and(|f| {
            f.kind() == "attribute" && f.field("attribute").is_some_and(|a| a.text() == method)
        })
}

/// Whether an `.assign(...)` call starts from a frame with no columns, as in
/// `pd.DataFrame(index=frame.index).assign(...)`.
fn is_new_frame(assign: &Syntax<'_>) -> bool {
    assign
        .field("function")
        .and_then(|f| f.field("object"))
        .is_some_and(|object| {
            object.kind() == "call"
                && object
                    .field("function")
                    .is_some_and(|f| f.text().rsplit('.').next() == Some("DataFrame"))
        })
}

/// The `name=value` arguments of a call.
fn keywords<'r>(call: &Syntax<'r>) -> Vec<(String, Syntax<'r>)> {
    let Some(arguments) = call.field("arguments") else {
        return Vec::new();
    };
    arguments
        .children()
        .filter(|a| a.kind() == "keyword_argument")
        .filter_map(|a| Some((a.field("name")?.text().into_owned(), a.field("value")?)))
        .collect()
}

/// Whether a formula computes something from a column. A bare column read does not.
fn is_computation(text: &str) -> bool {
    let reads = text.matches("col[").count();
    let bare = text.starts_with("col[") && text.ends_with(']') && reads == 1;
    reads > 0 && !bare
}
