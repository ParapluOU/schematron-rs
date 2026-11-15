//! Parse an ISO Schematron schema into [`Schema`].

use crate::error::{Error, Result};
use crate::model::*;
use xot::Xot;

/// The ISO Schematron namespace.
pub const SCH_NS: &str = "http://purl.oclc.org/dsdl/schematron";

/// Parse a Schematron schema from a string.
///
/// Elements are recognised in the ISO namespace, and also with no namespace at
/// all, since schemas in the wild are sometimes written without it.
pub fn parse(src: &str) -> Result<Schema> {
    let mut xot = Xot::new();
    let root = xot.parse(src).map_err(|e| Error::Xml(e.to_string()))?;
    let doc = xot
        .document_element(root)
        .map_err(|e| Error::Xml(e.to_string()))?;
    let p = Parser { xot: &xot };
    let (name, _) = p.name_of(doc).ok_or_else(|| Error::NotASchema("?".into()))?;
    if name != "schema" {
        return Err(Error::NotASchema(name));
    }
    p.schema(doc)
}

struct Parser<'a> {
    xot: &'a Xot,
}

impl<'a> Parser<'a> {
    fn name_of(&self, node: xot::Node) -> Option<(String, String)> {
        let el = self.xot.element(node)?;
        let nid = el.name();
        let local = self.xot.local_name_str(nid).to_string();
        let ns = self
            .xot
            .namespace_str(self.xot.namespace_for_name(nid))
            .to_string();
        Some((local, ns))
    }

    /// Schematron children of `node`, by local name, ignoring foreign namespaces.
    fn kids(&self, node: xot::Node) -> impl Iterator<Item = (String, xot::Node)> + '_ {
        self.xot.children(node).filter_map(move |c| {
            let (local, ns) = self.name_of(c)?;
            (ns == SCH_NS || ns.is_empty()).then_some((local, c))
        })
    }

    fn attr(&self, node: xot::Node, want: &str) -> Option<String> {
        self.xot.attributes(node).iter().find_map(|(nid, v)| {
            (self.xot.local_name_str(nid) == want).then(|| v.to_string())
        })
    }

    fn text(&self, node: xot::Node) -> String {
        let mut out = String::new();
        self.collect_text(node, &mut out);
        out
    }

    fn collect_text(&self, node: xot::Node, out: &mut String) {
        for c in self.xot.children(node) {
            if let Some(t) = self.xot.text_str(c) {
                out.push_str(t);
            } else if self.xot.element(c).is_some() {
                self.collect_text(c, out);
            }
        }
    }

    fn lets(&self, node: xot::Node) -> Vec<Let> {
        self.kids(node)
            .filter(|(n, _)| n == "let")
            .filter_map(|(_, c)| {
                Some(Let {
                    name: self.attr(c, "name")?,
                    value: self.attr(c, "value").unwrap_or_else(|| self.text(c)),
                })
            })
            .collect()
    }

    fn schema(&self, node: xot::Node) -> Result<Schema> {
        let mut s = Schema {
            query_binding: self.attr(node, "queryBinding"),
            default_phase: self.attr(node, "defaultPhase"),
            lets: self.lets(node),
            ..Default::default()
        };
        for (name, c) in self.kids(node) {
            match name.as_str() {
                "title" => s.title = Some(self.text(c).trim().to_string()),
                "ns" => {
                    if let (Some(prefix), Some(uri)) = (self.attr(c, "prefix"), self.attr(c, "uri"))
                    {
                        s.ns.push(Ns { prefix, uri });
                    }
                }
                "phase" => {
                    if let Some(id) = self.attr(c, "id") {
                        s.phases.push(Phase {
                            id,
                            active: self
                                .kids(c)
                                .filter(|(n, _)| n == "active")
                                .filter_map(|(_, a)| self.attr(a, "pattern"))
                                .collect(),
                        });
                    }
                }
                "pattern" => s.patterns.push(self.pattern(c)),
                _ => {}
            }
        }
        Ok(s)
    }

    fn pattern(&self, node: xot::Node) -> Pattern {
        Pattern {
            id: self.attr(node, "id"),
            title: self
                .kids(node)
                .find(|(n, _)| n == "title")
                .map(|(_, t)| self.text(t).trim().to_string()),
            is_abstract: self.attr(node, "abstract").as_deref() == Some("true"),
            lets: self.lets(node),
            rules: self
                .kids(node)
                .filter(|(n, _)| n == "rule")
                .map(|(_, r)| self.rule(r))
                .collect(),
        }
    }

    fn rule(&self, node: xot::Node) -> Rule {
        let mut checks = Vec::new();
        let mut extends = Vec::new();
        for (name, c) in self.kids(node) {
            match name.as_str() {
                "assert" => checks.push(self.check(c, CheckKind::Assert)),
                "report" => checks.push(self.check(c, CheckKind::Report)),
                "extends" => {
                    if let Some(r) = self.attr(c, "rule") {
                        extends.push(r);
                    }
                }
                _ => {}
            }
        }
        Rule {
            id: self.attr(node, "id"),
            context: self.attr(node, "context"),
            is_abstract: self.attr(node, "abstract").as_deref() == Some("true"),
            lets: self.lets(node),
            checks,
            extends,
        }
    }

    fn check(&self, node: xot::Node, kind: CheckKind) -> Check {
        Check {
            kind,
            id: self.attr(node, "id"),
            test: self.attr(node, "test").unwrap_or_default(),
            flag: self.attr(node, "flag"),
            message: self.message(node),
        }
    }

    /// Mixed content: text, `value-of`, `name`, and inline markup flattened to text.
    fn message(&self, node: xot::Node) -> Vec<MsgPart> {
        let mut parts = Vec::new();
        for c in self.xot.children(node) {
            if let Some(t) = self.xot.text_str(c) {
                parts.push(MsgPart::Text(t.to_string()));
                continue;
            }
            match self.name_of(c) {
                Some((n, _)) if n == "value-of" => {
                    parts.push(MsgPart::ValueOf(self.attr(c, "select").unwrap_or_default()))
                }
                Some((n, _)) if n == "name" => parts.push(MsgPart::Name(self.attr(c, "path"))),
                Some(_) => parts.extend(self.message(c)), // emph, span, dir, b, i …
                None => {}
            }
        }
        parts
    }
}
