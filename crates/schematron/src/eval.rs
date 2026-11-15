//! Evaluate a [`Schema`] against an instance document, over XPath 3.1.

use crate::error::{Error, Result};
use crate::model::*;
use std::collections::HashSet;
use xee_xpath::context::{StaticContextBuilder, Variables};
use xee_xpath::{Documents, Itemable, Queries, Query, Sequence};
use xee_xpath::Item;
use xot::xmlname::OwnedName;

/// Reserved variable backing the XSLT `current()` function (see [`rewrite_current`]).
const CURRENT: &str = "__schematron_current__";

/// Rewrite the XSLT-only `current()` to a variable bound to the node the rule matched.
///
/// Schematron's reference implementation compiles to XSLT, so schemas in the wild use
/// `current()` to escape a nested predicate back to the matched node. A pure XPath
/// engine has no such function (XPST0017). Inside a Schematron rule `current()` is by
/// definition the matched node, so binding it as a variable is exact, not an
/// approximation. Occurrences inside string literals are left alone.
fn rewrite_current(expr: &str) -> String {
    if !expr.contains("current()") {
        return expr.to_string();
    }
    let (mut out, mut quote) = (String::with_capacity(expr.len()), None::<char>);
    let b: Vec<char> = expr.chars().collect();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        match quote {
            Some(q) if c == q => { quote = None; out.push(c); i += 1; }
            Some(_) => { out.push(c); i += 1; }
            None if c == '\'' || c == '"' => { quote = Some(c); out.push(c); i += 1; }
            None if b[i..].starts_with(&['c','u','r','r','e','n','t','(',')']) => {
                out.push('$'); out.push_str(CURRENT); i += 9;
            }
            None => { out.push(c); i += 1; }
        }
    }
    out
}

/// One assertion that fired.
#[derive(Debug, Clone)]
pub struct Firing {
    pub kind: CheckKind,
    pub id: Option<String>,
    pub test: String,
    pub flag: Option<String>,
    /// Message with `value-of` and `name` resolved against the firing node.
    pub message: String,
    /// Location of the firing node, as an XPath.
    pub location: String,
    pub pattern: Option<String>,
    pub rule: Option<String>,
}

/// The outcome of a validation run.
#[derive(Debug, Clone, Default)]
pub struct Report {
    pub firings: Vec<Firing>,
    /// Checks that could not be evaluated, with the reason.
    pub errors: Vec<(String, String)>,
    pub checks_run: usize,
    pub nodes_matched: usize,
}

impl Report {
    /// Failed asserts. Reports are informational and never count as failures.
    pub fn failures(&self) -> impl Iterator<Item = &Firing> {
        self.firings.iter().filter(|f| f.kind == CheckKind::Assert)
    }
    pub fn is_valid(&self) -> bool {
        self.failures().next().is_none() && self.errors.is_empty()
    }
}

/// Validate `instance` against `schema`, optionally restricted to a phase.
pub fn validate(schema: &Schema, instance: &str, phase: Option<&str>) -> Result<Report> {
    let mut docs = Documents::new();
    let handle = docs
        .add_string_without_uri(instance)
        .map_err(|e| Error::Xml(format!("{e:?}")))?;
    let root = docs.document_node(handle).ok_or_else(|| {
        Error::Xml("document has no root node".into())
    })?;

    // Every variable in the schema must be declared when expressions are compiled.
    let var_names: Vec<String> = schema
        .lets
        .iter()
        .map(|l| l.name.clone())
        .chain(schema.patterns.iter().flat_map(|p| {
            p.lets
                .iter()
                .map(|l| l.name.clone())
                .chain(p.rules.iter().flat_map(|r| r.lets.iter().map(|l| l.name.clone())))
        }))
        .collect();

    let mut var_names = var_names;
    var_names.push(CURRENT.to_string());

    let ev = Evaluator {
        ns: &schema.ns,
        var_names,
    };

    let mut report = Report::default();

    // Every name declared in the static context must also be bound in the dynamic
    // context, in the same order. Start them all as the empty sequence so a schema
    // whose variables are still being resolved cannot desynchronise the two.
    let mut vars = Variables::new();
    for n in &ev.var_names {
        vars.insert(
            OwnedName::new(n.clone(), String::new(), String::new()),
            Sequence::default(),
        );
    }

    // Schema-level variables, in document order so later ones can use earlier ones.
    ev.bind_lets(&mut docs, root, &schema.lets, &mut vars, &mut report);

    for pattern in schema.active_patterns(phase) {
        let mut pvars = vars.clone();
        ev.bind_lets(&mut docs, root, &pattern.lets, &mut pvars, &mut report);

        // ISO semantics: within a pattern, the first rule whose context matches a
        // node claims it; later rules in the same pattern never see that node.
        let mut claimed: HashSet<xot::Node> = HashSet::new();

        for rule in &pattern.rules {
            if rule.is_abstract {
                continue;
            }
            let Some(context) = rule.context.as_deref() else {
                continue;
            };
            let selector = absolute_selector(context);
            let nodes = match ev.nodes(&mut docs, root, &selector, &pvars) {
                Ok(n) => n,
                Err(e) => {
                    report
                        .errors
                        .push((format!("rule context `{context}`"), e.to_string()));
                    continue;
                }
            };

            let checks = resolve_extends(pattern, rule);

            for node in nodes {
                if !claimed.insert(node) {
                    continue; // already claimed by an earlier rule in this pattern
                }
                report.nodes_matched += 1;

                let mut rvars = pvars.clone();
                rvars.insert(
                    OwnedName::new(CURRENT.to_string(), String::new(), String::new()),
                    Sequence::from(vec![Item::Node(node)]),
                );
                ev.bind_lets(&mut docs, node, &rule.lets, &mut rvars, &mut report);

                for check in &checks {
                    report.checks_run += 1;
                    let holds = match ev.boolean(&mut docs, node, &check.test, &rvars) {
                        Ok(b) => b,
                        Err(e) => {
                            report.errors.push((
                                check
                                    .id
                                    .clone()
                                    .unwrap_or_else(|| check.test.clone()),
                                e.to_string(),
                            ));
                            continue;
                        }
                    };
                    let fires = match check.kind {
                        CheckKind::Assert => !holds,
                        CheckKind::Report => holds,
                    };
                    if !fires {
                        continue;
                    }
                    let message = ev.message(&mut docs, node, &check.message, &rvars);
                    let location = ev
                        .string(&mut docs, node, "path()", &rvars)
                        .unwrap_or_default();
                    report.firings.push(Firing {
                        kind: check.kind,
                        id: check.id.clone(),
                        test: check.test.clone(),
                        flag: check.flag.clone(),
                        message,
                        location,
                        pattern: pattern.id.clone(),
                        rule: rule.id.clone(),
                    });
                }
            }
        }
    }
    Ok(report)
}

/// Splice in the checks of any abstract rules this rule extends.
fn resolve_extends(pattern: &Pattern, rule: &Rule) -> Vec<Check> {
    let mut checks = Vec::new();
    for name in &rule.extends {
        if let Some(base) = pattern
            .rules
            .iter()
            .find(|r| r.is_abstract && r.id.as_deref() == Some(name.as_str()))
        {
            checks.extend(base.checks.iter().cloned());
        }
    }
    checks.extend(rule.checks.iter().cloned());
    checks
}

/// Turn a rule context into an absolute selection.
///
/// A Schematron context is an XSLT match pattern. For the patterns that occur in
/// practice, the node set a pattern matches is exactly what you get by rooting it
/// at `//`: the pattern `foo/bar` matches any `bar` whose parent is `foo`, which
/// is what `//foo/bar` selects. Absolute patterns are left alone, and each branch
/// of a union is handled separately.
fn absolute_selector(context: &str) -> String {
    split_union(context)
        .into_iter()
        .map(|branch| {
            let b = branch.trim();
            if b.starts_with('/') || b.starts_with('(') {
                b.to_string()
            } else {
                format!("//{b}")
            }
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

/// Split on `|` at depth zero, so unions inside predicates or calls survive.
fn split_union(s: &str) -> Vec<&str> {
    let (mut out, mut depth, mut start, mut quote) = (Vec::new(), 0i32, 0usize, None::<char>);
    for (i, c) in s.char_indices() {
        match c {
            '\'' | '"' if quote == Some(c) => quote = None,
            '\'' | '"' if quote.is_none() => quote = Some(c),
            _ if quote.is_some() => {}
            '(' | '[' => depth += 1,
            ')' | ']' => depth -= 1,
            '|' if depth == 0 => {
                out.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(&s[start..]);
    out
}

struct Evaluator<'a> {
    ns: &'a [Ns],
    var_names: Vec<String>,
}

impl<'a> Evaluator<'a> {
    fn queries(&self) -> Queries<'_> {
        let mut sb = StaticContextBuilder::default();
        for n in self.ns {
            sb.add_namespace(&n.prefix, &n.uri);
        }
        sb.variable_names(
            self.var_names
                .iter()
                .map(|n| OwnedName::new(n.clone(), String::new(), String::new())),
        );
        Queries::new(sb)
    }

    fn bind_lets(
        &self,
        docs: &mut Documents,
        node: xot::Node,
        lets: &[Let],
        vars: &mut Variables,
        report: &mut Report,
    ) {
        for l in lets {
            match self.sequence(docs, node, &l.value, vars) {
                Ok(seq) => {
                    vars.insert(
                        OwnedName::new(l.name.clone(), String::new(), String::new()),
                        seq,
                    );
                }
                Err(e) => report.errors.push((format!("let ${}", l.name), e.to_string())),
            }
        }
    }

    /// The nodes selected by `expr`, in document order, as xot node ids so the
    /// caller can track which rule has already claimed them.
    fn nodes(
        &self,
        docs: &mut Documents,
        node: xot::Node,
        expr: &str,
        vars: &Variables,
    ) -> Result<Vec<xot::Node>> {
        let queries = self.queries();
        let expr = &rewrite_current(expr);
        let q = queries
            .many(expr, |_, i| Ok(i.to_node()?))
            .map_err(|e| self.err(expr, e))?;
        let item = node.to_item(docs).map_err(|e| self.err(expr, e))?;
        let vs = vars.clone();
        q.execute_build_context(docs, |b| {
            b.context_item(item);
            b.variables(vs);
        })
        .map_err(|e| self.err(expr, e))
    }

    fn sequence(
        &self,
        docs: &mut Documents,
        node: xot::Node,
        expr: &str,
        vars: &Variables,
    ) -> Result<Sequence> {
        let queries = self.queries();
        let expr = &rewrite_current(expr);
        let q = queries.sequence(expr).map_err(|e| self.err(expr, e))?;
        let item = node.to_item(docs).map_err(|e| self.err(expr, e))?;
        let vs = vars.clone();
        q.execute_build_context(docs, |b| {
            b.context_item(item);
            b.variables(vs);
        })
        .map_err(|e| self.err(expr, e))
    }

    fn boolean(
        &self,
        docs: &mut Documents,
        node: xot::Node,
        expr: &str,
        vars: &Variables,
    ) -> Result<bool> {
        let queries = self.queries();
        let expr = &rewrite_current(expr);
        let q = queries
            .one(&format!("boolean({expr})"), |_, i| Ok(i.try_into_value::<bool>()?))
            .map_err(|e| self.err(expr, e))?;
        let item = node.to_item(docs).map_err(|e| self.err(expr, e))?;
        let vs = vars.clone();
        q.execute_build_context(docs, |b| {
            b.context_item(item);
            b.variables(vs);
        })
        .map_err(|e| self.err(expr, e))
    }

    fn string(
        &self,
        docs: &mut Documents,
        node: xot::Node,
        expr: &str,
        vars: &Variables,
    ) -> Result<String> {
        let queries = self.queries();
        let expr = &rewrite_current(expr);
        let q = queries
            .one(&format!("string-join(({expr}) ! string(), ' ')"), |_, i| {
                Ok(i.try_into_value::<String>()?)
            })
            .map_err(|e| self.err(expr, e))?;
        let item = node.to_item(docs).map_err(|e| self.err(expr, e))?;
        let vs = vars.clone();
        q.execute_build_context(docs, |b| {
            b.context_item(item);
            b.variables(vs);
        })
        .map_err(|e| self.err(expr, e))
    }

    fn message(
        &self,
        docs: &mut Documents,
        node: xot::Node,
        parts: &[MsgPart],
        vars: &Variables,
    ) -> String {
        let mut out = String::new();
        for p in parts {
            match p {
                MsgPart::Text(t) => out.push_str(t),
                MsgPart::ValueOf(e) => {
                    out.push_str(&self.string(docs, node, e, vars).unwrap_or_else(|_| "?".into()))
                }
                MsgPart::Name(path) => {
                    let e = path.clone().unwrap_or_else(|| ".".into());
                    out.push_str(
                        &self
                            .string(docs, node, &format!("name({e})"), vars)
                            .unwrap_or_else(|_| "?".into()),
                    )
                }
            }
        }
        out.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    fn err(&self, context: &str, e: impl std::fmt::Debug) -> Error {
        Error::XPath {
            context: context.to_string(),
            message: format!("{e:?}"),
        }
    }
}
