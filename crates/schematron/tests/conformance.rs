use schematron::{parse, validate, CheckKind};

const DOC: &str = r#"<lib xmlns="urn:x">
  <book id="a"><title>One</title><pages>100</pages></book>
  <book id="b"><title>Two</title></book>
  <mag id="c"><title>Mag</title></mag>
</lib>"#;

fn sch(body: &str) -> String {
    format!(
        r#"<sch:schema xmlns:sch="http://purl.oclc.org/dsdl/schematron">
             <sch:ns prefix="x" uri="urn:x"/>{body}</sch:schema>"#
    )
}

fn run(body: &str) -> schematron::Report {
    let s = parse(&sch(body)).expect("parse");
    validate(&s, DOC, None).expect("validate")
}

fn ids(r: &schematron::Report) -> Vec<String> {
    r.firings.iter().filter_map(|f| f.id.clone()).collect()
}

#[test]
fn assert_fires_when_false_report_when_true() {
    let r = run(r#"<sch:pattern id="p"><sch:rule context="//x:book">
        <sch:assert id="has-pages" test="x:pages">missing pages</sch:assert>
        <sch:report id="is-book" test="true()">a book</sch:report>
      </sch:rule></sch:pattern>"#);
    // one of the two books lacks <pages>, so exactly one assert fires
    assert_eq!(ids(&r).iter().filter(|i| *i == "has-pages").count(), 1);
    // the report fires for both
    assert_eq!(ids(&r).iter().filter(|i| *i == "is-book").count(), 2);
    assert!(!r.is_valid(), "a failed assert makes the document invalid");
}

#[test]
fn reports_alone_do_not_invalidate() {
    let r = run(r#"<sch:pattern id="p"><sch:rule context="//x:book">
        <sch:report id="n" test="true()">note</sch:report>
      </sch:rule></sch:pattern>"#);
    assert!(r.is_valid(), "reports are informational");
    assert_eq!(r.firings.len(), 2);
}

#[test]
fn first_matching_rule_in_a_pattern_claims_the_node() {
    // ISO: within one pattern, a node is processed by the FIRST matching rule only.
    let r = run(r#"<sch:pattern id="p">
        <sch:rule context="//x:book"><sch:report id="first" test="true()">first</sch:report></sch:rule>
        <sch:rule context="//x:book"><sch:report id="second" test="true()">second</sch:report></sch:rule>
      </sch:pattern>"#);
    assert_eq!(ids(&r).iter().filter(|i| *i == "first").count(), 2);
    assert_eq!(ids(&r).iter().filter(|i| *i == "second").count(), 0);
}

#[test]
fn patterns_are_independent_of_each_other() {
    let r = run(r#"
        <sch:pattern id="p1"><sch:rule context="//x:book"><sch:report id="one" test="true()">1</sch:report></sch:rule></sch:pattern>
        <sch:pattern id="p2"><sch:rule context="//x:book"><sch:report id="two" test="true()">2</sch:report></sch:rule></sch:pattern>"#);
    assert_eq!(ids(&r).iter().filter(|i| *i == "one").count(), 2);
    assert_eq!(ids(&r).iter().filter(|i| *i == "two").count(), 2);
}

#[test]
fn union_context_is_rooted_per_branch() {
    let r = run(r#"<sch:pattern id="p"><sch:rule context="x:book | x:mag">
        <sch:report id="any" test="true()">any</sch:report></sch:rule></sch:pattern>"#);
    assert_eq!(ids(&r).len(), 3, "two books and one magazine");
}

#[test]
fn variables_resolve_and_chain() {
    let r = run(r#"<sch:pattern id="p">
        <sch:let name="n" value="count(//x:book)"/>
        <sch:let name="double" value="$n * 2"/>
        <sch:rule context="/x:lib">
          <sch:report id="v" test="$double = 4">double is <sch:value-of select="$double"/></sch:report>
        </sch:rule></sch:pattern>"#);
    assert_eq!(ids(&r), vec!["v"]);
    assert!(r.firings[0].message.contains('4'));
}

#[test]
fn value_of_and_name_resolve_against_the_firing_node() {
    let r = run(r#"<sch:pattern id="p"><sch:rule context="//x:book[not(x:pages)]">
        <sch:assert id="a" test="false()"><sch:name/> <sch:value-of select="@id"/> has no pages</sch:assert>
      </sch:rule></sch:pattern>"#);
    assert_eq!(r.firings.len(), 1);
    assert_eq!(r.firings[0].message, "book b has no pages");
}

#[test]
fn abstract_rules_are_spliced_in_via_extends() {
    let r = run(r#"<sch:pattern id="p">
        <sch:rule id="base" abstract="true">
          <sch:assert id="titled" test="x:title">needs a title</sch:assert>
        </sch:rule>
        <sch:rule context="//x:book"><sch:extends rule="base"/>
          <sch:assert id="paged" test="x:pages">needs pages</sch:assert>
        </sch:rule></sch:pattern>"#);
    // every book has a title, one lacks pages
    assert_eq!(ids(&r).iter().filter(|i| *i == "titled").count(), 0);
    assert_eq!(ids(&r).iter().filter(|i| *i == "paged").count(), 1);
}

#[test]
fn abstract_rules_never_match_on_their_own() {
    let r = run(r#"<sch:pattern id="p">
        <sch:rule id="base" abstract="true" context="//x:book">
          <sch:assert id="never" test="false()">should not run</sch:assert>
        </sch:rule></sch:pattern>"#);
    assert!(r.firings.is_empty());
}

#[test]
fn phases_select_patterns() {
    let body = r#"
        <sch:phase id="quick"><sch:active pattern="p1"/></sch:phase>
        <sch:pattern id="p1"><sch:rule context="//x:book"><sch:report id="one" test="true()">1</sch:report></sch:rule></sch:pattern>
        <sch:pattern id="p2"><sch:rule context="//x:book"><sch:report id="two" test="true()">2</sch:report></sch:rule></sch:pattern>"#;
    let s = parse(&sch(body)).unwrap();

    let all = validate(&s, DOC, None).unwrap();
    assert_eq!(all.firings.len(), 4, "no phase runs every pattern");

    let quick = validate(&s, DOC, Some("quick")).unwrap();
    assert!(quick.firings.iter().all(|f| f.id.as_deref() == Some("one")));
    assert_eq!(quick.firings.len(), 2);
}

#[test]
fn current_is_bound_to_the_matched_node() {
    // current() is an XSLT function with no XPath equivalent; inside a rule it is
    // the matched node, so it is rewritten to a variable bound to exactly that.
    let r = run(r#"<sch:pattern id="p"><sch:rule context="//x:book">
        <sch:report id="c" test="count(//x:book[@id = current()/@id]) = 1">
          <sch:value-of select="current()/@id"/></sch:report>
      </sch:rule></sch:pattern>"#);
    assert_eq!(ids(&r).len(), 2);
    let msgs: Vec<&str> = r.firings.iter().map(|f| f.message.as_str()).collect();
    assert!(msgs.contains(&"a") && msgs.contains(&"b"));
}

#[test]
fn current_inside_a_string_literal_is_left_alone() {
    let r = run(r#"<sch:pattern id="p"><sch:rule context="/x:lib">
        <sch:report id="lit" test="'current()' = 'current()'">ok</sch:report>
      </sch:rule></sch:pattern>"#);
    assert_eq!(ids(&r), vec!["lit"]);
}

#[test]
fn xpath_31_constructs_are_available() {
    let r = run(r#"<sch:pattern id="p"><sch:rule context="/x:lib">
        <sch:report id="x31" test="some $b in //x:book satisfies xs:integer($b/x:pages) gt 50">
          typed comparison and a range variable</sch:report>
      </sch:rule></sch:pattern>"#);
    assert_eq!(ids(&r), vec!["x31"]);
}

#[test]
fn errors_are_collected_not_fatal() {
    let r = run(r#"<sch:pattern id="p"><sch:rule context="/x:lib">
        <sch:assert id="bad" test="no-such-function(1)">broken</sch:assert>
        <sch:report id="good" test="true()">fine</sch:report>
      </sch:rule></sch:pattern>"#);
    assert_eq!(r.errors.len(), 1, "the broken test is recorded");
    assert!(ids(&r).contains(&"good".to_string()), "others still run");
    assert!(!r.is_valid(), "an un-evaluable check invalidates the run");
}

#[test]
fn svrl_contains_both_outcome_elements() {
    let r = run(r#"<sch:pattern id="p"><sch:rule context="//x:book">
        <sch:assert id="a" test="x:pages">no pages</sch:assert>
        <sch:report id="r" test="x:title">has a title</sch:report>
      </sch:rule></sch:pattern>"#);
    let svrl = schematron::to_svrl(&r, Some("t"));
    assert!(svrl.contains("svrl:failed-assert"));
    assert!(svrl.contains("svrl:successful-report"));
    assert!(svrl.contains(r#"id="a""#));
    assert!(svrl.starts_with("<?xml"));
}

#[test]
fn locations_are_xpaths_into_the_instance() {
    let r = run(r#"<sch:pattern id="p"><sch:rule context="//x:book[not(x:pages)]">
        <sch:assert id="a" test="false()">x</sch:assert></sch:rule></sch:pattern>"#);
    assert!(r.firings[0].location.contains("book"), "got {}", r.firings[0].location);
}

#[test]
fn assert_and_report_kinds_are_distinguished() {
    let r = run(r#"<sch:pattern id="p"><sch:rule context="/x:lib">
        <sch:assert id="a" test="false()">a</sch:assert>
        <sch:report id="r" test="true()">r</sch:report></sch:rule></sch:pattern>"#);
    assert_eq!(r.firings.iter().filter(|f| f.kind == CheckKind::Assert).count(), 1);
    assert_eq!(r.firings.iter().filter(|f| f.kind == CheckKind::Report).count(), 1);
    assert_eq!(r.failures().count(), 1);
}

/// Known upstream defect in xee-xpath 0.1.5: a greater-than comparison against NaN
/// returns true, where XPath requires every comparison with NaN to be false.
/// `lt` and `eq` are correct, so this is isolated to `gt`.
///
/// This test asserts the CURRENT behaviour so the day it is fixed upstream, it
/// fails loudly and the note in the README can be removed.
#[test]
fn known_issue_nan_greater_than_is_wrong() {
    let r = run(r#"<sch:pattern id="p"><sch:rule context="/x:lib">
        <sch:report id="gt" test="xs:double('NaN') &gt; 1">NaN gt</sch:report>
        <sch:report id="lt" test="xs:double('NaN') &lt; 1">NaN lt</sch:report>
        <sch:report id="eq" test="xs:double('NaN') = xs:double('NaN')">NaN eq</sch:report>
      </sch:rule></sch:pattern>"#);
    let fired = ids(&r);
    assert!(fired.contains(&"gt".into()), "upstream NaN gt bug appears fixed — update the README");
    assert!(!fired.contains(&"lt".into()), "NaN lt is correct");
    assert!(!fired.contains(&"eq".into()), "NaN eq is correct");
}
