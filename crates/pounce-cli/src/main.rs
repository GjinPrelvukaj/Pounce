use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use pounce_core::CrawlUrl;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "pounce", about = "Fast, disk-backed technical SEO crawler")]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Crawl a site into a portable SQLite database.
    Crawl {
        url: String,
        #[arg(short, long, default_value = "crawl.pounce")]
        output: PathBuf,
        #[arg(long)]
        quiet: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let Args { command } = Args::parse();
    match command {
        Command::Crawl { url, output, quiet } => {
            let seed = CrawlUrl::parse(&url).context("invalid crawl URL")?;
            let summary = pounce_seo::crawl(seed, &output).await?;
            if !quiet {
                println!(
                    "crawled {} pages ({} failures) into {}",
                    summary.pages,
                    summary.failures,
                    output.display()
                );
            }
        }
    }
    Ok(())
}
