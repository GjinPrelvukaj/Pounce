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
        /// Check every `<img src>` with a HEAD request, so the media rules can
        /// report broken and oversized images.
        ///
        /// Opt-in rather than default. It multiplies request count by however
        /// many distinct images the site has, sends those requests to whatever
        /// third-party hosts the markup names, and adds wall time to a number
        /// this project publishes as its headline. Turning it on is a decision
        /// worth making rather than one to discover afterwards.
        #[arg(long)]
        images: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let Args { command } = Args::parse();
    match command {
        Command::Crawl {
            url,
            output,
            quiet,
            images,
        } => {
            let seed = CrawlUrl::parse(&url).context("invalid crawl URL")?;
            let summary = {
                // Every shipped rule, registered once. An empty registry here
                // would mean the rules exist and never run.
                let mut registry = pounce_audit::Registry::new();
                pounce_audit::register_all(&mut registry)?;
                pounce_run::crawl(seed, &output, &registry, images).await?
            };
            if !quiet {
                println!(
                    "crawled {} pages ({} failures) into {}",
                    summary.pages,
                    summary.failures,
                    output.display()
                );
                if images {
                    println!("checked {} distinct images", summary.resources);
                }
            }
        }
    }
    Ok(())
}
