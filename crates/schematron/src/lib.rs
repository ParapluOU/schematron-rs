//! ISO Schematron (ISO/IEC 19757-3) validation for Rust, on XPath 3.1.
//!
//! The reference implementation of Schematron is a set of XSLT stylesheets that
//! compile a schema into a validator. This crate instead interprets the schema
//! directly: it parses `.sch` into a model and evaluates each rule with
//! [`xee-xpath`](https://docs.rs/xee-xpath), a native Rust XPath 3.1 engine.
//! No XSLT processor, no Java, no shelling out.
//!
//! ```no_run
//! let schema = schematron::parse(&std::fs::read_to_string("rules.sch")?)?;
//! let report = schematron::validate(&schema, &std::fs::read_to_string("doc.xml")?, None)?;
//! for f in report.failures() {
//!     println!("{}: {}", f.location, f.message);
//! }
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! # What is implemented
//!
//! Schema, `ns`, `let` (schema, pattern and rule level), `phase` with `active`,
//! `pattern`, `rule` including abstract rules and `extends`, `assert`, `report`,
//! message content (`value-of`, `name`, and inline markup flattened to text),
//! and the ISO rule-matching semantics where the first matching rule in a pattern
//! claims a node. Output as SVRL or as text.
//!
//! # What is not, yet
//!
//! Abstract patterns with `param`, `diagnostics`, `include`, `properties`, and
//! the `xslt` query binding's access to XSLT-only functions. `queryBinding` is
//! recorded but not enforced; expressions are always XPath 3.1, which is a
//! superset of the `xpath2` binding and differs from `xslt1`/`xpath` (1.0) in
//! some edge cases.

pub mod error;
pub mod eval;
pub mod model;
pub mod parse;
pub mod svrl;

pub use error::{Error, Result};
pub use eval::{validate, Firing, Report};
pub use model::{Check, CheckKind, Pattern, Rule, Schema};
pub use parse::parse;
pub use svrl::to_svrl;
