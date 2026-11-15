//! `schematron` — validate an XML document against an ISO Schematron schema.

use std::process::ExitCode;

const USAGE: &str = "\
schematron — ISO Schematron validation

USAGE:
    schematron <rules.sch> <document.xml> [OPTIONS]

OPTIONS:
    --phase <id>   Run only the patterns active in this phase
    --svrl         Emit an SVRL report instead of text
    --quiet        Only show assertions that fired
    -h, --help     Show this help

Exits 1 if any assertion failed, 2 on a usage or evaluation error.
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args.iter().any(|a| a == "-h" || a == "--help") {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }

    let flag = |n: &str| args.iter().any(|a| a == n);
    let opt = |n: &str| {
        args.iter()
            .position(|a| a == n)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let positional: Vec<&String> = {
        let mut out = Vec::new();
        let mut skip = false;
        for a in &args {
            if skip {
                skip = false;
                continue;
            }
            if a == "--phase" {
                skip = true;
            } else if !a.starts_with("--") {
                out.push(a);
            }
        }
        out
    };

    let (Some(sch_path), Some(xml_path)) = (positional.first(), positional.get(1)) else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };

    let run = || -> schematron::Result<(schematron::Report, Option<String>)> {
        let schema = schematron::parse(&std::fs::read_to_string(sch_path)?)?;
        let doc = std::fs::read_to_string(xml_path)?;
        let report = schematron::validate(&schema, &doc, opt("--phase").as_deref())?;
        Ok((report, schema.title.clone()))
    };

    let (report, title) = match run() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(2);
        }
    };

    if flag("--svrl") {
        print!("{}", schematron::to_svrl(&report, title.as_deref()));
        return if report.is_valid() { ExitCode::SUCCESS } else { ExitCode::FAILURE };
    }

    let quiet = flag("--quiet");
    let (red, yellow, green, dim, off) =
        ("\x1b[31m", "\x1b[33m", "\x1b[32m", "\x1b[2m", "\x1b[0m");

    println!();
    if let Some(t) = &title {
        println!("{t}");
    }
    println!("{dim}{} — {}{off}\n", sch_path, xml_path);

    let mut failed = 0usize;
    let mut reported = 0usize;
    for f in &report.firings {
        match f.kind {
            schematron::CheckKind::Assert => {
                failed += 1;
                // Schemas often repeat the id at the start of the message; do not print it twice.
                let id = match &f.id {
                    Some(i) if !f.message.starts_with(i.as_str()) => format!("{i} "),
                    _ => String::new(),
                };
                println!("{red}FAIL{off} {id}{}", f.message);
                if !quiet {
                    println!("     {dim}at {}{off}", f.location);
                }
            }
            schematron::CheckKind::Report => {
                reported += 1;
                println!("{yellow}note{off} {}", f.message);
            }
        }
    }
    for (ctx, msg) in &report.errors {
        println!("{red}ERR {off} {ctx}: {msg}");
    }

    let colour = if report.is_valid() { green } else { red };
    println!(
        "\n{colour}{failed} failed{off}, {reported} notes, {} errors — \
         {} checks over {} matched nodes\n",
        report.errors.len(),
        report.checks_run,
        report.nodes_matched
    );

    if report.is_valid() { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}
