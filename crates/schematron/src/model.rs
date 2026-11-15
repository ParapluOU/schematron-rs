//! The ISO Schematron document model (ISO/IEC 19757-3).

/// `sch:ns` — a namespace binding made available to every XPath expression.
#[derive(Debug, Clone)]
pub struct Ns {
    pub prefix: String,
    pub uri: String,
}

/// `sch:let` — a named variable bound to the result of an XPath expression.
///
/// Schematron allows either `value="expr"` or element content; only the
/// attribute form is supported, which is what virtually all schemas use.
#[derive(Debug, Clone)]
pub struct Let {
    pub name: String,
    pub value: String,
}

/// `sch:phase` — a named subset of patterns to run.
#[derive(Debug, Clone)]
pub struct Phase {
    pub id: String,
    /// `sch:active/@pattern`
    pub active: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckKind {
    /// `sch:assert` — fires when the test is FALSE.
    Assert,
    /// `sch:report` — fires when the test is TRUE.
    Report,
}

/// A piece of an assertion message.
#[derive(Debug, Clone)]
pub enum MsgPart {
    Text(String),
    /// `sch:value-of/@select`, evaluated against the firing node.
    ValueOf(String),
    /// `sch:name` — the name of the firing node, or of `@path`.
    Name(Option<String>),
}

/// `sch:assert` or `sch:report`.
#[derive(Debug, Clone)]
pub struct Check {
    pub kind: CheckKind,
    pub id: Option<String>,
    pub test: String,
    pub flag: Option<String>,
    pub message: Vec<MsgPart>,
}

impl Check {
    /// The message with every `value-of` and `name` left as a placeholder.
    pub fn message_template(&self) -> String {
        let mut s = String::new();
        for p in &self.message {
            match p {
                MsgPart::Text(t) => s.push_str(t),
                MsgPart::ValueOf(e) => s.push_str(&format!("{{{e}}}")),
                MsgPart::Name(_) => s.push_str("{name}"),
            }
        }
        s.split_whitespace().collect::<Vec<_>>().join(" ")
    }
}

/// `sch:rule` — a context plus the checks that apply to nodes matching it.
#[derive(Debug, Clone)]
pub struct Rule {
    pub id: Option<String>,
    /// Absent for an abstract rule.
    pub context: Option<String>,
    pub is_abstract: bool,
    pub lets: Vec<Let>,
    pub checks: Vec<Check>,
    /// `sch:extends/@rule` — abstract rules whose checks are spliced in.
    pub extends: Vec<String>,
}

/// `sch:pattern` — an independent group of rules.
///
/// Rule matching is per pattern: within one pattern a node is claimed by the
/// FIRST rule whose context matches it, and later rules in that pattern do not
/// see it. Patterns do not interfere with each other.
#[derive(Debug, Clone)]
pub struct Pattern {
    pub id: Option<String>,
    pub title: Option<String>,
    pub is_abstract: bool,
    pub lets: Vec<Let>,
    pub rules: Vec<Rule>,
}

/// A whole Schematron schema.
#[derive(Debug, Clone, Default)]
pub struct Schema {
    pub title: Option<String>,
    pub query_binding: Option<String>,
    pub default_phase: Option<String>,
    pub ns: Vec<Ns>,
    /// Schema-level variables, in document order (later ones may use earlier ones).
    pub lets: Vec<Let>,
    pub phases: Vec<Phase>,
    pub patterns: Vec<Pattern>,
}

impl Schema {
    /// Patterns active in a phase. `None` or `#ALL` means every pattern.
    pub fn active_patterns(&self, phase: Option<&str>) -> Vec<&Pattern> {
        let wanted = phase.or(self.default_phase.as_deref());
        let ids: Option<Vec<&str>> = match wanted {
            None | Some("#ALL") => None,
            Some(p) => self
                .phases
                .iter()
                .find(|ph| ph.id == p)
                .map(|ph| ph.active.iter().map(|s| s.as_str()).collect()),
        };
        self.patterns
            .iter()
            .filter(|p| !p.is_abstract)
            .filter(|p| match &ids {
                None => true,
                Some(ids) => p.id.as_deref().is_some_and(|id| ids.contains(&id)),
            })
            .collect()
    }
}
