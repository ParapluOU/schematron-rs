# schematron-rs

ISO Schematron (ISO/IEC 19757-3) validation in Rust, on XPath 3.1.

The reference implementation of Schematron is a stack of XSLT stylesheets that compile a
schema into a validator — which means a JVM or an XSLT 2.0 processor at runtime. This
crate interprets the schema directly instead: it parses `.sch` into a model and evaluates
each rule with [`xee-xpath`](https://docs.rs/xee-xpath), a native Rust XPath 3.1 engine.
No XSLT, no Java, no shelling out.

```sh
cargo run -p schematron-cli -- rules.sch document.xml
cargo run -p schematron-cli -- rules.sch document.xml --svrl > report.svrl
cargo run -p schematron-cli -- rules.sch document.xml --phase quick
```

```rust
let schema = schematron::parse(&std::fs::read_to_string("rules.sch")?)?;
let report = schematron::validate(&schema, &doc, None)?;
for f in report.failures() {
    println!("{}: {}", f.location, f.message);
}
```

Exits 1 on a failed assertion, 2 on a usage or evaluation error.

## Why

`ParapluOU/pubbin` carries `Schematron validation engine` on its TODO, a Schematron tab in
the frontend and `has_schematron_errors` on its models, with nothing behind them.
`ParapluOU/schemas-rs` already embeds DocBook 5.1 Schematron rules with no way to run them.
This is the missing piece, and it reuses the org's own XML stack: `xee` for XPath and `xot`
for the tree.

## What is implemented

| | |
|---|---|
| `schema`, `ns`, `title` | yes |
| `let` | schema, pattern and rule level; chained (a later `let` may use an earlier one) |
| `phase` / `active`, `@defaultPhase` | yes |
| `pattern`, `rule`, `assert`, `report` | yes |
| Abstract rules and `extends` | yes |
| Message content | `value-of`, `name`, text; inline markup (`emph`, `span`, `dir`, `b`, `i`) flattened |
| ISO rule semantics | **first matching rule in a pattern claims the node**; patterns independent |
| `current()` | yes — see below |
| Output | SVRL, or text |

### `current()`

`current()` is an XSLT function with no XPath equivalent, so a pure XPath engine rejects it
with `XPST0017`. Schemas in the wild use it constantly, to escape a nested predicate back
out to the matched node.

Inside a Schematron rule `current()` *is* the matched node, so it is rewritten to a variable
bound to exactly that node. This is an identity, not an approximation. Occurrences inside
string literals are left alone.

## What is not implemented

Abstract patterns with `param`, `diagnostics`, `include`, and `properties`. `queryBinding`
is parsed but not enforced: expressions are always XPath 3.1. That is a superset of the
`xpath2` binding, but it differs from `xslt1`/`xpath` (XPath 1.0) in places — most visibly,
an untyped comparison such as `@a > @b` is numeric in 1.0 and may be string-ordered in 3.1.

## Known upstream defect

**`xee-xpath` 0.1.5 gets greater-than against NaN wrong.** `xs:double('NaN') > 1` returns
`true`; XPath requires every comparison involving NaN to be false. `lt` and `eq` are
correct, so this is isolated to `gt`.

It bites in a common Schematron idiom: `number(missing-element) > 1000` is NaN > 1000, and
so fires when it must not. Until it is fixed, prefer the bare comparison —
`missing-element > 1000` — which is an empty sequence and correctly yields false.

`known_issue_nan_greater_than_is_wrong` in `crates/schematron/tests/conformance.rs` pins the
current behaviour, so the test fails the day the fix lands and this note can go.

## Tests

```sh
cargo test
```

18 conformance tests covering assert/report polarity, first-match-wins, pattern
independence, union contexts, variable chaining, `value-of`/`name` resolution, abstract
rules, phases, `current()`, XPath 3.1 constructs, error collection, and SVRL shape.
