use anyhow::{Context, Result, bail, ensure};
use mct_trace::{ingest_pi_session, project_book, verify_repository};
use std::collections::BTreeMap;
use std::env;
use std::path::PathBuf;

fn main() -> Result<()> {
    let mut args = env::args().skip(1);
    let command = args.next().unwrap_or_else(|| "help".to_owned());
    if matches!(command.as_str(), "help" | "--help" | "-h") {
        print_help();
        return Ok(());
    }
    let options = Options::parse(args)?;
    let repo_root = options
        .optional_path("--repo-root")
        .unwrap_or(env::current_dir()?);

    match command.as_str() {
        "ingest" => {
            let source = options.required_path("--source")?;
            options.ensure_only(&["--repo-root", "--source"])?;
            let entry = ingest_pi_session(&repo_root, &source)?;
            println!(
                "ingested {}: {} events, {} -> {} bytes",
                entry.trace_id, entry.event_count, entry.source.bytes, entry.archive.bytes
            );
        }
        "verify" => {
            options.ensure_only(&["--repo-root"])?;
            let report = verify_repository(&repo_root)?;
            println!(
                "verified {} trace(s), {} event(s), {} compressed byte(s)",
                report.traces, report.events, report.compressed_bytes
            );
        }
        "project" => {
            let book_source = options.required_path("--book-source")?;
            options.ensure_only(&["--repo-root", "--book-source"])?;
            let report = project_book(&repo_root, &book_source)?;
            println!(
                "projected {} trace(s) and {} event(s) into {}",
                report.traces,
                report.events,
                book_source.display()
            );
        }
        other => bail!("unknown mct-trace command {other:?}; run mct-trace --help"),
    }
    Ok(())
}

struct Options(BTreeMap<String, String>);

impl Options {
    fn parse(mut args: impl Iterator<Item = String>) -> Result<Self> {
        let mut values = BTreeMap::new();
        while let Some(flag) = args.next() {
            ensure!(
                flag.starts_with("--"),
                "unexpected positional argument {flag:?}"
            );
            let value = args
                .next()
                .with_context(|| format!("missing value for {flag}"))?;
            ensure!(
                values.insert(flag.clone(), value).is_none(),
                "duplicate option {flag}"
            );
        }
        Ok(Self(values))
    }

    fn required_path(&self, name: &str) -> Result<PathBuf> {
        self.0
            .get(name)
            .map(PathBuf::from)
            .with_context(|| format!("missing required {name}"))
    }

    fn optional_path(&self, name: &str) -> Option<PathBuf> {
        self.0.get(name).map(PathBuf::from)
    }

    fn ensure_only(&self, allowed: &[&str]) -> Result<()> {
        for option in self.0.keys() {
            ensure!(
                allowed.contains(&option.as_str()),
                "unexpected option {option:?}"
            );
        }
        Ok(())
    }
}

fn print_help() {
    println!(
        "mct-trace — deterministic public trace archive tooling\n\n\
         Usage:\n  \
         mct-trace ingest --source <PI_JSONL> [--repo-root <REPO>]\n  \
         mct-trace verify [--repo-root <REPO>]\n  \
         mct-trace project --book-source <DIR> [--repo-root <REPO>]"
    );
}
