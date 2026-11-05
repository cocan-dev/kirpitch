// SPDX-License-Identifier: GPL-3.0-or-later

//! A small S-expression (KiCad's bracketed text format) tree, writer and parser.
//!
//! The writer reproduces the layout KiCad itself writes (observed in KiCad
//! 10): one tab per level, atoms and strings on the line of the list name, every
//! child list on its own line, and a closing bracket on its own line only when
//! the list has child lists. The one exception is the point list of a symbol
//! graphic, whose points stay on one line; use [`List::inline_children`].
//!
//! The parser exists for the load check of ADR 0012: a generated file must
//! parse and print back to exactly the same text.

use kirpitch_core::format_number;
use thiserror::Error;

/// A node of an S-expression tree.
#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    /// An unquoted word such as `smd` or `20260206`.
    Atom(String),
    /// A quoted string such as `"F.Cu"`.
    Str(String),
    /// A bracketed list.
    List(List),
}

/// How the child lists of a [`List`] are laid out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Layout {
    /// Each child list on its own line.
    #[default]
    Expanded,
    /// All child lists on one line, as KiCad writes symbol point lists.
    InlineChildren,
}

/// A bracketed list: usually a name followed by values and child lists.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct List {
    /// The items between the brackets.
    pub items: Vec<Node>,
    /// Layout of the child lists.
    pub layout: Layout,
}

/// Starts a list whose first item is the unquoted `name`.
pub fn list(name: &str) -> List {
    List::default().atom(name)
}

impl List {
    /// Appends an unquoted word. Panics when the word is empty or contains
    /// whitespace, brackets or quotes, because that is a programming error.
    pub fn atom(mut self, word: &str) -> List {
        assert!(is_valid_atom(word), "invalid S-expression atom {word:?}");
        self.items.push(Node::Atom(word.to_string()));
        self
    }

    /// Appends a quoted string.
    pub fn string(mut self, text: &str) -> List {
        self.items.push(Node::Str(text.to_string()));
        self
    }

    /// Appends a number in the canonical format of ADR 0010.
    pub fn num(mut self, value: f64) -> List {
        self.items.push(Node::Atom(format_number(value)));
        self
    }

    /// Appends `yes` or `no`.
    pub fn yes_no(self, value: bool) -> List {
        self.atom(if value { "yes" } else { "no" })
    }

    /// Appends a child list.
    pub fn child(mut self, child: List) -> List {
        self.items.push(Node::List(child));
        self
    }

    /// Appends a child list when `child` is `Some`.
    pub fn child_opt(self, child: Option<List>) -> List {
        match child {
            Some(c) => self.child(c),
            None => self,
        }
    }

    /// Puts all child lists on one line.
    pub fn inline_children(mut self) -> List {
        self.layout = Layout::InlineChildren;
        self
    }

    /// The name: the first item when it is an atom.
    pub fn name(&self) -> Option<&str> {
        match self.items.first() {
            Some(Node::Atom(a)) => Some(a),
            _ => None,
        }
    }

    /// Writes the list as KiCad text with a final newline.
    pub fn write(&self) -> String {
        let mut out = String::new();
        write_list(self, 0, &mut out);
        out.push('\n');
        out
    }
}

impl From<List> for Node {
    fn from(list: List) -> Node {
        Node::List(list)
    }
}

fn is_valid_atom(word: &str) -> bool {
    !word.is_empty()
        && !word
            .chars()
            .any(|c| c.is_whitespace() || matches!(c, '(' | ')' | '"'))
}

fn write_string(text: &str, out: &mut String) {
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c => out.push(c),
        }
    }
    out.push('"');
}

fn indent(level: usize, out: &mut String) {
    for _ in 0..level {
        out.push('\t');
    }
}

fn write_scalar(node: &Node, out: &mut String) {
    match node {
        Node::Atom(a) => out.push_str(a),
        Node::Str(s) => write_string(s, out),
        Node::List(_) => unreachable!("lists are written by write_list"),
    }
}

fn write_list(list: &List, level: usize, out: &mut String) {
    out.push('(');
    let has_children = list.items.iter().any(|n| matches!(n, Node::List(_)));
    let mut seen_child = false;
    let mut first = true;
    let mut inline_open = false;
    for item in &list.items {
        match item {
            Node::List(child) => {
                if list.layout == Layout::InlineChildren {
                    if !inline_open {
                        out.push('\n');
                        indent(level + 1, out);
                        inline_open = true;
                    } else {
                        out.push(' ');
                    }
                } else {
                    out.push('\n');
                    indent(level + 1, out);
                }
                write_list(child, level + 1, out);
                seen_child = true;
            }
            scalar => {
                if first {
                    // The first item (normally the name) follows the bracket.
                } else if seen_child {
                    out.push('\n');
                    indent(level + 1, out);
                } else {
                    out.push(' ');
                }
                write_scalar(scalar, out);
            }
        }
        first = false;
    }
    if has_children {
        out.push('\n');
        indent(level, out);
    }
    out.push(')');
}

/// A problem found while parsing S-expression text.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ParseError {
    /// A string was opened but never closed.
    #[error("line {line}: unterminated string")]
    UnterminatedString {
        /// Line where the string starts.
        line: usize,
    },
    /// A bracket was opened but never closed.
    #[error("line {line}: unclosed bracket")]
    Unclosed {
        /// Line of the opening bracket.
        line: usize,
    },
    /// A closing bracket without an opening one.
    #[error("line {line}: unexpected closing bracket")]
    Unexpected {
        /// Line of the bracket.
        line: usize,
    },
    /// Text after the first complete list, or no list at all.
    #[error("line {line}: expected exactly one top-level list")]
    TopLevel {
        /// Line of the problem.
        line: usize,
    },
}

/// Parses text that holds exactly one top-level list.
///
/// Both `\n` and `\r\n` line endings are accepted. The layout of a list is set
/// to [`Layout::InlineChildren`] when it has two or more child lists that all
/// start on the same line, which is how KiCad writes symbol point lists.
pub fn parse(text: &str) -> Result<List, ParseError> {
    let mut p = Parser {
        chars: text.chars().collect(),
        pos: 0,
        line: 1,
    };
    p.skip_space();
    if p.peek() != Some('(') {
        return Err(ParseError::TopLevel { line: p.line });
    }
    let list = p.list()?;
    p.skip_space();
    if p.peek().is_some() {
        return Err(ParseError::TopLevel { line: p.line });
    }
    Ok(list)
}

struct Parser {
    chars: Vec<char>,
    pos: usize,
    line: usize,
}

impl Parser {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += 1;
        if c == '\n' {
            self.line += 1;
        }
        Some(c)
    }

    fn skip_space(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.bump();
        }
    }

    fn list(&mut self) -> Result<List, ParseError> {
        let open_line = self.line;
        self.bump(); // '('
        let mut items = Vec::new();
        let mut child_lines: Vec<usize> = Vec::new();
        loop {
            self.skip_space();
            match self.peek() {
                None => return Err(ParseError::Unclosed { line: open_line }),
                Some(')') => {
                    self.bump();
                    break;
                }
                Some('(') => {
                    let start_line = self.line;
                    let child = self.list()?;
                    child_lines.push(start_line);
                    items.push(Node::List(child));
                }
                Some('"') => items.push(Node::Str(self.string()?)),
                Some(_) => items.push(Node::Atom(self.atom())),
            }
        }
        let inline = child_lines.len() >= 2
            && child_lines.iter().all(|l| *l == child_lines[0])
            && child_lines[0] != open_line;
        let layout = if inline {
            Layout::InlineChildren
        } else {
            Layout::Expanded
        };
        Ok(List { items, layout })
    }

    fn string(&mut self) -> Result<String, ParseError> {
        let start = self.line;
        self.bump(); // opening quote
        let mut s = String::new();
        loop {
            match self.bump() {
                None => return Err(ParseError::UnterminatedString { line: start }),
                Some('"') => return Ok(s),
                Some('\\') => match self.bump() {
                    Some('n') => s.push('\n'),
                    Some('r') => s.push('\r'),
                    Some(c) => s.push(c),
                    None => return Err(ParseError::UnterminatedString { line: start }),
                },
                Some(c) => s.push(c),
            }
        }
    }

    fn atom(&mut self) -> String {
        let mut s = String::new();
        while let Some(c) = self.peek() {
            if c.is_whitespace() || c == '(' || c == ')' || c == '"' {
                break;
            }
            s.push(c);
            self.bump();
        }
        s
    }
}

/// Checks that `text` parses and prints back to exactly the same text (with
/// `\r\n` read as `\n`). Returns the parsed list.
pub fn check_round_trip(text: &str) -> Result<List, RoundTripError> {
    let list = parse(text)?;
    let again = list.write();
    let original = text.replace("\r\n", "\n");
    if again == original {
        Ok(list)
    } else {
        let line = original
            .lines()
            .zip(again.lines())
            .position(|(a, b)| a != b)
            .map_or_else(
                || original.lines().count().min(again.lines().count()) + 1,
                |i| i + 1,
            );
        Err(RoundTripError::Differs { line })
    }
}

/// A failure of [`check_round_trip`].
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RoundTripError {
    /// The text does not parse.
    #[error(transparent)]
    Parse(#[from] ParseError),
    /// The text parses but is not in canonical layout.
    #[error("not in canonical layout; first difference on line {line}")]
    Differs {
        /// First line that differs.
        line: usize,
    },
}
