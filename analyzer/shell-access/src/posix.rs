//! Walk a POSIX shell command and record which files it attempted to touch.
//!
//! The tree is used for syntax only. Every attribution decision comes from
//! [`crate::table`] or from a structural rule stated here, and every path is the
//! operand text as the command wrote it.

use std::path::Path;
use tree_sitter::{Node, Parser};

use crate::table::{self, FlagValue, Positionals, Utility, Wrapper};
use crate::{AccessOperation, AccessTarget, AmbiguityReason, ClassificationError, FileAccess};

/// Bounds recursive wrapper and `sh -c` classification. Beyond this depth, stop
/// attribution rather than guess.
const MAX_NESTING: usize = 8;

type Record = Result<FileAccess, ClassificationError>;

pub(crate) fn classify(command: &str, cwd: Option<&Path>) -> Vec<Record> {
    let mut walk = Walk {
        cwd,
        out: Vec::new(),
    };
    walk.read(command, 0);
    walk.out
}

struct Walk<'c> {
    cwd: Option<&'c Path>,
    out: Vec<Record>,
}

/// What a command name turned out to name, once wrappers were peeled off.
enum Resolved<'t> {
    /// A utility the table knows, with the operands that belong to it.
    Known(&'static Utility, Vec<Node<'t>>),
    /// A shell was handed a command string to read on its own.
    Nested(String),
    /// Nothing the table knows, which contributes no rows.
    Unattributed,
}

impl Walk<'_> {
    fn read(&mut self, source: &str, depth: usize) {
        if depth > MAX_NESTING {
            return;
        }
        let mut parser = Parser::new();
        if parser
            .set_language(&tree_sitter_bash::LANGUAGE.into())
            .is_err()
        {
            self.out.push(Err(ClassificationError::Parse));
            return;
        }
        let Some(tree) = parser.parse(source, None) else {
            self.out.push(Err(ClassificationError::Parse));
            return;
        };
        self.node(tree.root_node(), source, depth);
    }

    fn node(&mut self, node: Node, source: &str, depth: usize) {
        // An unreadable fragment is reported as unreadable, at its own place in
        // the stream, rather than attributed from whatever words survived it.
        if node.is_error() || node.is_missing() {
            self.out.push(Err(ClassificationError::Parse));
            return;
        }
        match node.kind() {
            // Redirects hang off the statement rather than the command, and
            // belong to the last command inside it. Operands are recorded
            // first, so one command reads in the order its own text does.
            "redirected_statement" => {
                let body = node.child_by_field_name("body");
                if let Some(body) = body {
                    self.node(body, source, depth);
                }
                let scripting = body.is_some_and(|body| self.body_is_scripting(body, source));
                let mut cursor = node.walk();
                let redirects: Vec<Node> = node
                    .children_by_field_name("redirect", &mut cursor)
                    .collect();
                for redirect in redirects {
                    self.redirect(redirect, source, scripting);
                }
            }
            "command" => self.command(node, source, depth),
            _ => {
                let mut cursor = node.walk();
                let children: Vec<Node> = node.named_children(&mut cursor).collect();
                for child in children {
                    self.node(child, source, depth);
                }
            }
        }
    }

    fn command(&mut self, node: Node, source: &str, depth: usize) {
        let name = command_name(node, source).unwrap_or_default();
        match self.resolve(&name, operands(node), source) {
            Resolved::Known(utility, operands) => {
                self.attribute(utility, &operands, source, false);
            }
            Resolved::Nested(inner) => self.read(&inner, depth + 1),
            Resolved::Unattributed => {}
        }
        // A substitution's own commands are commands, whether or not the
        // utility around them is one the table knows.
        self.substitutions(node, source, depth);
    }

    /// Peels wrappers until a name the table can answer for, or until nothing
    /// answers. `sudo -u builder rm -f build/stale.env` resolves to `rm` with
    /// `-f build/stale.env`.
    fn resolve<'t>(&self, name: &str, operands: Vec<Node<'t>>, source: &str) -> Resolved<'t> {
        let mut name = name.to_string();
        let mut operands = operands;
        for _ in 0..MAX_NESTING {
            match table::wrapper(&name) {
                Some(Wrapper::Prefix {
                    value_flags,
                    skip_operands,
                }) => {
                    let Some((inner, rest)) = peel(&operands, source, value_flags, *skip_operands)
                    else {
                        return Resolved::Unattributed;
                    };
                    name = inner;
                    operands = rest;
                }
                // A shell invoked with no `-c` string runs a script file this
                // crate does not open.
                Some(Wrapper::CommandString) => {
                    return match command_string(&operands, source) {
                        Some(inner) => Resolved::Nested(inner),
                        None => Resolved::Unattributed,
                    };
                }
                None => {
                    return match table::lookup(&name) {
                        Some(utility) => Resolved::Known(utility, operands),
                        None => Resolved::Unattributed,
                    };
                }
            }
        }
        Resolved::Unattributed
    }

    fn attribute(
        &mut self,
        utility: &'static Utility,
        operands: &[Node],
        source: &str,
        scripting: bool,
    ) {
        let mut positionals = utility.positionals;
        let mut in_place = false;
        let mut flags_over = false;
        let mut paths = Vec::new();
        let mut index = 0;

        while index < operands.len() {
            let operand = operands[index];
            index += 1;
            let text = literal(operand, source);
            if flags_over || !is_flag(&text) {
                if utility.expression && opens_expression(&text) {
                    break;
                }
                paths.push(operand);
                continue;
            }
            if text == "--" {
                flags_over = true;
                continue;
            }

            let (flag_text, attached) = split_flag(&text);
            let Some(spec) = table::find_flag(utility.flags, flag_text) else {
                if utility.expression {
                    // The utility's expression starts here, and its words are
                    // tests and arguments rather than names.
                    break;
                }
                // The flag itself is not a path, and the operands after it are
                // still read under this utility's policy.
                continue;
            };

            if let Some(replacement) = spec.positionals {
                positionals = replacement;
            }
            if spec.in_place {
                in_place = true;
                // BSD spells the in-place suffix as a separate empty word
                // (`sed -i '' …`) where GNU attaches it. Left unconsumed it
                // takes a positional slot and shifts every path after it.
                if attached.is_none() && next_is_empty(operands, index, source) {
                    index += 1;
                }
            }

            match spec.value {
                FlagValue::None => {}
                FlagValue::Ignored => {
                    if attached.is_none() {
                        index += 1;
                    }
                }
                FlagValue::ModeChanging(replacement) => {
                    positionals = replacement;
                    if attached.is_none() {
                        index += 1;
                    }
                }
                FlagValue::Read => {
                    if let Some((path, ambiguity)) =
                        flag_value_path(operand, attached, operands, &mut index, source)
                    {
                        self.record_path(
                            path,
                            AccessOperation::Read,
                            AccessTarget::File,
                            scripting,
                            ambiguity,
                        );
                    }
                }
                FlagValue::Write => {
                    if let Some((path, ambiguity)) =
                        flag_value_path(operand, attached, operands, &mut index, source)
                    {
                        self.record_path(
                            path,
                            AccessOperation::Write,
                            AccessTarget::File,
                            scripting,
                            ambiguity,
                        );
                    }
                }
                FlagValue::WriteDirectory => {
                    if let Some((path, ambiguity)) =
                        flag_value_path(operand, attached, operands, &mut index, source)
                    {
                        self.record_path(
                            path,
                            AccessOperation::Write,
                            AccessTarget::Directory,
                            scripting,
                            ambiguity,
                        );
                    }
                }
            }
        }

        let total = paths.len();
        for (position, operand) in paths.into_iter().enumerate() {
            if let Some((op, target)) = operation(positionals, position, total, in_place) {
                self.record(operand, source, op, target, scripting);
            }
        }
    }

    fn redirect(&mut self, node: Node, source: &str, scripting: bool) {
        match node.kind() {
            "file_redirect" => {
                let Some(op) = redirect_operation(node, source) else {
                    return;
                };
                // tree-sitter-bash issue #233: `file_redirect` accepts repeated
                // destinations, so `printf > dest extra` parses with two. Only
                // the first is a file the shell would have opened.
                let Some(destination) = node.child_by_field_name("destination") else {
                    return;
                };
                // A descriptor is not a file: `2>&1` must never report a path
                // named `2` or `1`.
                if matches!(destination.kind(), "number" | "file_descriptor") {
                    return;
                }
                self.record(destination, source, op, AccessTarget::File, scripting);
            }
            "heredoc_redirect" => {
                if let Some(fragment) = expanded_heredoc(node, source) {
                    self.out.push(Ok(FileAccess {
                        op: AccessOperation::Read,
                        target: AccessTarget::File,
                        path: fragment,
                        cwd: self.cwd.map(Path::to_path_buf),
                        ambiguity: Some(AmbiguityReason::ExpandedHeredoc),
                        scripting,
                    }));
                }
                // A heredoc can carry its own output redirect, which is an
                // ordinary literal destination.
                let mut cursor = node.walk();
                let nested: Vec<Node> = node
                    .named_children(&mut cursor)
                    .filter(|child| child.kind() == "file_redirect")
                    .collect();
                for child in nested {
                    self.redirect(child, source, scripting);
                }
            }
            _ => {}
        }
    }

    fn record(
        &mut self,
        operand: Node,
        source: &str,
        op: AccessOperation,
        target: AccessTarget,
        scripting: bool,
    ) {
        // A number is a count or a descriptor, never a name this crate reports.
        if operand.kind() == "number" {
            return;
        }
        let path = literal(operand, source);
        if path.is_empty() {
            return;
        }
        self.record_path(path, op, target, scripting, ambiguity(operand, source));
    }

    fn record_path(
        &mut self,
        path: String,
        op: AccessOperation,
        target: AccessTarget,
        scripting: bool,
        ambiguity: Option<AmbiguityReason>,
    ) {
        if path.is_empty() {
            return;
        }
        // A utility's operand policy says what it does with a word; the word's
        // own spelling can still say the word is a directory, and `rg TODO .`
        // is the shape where only the second half knows.
        let target = match target {
            AccessTarget::File => AccessTarget::of_path(&path),
            settled => settled,
        };
        self.out.push(Ok(FileAccess {
            op,
            target,
            path,
            cwd: self.cwd.map(Path::to_path_buf),
            ambiguity,
            scripting,
        }));
    }

    /// True when the command a redirect belongs to is an inline interpreter, so
    /// its literal destinations are still recorded but marked as scripted.
    fn body_is_scripting(&self, body: Node, source: &str) -> bool {
        let Some(command) = last_command(body) else {
            return false;
        };
        let name = command_name(command, source).unwrap_or_default();
        matches!(
            self.resolve(&name, operands(command), source),
            Resolved::Known(utility, _) if utility.scripting
        )
    }

    fn substitutions(&mut self, node: Node, source: &str, depth: usize) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.named_children(&mut cursor).collect();
        for child in children {
            if child.kind() == "command_substitution" || child.kind() == "process_substitution" {
                let mut inner = child.walk();
                let statements: Vec<Node> = child.named_children(&mut inner).collect();
                for statement in statements {
                    self.node(statement, source, depth);
                }
            } else {
                self.substitutions(child, source, depth);
            }
        }
    }
}

/// The value a file-valued flag took: the attached `--flag=value` form, or the
/// next operand when the forms are separate. The reason a value stayed a
/// fragment travels with it, so `sort -o $OUT` is as ambiguous here as
/// `tee $OUT` is as a positional.
fn flag_value_path<'t>(
    flag: Node<'t>,
    attached: Option<&str>,
    operands: &[Node<'t>],
    index: &mut usize,
    source: &str,
) -> Option<(String, Option<AmbiguityReason>)> {
    if let Some(attached) = attached {
        return Some((attached.to_string(), ambiguity(flag, source)));
    }
    let operand = *operands.get(*index)?;
    *index += 1;
    let path = literal(operand, source);
    if path.is_empty() {
        None
    } else {
        Some((path, ambiguity(operand, source)))
    }
}

/// True when the next operand is an empty word, which only a quoted `''` or
/// `""` can be.
fn next_is_empty(operands: &[Node], index: usize, source: &str) -> bool {
    operands
        .get(index)
        .is_some_and(|operand| literal(*operand, source).is_empty())
}

fn split_flag(text: &str) -> (&str, Option<&str>) {
    if text.starts_with("--") || (text.starts_with('-') && text.contains('=')) {
        if let Some((name, value)) = text.split_once('=') {
            return (name, Some(value));
        }
    }
    (text, None)
}

fn operation(
    positionals: Positionals,
    position: usize,
    total: usize,
    in_place: bool,
) -> Option<(AccessOperation, AccessTarget)> {
    let (op, target) = match positionals {
        Positionals::Read => (AccessOperation::Read, AccessTarget::File),
        Positionals::ScriptThenRead if position == 0 => return None,
        Positionals::ScriptThenRead => (AccessOperation::Read, AccessTarget::File),
        // A lone operand cannot be a destination: with nothing to copy into it,
        // it is the source and the destination was left implicit.
        Positionals::ReadThenWriteLast if total > 1 && position + 1 == total => {
            (AccessOperation::Write, AccessTarget::File)
        }
        Positionals::ReadThenWriteLast => (AccessOperation::Read, AccessTarget::File),
        Positionals::Write => (AccessOperation::Write, AccessTarget::File),
        Positionals::Delete => (AccessOperation::Delete, AccessTarget::File),
        Positionals::ReadDirectory => (AccessOperation::Read, AccessTarget::Directory),
        Positionals::WriteDirectory => (AccessOperation::Write, AccessTarget::Directory),
        Positionals::DeleteDirectory => (AccessOperation::Delete, AccessTarget::Directory),
        Positionals::None => return None,
    };
    Some((
        if in_place && op == AccessOperation::Read {
            AccessOperation::Write
        } else {
            op
        },
        target,
    ))
}

fn redirect_operation(node: Node, source: &str) -> Option<AccessOperation> {
    let mut cursor = node.walk();
    let operator = node
        .children(&mut cursor)
        .find(|child| !child.is_named())
        .map(|child| &source[child.byte_range()])?;
    match operator {
        ">" | ">>" | ">|" | "&>" | "&>>" => Some(AccessOperation::Write),
        "<" | "<>" => Some(AccessOperation::Read),
        // `>&` and `<&` name a file descriptor, not a file.
        _ => None,
    }
}

/// The fragment to report for a heredoc whose delimiter is unquoted, meaning the
/// shell expands the body before any utility sees it. A quoted delimiter makes
/// the body literal, and a literal body names no file, so it yields nothing.
fn expanded_heredoc(node: Node, source: &str) -> Option<String> {
    let mut named = node.walk();
    let start = node
        .named_children(&mut named)
        .find(|child| child.kind() == "heredoc_start")?;
    let delimiter = &source[start.byte_range()];
    if delimiter.starts_with('\'') || delimiter.starts_with('"') {
        return None;
    }
    let mut all = node.walk();
    let operator = node
        .children(&mut all)
        .find(|child| !child.is_named())
        .map(|child| &source[child.byte_range()])
        .unwrap_or("<<");
    Some(format!("{operator}{delimiter}"))
}

fn operands<'t>(command: Node<'t>) -> Vec<Node<'t>> {
    let mut cursor = command.walk();
    command
        .named_children(&mut cursor)
        .filter(|child| {
            !matches!(
                child.kind(),
                "command_name" | "variable_assignment" | "file_redirect" | "heredoc_redirect"
            )
        })
        .collect()
}

/// The utility a command invokes, by its last path component so `/usr/bin/cat`
/// answers as `cat`. `None` when the name is itself an expansion, which names
/// nothing this crate can look up.
fn command_name(command: Node, source: &str) -> Option<String> {
    let name = command.child_by_field_name("name")?;
    let text = literal(name, source);
    if text.is_empty() || text.contains('$') {
        return None;
    }
    Some(basename(&text))
}

fn basename(text: &str) -> String {
    text.rsplit('/')
        .find(|part| !part.is_empty())
        .unwrap_or(text)
        .to_string()
}

fn last_command<'t>(node: Node<'t>) -> Option<Node<'t>> {
    if node.kind() == "command" {
        return Some(node);
    }
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .filter_map(last_command)
        .last()
}

fn peel<'t>(
    operands: &[Node<'t>],
    source: &str,
    value_flags: &[&str],
    mut skip_operands: usize,
) -> Option<(String, Vec<Node<'t>>)> {
    let mut index = 0;
    while index < operands.len() {
        let text = literal(operands[index], source);
        index += 1;
        if is_flag(&text) {
            let (flag_text, attached) = split_flag(&text);
            if flag_text != "--" && value_flags.contains(&flag_text) && attached.is_none() {
                index += 1;
            }
            continue;
        }
        // `VAR=value` in front of the command belongs to the environment.
        if !text.starts_with('=') && text.contains('=') {
            continue;
        }
        if skip_operands > 0 {
            skip_operands -= 1;
            continue;
        }
        return Some((basename(&text), operands[index..].to_vec()));
    }
    None
}

/// The command string a shell's `-c` was handed. Combined flags count, so
/// `bash -lc '…'` is found as well as `sh -c '…'`.
fn command_string(operands: &[Node], source: &str) -> Option<String> {
    let mut index = 0;
    while index < operands.len() {
        let text = literal(operands[index], source);
        index += 1;
        if is_flag(&text) && !text.starts_with("--") && text.contains('c') {
            return operands.get(index).map(|node| literal(*node, source));
        }
    }
    None
}

fn is_flag(text: &str) -> bool {
    text.starts_with('-') && text.len() > 1
}

/// Whether a word that is not flag-shaped nonetheless begins an expression.
/// `find`'s grouping and negation operators have to be escaped or quoted to
/// survive the shell, so they arrive as ordinary words; reading `\(` as a root
/// would report a directory no command named.
fn opens_expression(text: &str) -> bool {
    matches!(text.trim_start_matches('\\'), "(" | ")" | "!" | ",")
}

fn ambiguity(operand: Node, source: &str) -> Option<AmbiguityReason> {
    if contains_kind(operand, "command_substitution")
        || contains_kind(operand, "process_substitution")
    {
        return Some(AmbiguityReason::CommandSubstitution);
    }
    if contains_kind(operand, "expansion")
        || contains_kind(operand, "simple_expansion")
        || contains_kind(operand, "arithmetic_expansion")
    {
        return Some(AmbiguityReason::Expansion);
    }
    // Only an unquoted word globs; the same characters inside quotes are the
    // name the shell would have used.
    if matches!(operand.kind(), "word" | "concatenation") {
        let text = &source[operand.byte_range()];
        if text.contains('*') || text.contains('?') || text.contains('[') {
            return Some(AmbiguityReason::Glob);
        }
    }
    None
}

fn contains_kind(node: Node, kind: &str) -> bool {
    if node.kind() == kind {
        return true;
    }
    let mut cursor = node.walk();
    let children: Vec<Node> = node.named_children(&mut cursor).collect();
    children.iter().any(|child| contains_kind(*child, kind))
}

/// The operand as the command wrote it, with one layer of surrounding quotes
/// removed. Nothing else is resolved: a relative path stays relative, and a
/// fragment stays a fragment.
fn literal(node: Node, source: &str) -> String {
    let text = &source[node.byte_range()];
    for quote in ['\'', '"'] {
        if text.len() >= 2 && text.starts_with(quote) && text.ends_with(quote) {
            return text[1..text.len() - 1].to_string();
        }
    }
    text.to_string()
}
