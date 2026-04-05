mod api;
mod models;
mod output;

use anyhow::{Context, Result, bail};
use clap::Parser;
use console::style;
use indicatif::{ProgressBar, ProgressStyle};
use models::{ConversionParams, OutputFormat};
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
///   mineru doc.pdf --embed-images        # inline images as base64 (multimodal LLMs)
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
    #[arg(
        short = 'l',
        long,
        default_value = "ch (Chinese, English, Chinese Traditional)"
    )]
    lang: String,

    /// Processing backend
    #[arg(short = 'b', long,
          default_value = "hybrid-auto-engine",
          value_parser = clap::builder::PossibleValuesParser::new([
              "pipeline", "vlm-auto-engine", "hybrid-auto-engine"
          ]))]
    backend: String,

    /// Save output to this directory (markdown + images/ folder extracted here)
    #[arg(short = 'o', long)]
    output_dir: Option<PathBuf>,

    /// Inline images as base64 data URIs in the markdown (for multimodal LLMs / self-contained output).
    /// When --output-dir is set, images are always saved as files instead.
    #[arg(long, default_value_t = false)]
    embed_images: bool,

    /// Suppress progress output (useful when piping output)
    #[arg(short = 'q', long, default_value_t = false)]
    quiet: bool,

    /// Custom Gradio server URL (default: MinerU HuggingFace Space).
    /// Use this if HuggingFace is blocked in your region (e.g. mainland China).
    /// Set HTTPS_PROXY / HTTP_PROXY for network-level proxy instead.
    ///
    /// Example: --server-url https://your-mineru-mirror.example.com
    #[arg(long, default_value = api::DEFAULT_SPACE_BASE, env = "MINERU_SERVER_URL")]
    server_url: String,
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
        // Normalize server URL: strip trailing slash
        let server_url = cli.server_url.trim_end_matches('/');
        match process_file(&client, file_path, &cli, server_url).await {
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

async fn process_file(
    client: &Client,
    file_path: &Path,
    cli: &Cli,
    server_url: &str,
) -> Result<()> {
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
    let gradio_file = api::upload_file(client, file_path, server_url)
        .await
        .context("Upload failed")?;

    pb.set_message(format!("Queuing {file_name}…"));

    // ── Queue join ────────────────────────────────────────────────────────────
    let params = ConversionParams {
        max_pages: cli.pages,
        is_ocr: cli.ocr,
        formula_enable: !cli.no_formulas,
        table_enable: !cli.no_tables,
        language: &cli.lang,
        backend: &cli.backend,
    };
    let (_event_id, session_hash) = api::queue_join(client, &gradio_file, &params, server_url)
        .await
        .context("Failed to queue conversion job")?;

    // ── Stream result ─────────────────────────────────────────────────────────
    let pb_clone = pb.clone();
    let result = api::stream_result(client, &session_hash, server_url, move |msg| {
        pb_clone.set_message(format!("{}", style(msg).dim()));
    })
    .await
    .context("Failed during SSE stream")?;

    pb.finish_and_clear();

    if !cli.quiet {
        eprintln!("{} {}", style("✓").green().bold(), style(&file_name).bold());
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

    match &cli.output_dir {
        Some(dir) => {
            let stem = file_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("output");

            // ── Save images/ sub-folder ────────────────────────────────────────
            if !result.images.is_empty() {
                let img_dir = dir.join(stem).join("images");
                fs::create_dir_all(&img_dir)
                    .await
                    .with_context(|| format!("Cannot create {}", img_dir.display()))?;
                for (fname, bytes) in &result.images {
                    let img_path = img_dir.join(fname);
                    fs::write(&img_path, bytes)
                        .await
                        .with_context(|| format!("Cannot write image {fname}"))?;
                }
                if !cli.quiet {
                    eprintln!(
                        "  📁 images/  ({} files → {})",
                        result.images.len(),
                        style(dir.join(stem).join("images").display()).dim()
                    );
                }
            }

            // ── Render markdown with relative image paths ──────────────────────
            // Images are saved at <stem>/images/<file>, markdown at <stem>.<ext>,
            // so the relative path from the md file is "<stem>/images/<file>".
            let rendered = output::render(
                &result,
                &cli.format,
                file_path,
                &meta,
                output::ImageMode::RelativePath {
                    prefix: format!("{stem}/images"),
                },
            );

            let ext = match cli.format {
                OutputFormat::Markdown => "md",
                OutputFormat::Json => "json",
                OutputFormat::Plain => "txt",
            };
            let out_path = dir.join(format!("{stem}.{ext}"));
            fs::write(&out_path, &rendered)
                .await
                .with_context(|| format!("Cannot write to {}", out_path.display()))?;
            if !cli.quiet {
                eprintln!("  → {}", style(out_path.display()).underlined());
            }
        }
        None => {
            let image_mode = if cli.embed_images {
                output::ImageMode::Base64
            } else {
                output::ImageMode::Tag
            };
            let rendered = output::render(&result, &cli.format, file_path, &meta, image_mode);
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
        "pdf", "docx", "doc", "ppt", "pptx", "png", "jpg", "jpeg", "webp", "bmp", "tiff", "tif",
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
