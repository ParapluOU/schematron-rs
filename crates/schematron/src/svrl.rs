//! SVRL — Schematron Validation Report Language output.

use crate::eval::Report;
use crate::model::CheckKind;

pub const SVRL_NS: &str = "http://purl.oclc.org/dsdl/svrl";

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn attr(name: &str, v: &Option<String>) -> String {
    v.as_ref().map(|v| format!(" {name}=\"{}\"", esc(v))).unwrap_or_default()
}

/// Render a [`Report`] as an SVRL document.
pub fn to_svrl(report: &Report, title: Option<&str>) -> String {
    let mut out = String::from(r#"<?xml version="1.0" encoding="UTF-8"?>"#);
    out.push_str(&format!("\n<svrl:schematron-output xmlns:svrl=\"{SVRL_NS}\""));
    if let Some(t) = title {
        out.push_str(&format!(" title=\"{}\"", esc(t)));
    }
    out.push_str(">\n");

    for f in &report.firings {
        let el = match f.kind {
            CheckKind::Assert => "failed-assert",
            CheckKind::Report => "successful-report",
        };
        out.push_str(&format!(
            "  <svrl:{el} test=\"{}\" location=\"{}\"{}{}>\n    <svrl:text>{}</svrl:text>\n  </svrl:{el}>\n",
            esc(&f.test),
            esc(&f.location),
            attr("id", &f.id),
            attr("flag", &f.flag),
            esc(&f.message),
        ));
    }
    for (ctx, msg) in &report.errors {
        out.push_str(&format!(
            "  <!-- evaluation error in {}: {} -->\n",
            esc(ctx),
            esc(msg)
        ));
    }
    out.push_str("</svrl:schematron-output>\n");
    out
}
