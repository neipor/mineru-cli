mod api;
mod models;
mod output;

use anyhow::{Context, Result, bail};
use clap::Parser;
use console::style;
use indicatif::{ProgressBar, ProgressStyle};
use models::OutputFormat;
use output::MetaParams;
use reqwest::Client;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::fs;

/// MinerU CLI — convert PDFs, images, and documents to LLM-friendly Markdown
/// using the MinerU OCR HuggingFace Space.
///
/// Supported formats: PDF, DOCX, DOC, PPT, PPTX, PNG, JPG, JPEG, WebP, BMP, TIFF
///
/// Examples:
///   mineru paper.pdf
///   mineru scan.pdf --ocr --lang en
///   mineru report.pdf -f json -o ./output
///   mineru *.pdf --pages 5 --backend pipeline
#[derive(Parser, Debug)]
#[command(name = "mineru", version, about, long_about = None)]
struct Cli {
    /// Input files (PDF, DOCX, images, etc.)
    #[arg(required = true)]
    files: Vec<PathBuf>,

    /// Output format
    #[arg(short = 'f', long, value_enum, default_value = "markdown")]
    format: OutputFormat,

    /// Maximum pages to process per document
    #[arg(short = 'p', long, default_value_t = 20)]
    pages: u32,

    /// Force OCR mode (slower; use when native text extraction fails)
    #[arg(long, default_value_t = false)]
    ocr: bool,

    /// Disable formula (LaTeX) recognition
    #[arg(long, default_value_t = false)]
    no_formulas: bool,

    /// Disable table recognition
    #[arg(long, default_value_t = false)]
    no_tables: bool,

    /// OCR language code, e.g. "ch", "en", "fr"
    #[arg(short = 'l', long, default_value = "ch (Chinese, English, Chinese Traditional)")]
    lang: String,

    /// Processing backend
    #[arg(short = 'b', long,
          default_value = "hybrid-auto-engine",
          value_parser = clap::builder::PossibleValuesParser::new([
              "pipeline", "vlm-auto-engine", "hybrid-auto-engine"
          ]))]
    backend: String,

    /// Save output to this directory (one file per input); prints to stdout if omitted
    #[arg(short = 'o', long)]
    output_dir: Option<PathBuf>,

    /// Suppress progress output (useful when piping output)
    #[arg(short = 'q', long, default_value_t = false)]
    quiet: bool,
}

// ─── Entry point ──────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Create output directory if requested
    if let Some(dir) = &cli.output_dir {
        fs::create_dir_all(dir)
            .await
            .with_context(|| format!("Cannot create output directory: {}", dir.display()))?;
    }

    let client = build_client()?;
    let mut had_error = false;

    for file_path in &cli.files {
        match process_file(&client, file_path, &cli).await {
            Ok(()) => {}
            Err(e) => {
                eprintln!("{} {}: {e:#}", style("✗").red().bold(), file_path.display());
                had_error = true;
            }
        }
    }

    if had_error {
        std::process::exit(1);
    }
    Ok(())
}

// ─── Per-file processing ──────────────────────────────────────────────────────

async fn process_file(client: &Client, file_path: &Path, cli: &Cli) -> Result<()> {
    if !file_path.exists() {
        bail!("File not found: {}", file_path.display());
    }

    validate_extension(file_path)?;

    let file_name = file_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("document")
        .to_string();

    // ── Progress bar ─────────────────────────────────────────────────────────
    let pb = if cli.quiet {
        ProgressBar::hidden()
    } else {
        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::with_template("{spinner:.cyan} {msg}")
                .unwrap()
                .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏"),
        );
        pb.enable_steady_tick(Duration::from_millis(80));
        pb
    };

    pb.set_message(format!("Uploading {file_name}…"));

    // ── Upload ────────────────────────────────────────────────────────────────
    let gradio_file = api::upload_file(client, file_path)
        .await
        .context("Upload failed")?;

    pb.set_message(format!("Queuing {file_name}…"));

    // ── Queue join ────────────────────────────────────────────────────────────
    let (_event_id, session_hash) = api::queue_join(
        client,
        &gradio_file,
        cli.pages,
        cli.ocr,
        !cli.no_formulas,
        !cli.no_tables,
        &cli.lang,
        &cli.backend,
    )
    .await
    .context("Failed to queue conversion job")?;

    // ── Stream result ─────────────────────────────────────────────────────────
    let pb_clone = pb.clone();
    let result = api::stream_result(client, &session_hash, move |msg| {
        pb_clone.set_message(format!("{}", style(msg).dim()));
    })
    .await
    .context("Failed during SSE stream")?;

    pb.finish_and_clear();

    if !cli.quiet {
        eprintln!(
            "{} {}",
            style("✓").green().bold(),
            style(&file_name).bold()
        );
    }

    // ── Render output ─────────────────────────────────────────────────────────
    let meta = MetaParams {
        backend: cli.backend.clone(),
        pages: cli.pages,
        ocr: cli.ocr,
        formula: !cli.no_formulas,
        table: !cli.no_tables,
        language: cli.lang.clone(),
    };

    let rendered = output::render(&result, &cli.format, file_path, &meta);

    match &cli.output_dir {
        Some(dir) => {
            let ext = match cli.format {
                OutputFormat::Markdown => "md",
                OutputFormat::Json => "json",
                OutputFormat::Plain => "txt",
            };
            let stem = file_path.file_stem().and_then(|s| s.to_str()).unwrap_or("output");
            let out_path = dir.join(format!("{stem}.{ext}"));
            fs::write(&out_path, &rendered)
                .await
                .with_context(|| format!("Cannot write to {}", out_path.display()))?;
            if !cli.quiet {
                eprintln!("  → {}", style(out_path.display()).underlined());
            }
        }
        None => {
            print!("{rendered}");
        }
    }

    Ok(())
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn build_client() -> Result<Client> {
    Client::builder()
        .timeout(Duration::from_secs(600))
        .connect_timeout(Duration::from_secs(30))
        .user_agent("mineru-cli/0.1.0")
        .build()
        .context("Failed to build HTTP client")
}

fn validate_extension(path: &Path) -> Result<()> {
    const SUPPORTED: &[&str] = &[
        "pdf", "docx", "doc", "ppt", "pptx",
        "png", "jpg", "jpeg", "webp", "bmp", "tiff", "tif",
    ];
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    if !SUPPORTED.contains(&ext.as_str()) {
        bail!(
            "Unsupported file type '.{ext}'. Supported: {}",
            SUPPORTED.join(", ")
        );
    }
    Ok(())
}
