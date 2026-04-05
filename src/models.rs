use serde::{Deserialize, Serialize};

// ─── Gradio API Structures ────────────────────────────────────────────────────

/// File descriptor returned by Gradio 6 upload endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GradioFile {
    pub path: String,
    pub url: Option<String>,
    pub size: Option<u64>,
    pub orig_name: Option<String>,
    pub mime_type: Option<String>,
    #[serde(default)]
    pub is_stream: bool,
    pub meta: Option<serde_json::Value>,
}

/// Response from the /gradio_api/queue/join endpoint.
#[derive(Debug, Deserialize)]
pub struct QueueJoinResponse {
    pub event_id: String,
}

/// Generic SSE event from the Gradio queue.
#[derive(Debug, Deserialize)]
pub struct SseEvent {
    pub msg: String,
    #[serde(default)]
    pub output: Option<FnOutput>,
    pub rank: Option<u32>,
    #[allow(dead_code)]
    pub rank_eta: Option<f64>,
    #[allow(dead_code)]
    pub queue_size: Option<u32>,
    #[serde(default)]
    pub success: bool,
}

/// The output payload inside an SSE event.
#[derive(Debug, Deserialize)]
pub struct FnOutput {
    pub data: Option<Vec<serde_json::Value>>,
    #[allow(dead_code)]
    pub is_generating: bool,
    pub error: Option<String>,
}

// ─── Application State ────────────────────────────────────────────────────────

/// Indices of the five output positions produced by `convert_to_markdown_stream`.
/// outputs=[status_box(33), output_file(34), md(37), md_text(39), doc_show(29)]
pub struct OutputIndex;
impl OutputIndex {
    pub const STATUS: usize = 0;
    pub const OUTPUT_FILE: usize = 1;
    pub const MD_RENDER: usize = 2;
    pub const MD_TEXT: usize = 3;
}

/// Accumulated result of a conversion job.
#[derive(Debug, Default)]
pub struct ConversionResult {
    pub status_messages: Vec<String>,
    /// Markdown content extracted from the ZIP's .md file.
    pub markdown: Option<String>,
    /// Images extracted from the ZIP's images/ folder: filename → raw bytes.
    pub images: std::collections::HashMap<String, Vec<u8>>,
    pub output_file_url: Option<String>,
    pub output_file_path: Option<String>,
    /// True once `process_completed` SSE event is received.
    pub completed: bool,
}

// ─── Conversion Parameters ────────────────────────────────────────────────────

/// Parameters for a single conversion job sent to the Gradio queue.
pub struct ConversionParams<'a> {
    pub max_pages: u32,
    pub is_ocr: bool,
    pub formula_enable: bool,
    pub table_enable: bool,
    pub language: &'a str,
    pub backend: &'a str,
}

// ─── Output Formats ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, clap::ValueEnum, Default)]
pub enum OutputFormat {
    /// Clean Markdown, ideal for piping into LLMs
    #[default]
    Markdown,
    /// Structured JSON with metadata + content
    Json,
    /// Plain text (no Markdown syntax)
    Plain,
}

/// Metadata attached to a conversion result.
#[derive(Debug, Serialize)]
pub struct DocumentMeta {
    pub source_file: String,
    pub processed_at: String,
    pub backend: String,
    pub pages: u32,
    pub ocr: bool,
    pub formula: bool,
    pub table: bool,
    pub language: String,
    pub image_count: usize,
}

/// Full structured output when format == Json.
#[derive(Debug, Serialize)]
pub struct JsonOutput {
    pub meta: DocumentMeta,
    pub content: String,
    pub status_log: Vec<String>,
}
