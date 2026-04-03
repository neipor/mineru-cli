use chrono::Utc;
use std::path::Path;

use crate::models::{ConversionResult, DocumentMeta, JsonOutput, OutputFormat};

/// Render the conversion result into the requested output format,
/// producing a string ready for stdout or file writing.
pub fn render(
    result: &ConversionResult,
    format: &OutputFormat,
    source_path: &Path,
    meta_params: &MetaParams,
) -> String {
    let markdown = result
        .markdown
        .as_deref()
        .unwrap_or("")
        .trim()
        .to_string();

    match format {
        OutputFormat::Markdown => render_markdown(&markdown, source_path, meta_params),
        OutputFormat::Json => render_json(&markdown, result, source_path, meta_params),
        OutputFormat::Plain => render_plain(&markdown),
    }
}

// ─── Markdown ─────────────────────────────────────────────────────────────────

fn render_markdown(markdown: &str, source_path: &Path, meta: &MetaParams) -> String {
    let file_name = source_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("document");

    let processed_at = Utc::now().format("%Y-%m-%d %H:%M UTC").to_string();

    // Build LLM-friendly metadata block at the top
    let header = format!(
        "<!-- source: {file_name} | processed: {processed_at} | backend: {} | pages: {} -->",
        meta.backend, meta.pages
    );

    // Clean up common OCR artefacts
    let clean = clean_markdown(markdown);

    format!("{header}\n\n{clean}")
}

// ─── JSON ─────────────────────────────────────────────────────────────────────

fn render_json(
    markdown: &str,
    result: &ConversionResult,
    source_path: &Path,
    meta: &MetaParams,
) -> String {
    let file_name = source_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("document")
        .to_string();

    let doc_meta = DocumentMeta {
        source_file: file_name,
        processed_at: Utc::now().to_rfc3339(),
        backend: meta.backend.clone(),
        pages: meta.pages,
        ocr: meta.ocr,
        formula: meta.formula,
        table: meta.table,
        language: meta.language.clone(),
    };

    let output = JsonOutput {
        meta: doc_meta,
        content: clean_markdown(markdown),
        status_log: result.status_messages.clone(),
    };

    serde_json::to_string_pretty(&output).unwrap_or_else(|_| "{}".to_string())
}

// ─── Plain ────────────────────────────────────────────────────────────────────

fn render_plain(markdown: &str) -> String {
    let mut text = clean_markdown(markdown);

    // Remove Markdown headings (## -> blank)
    let heading_re = ["######", "#####", "####", "###", "##", "#"];
    let lines: Vec<String> = text
        .lines()
        .map(|line| {
            let mut l = line.to_string();
            for prefix in &heading_re {
                if l.starts_with(prefix) {
                    l = l.trim_start_matches('#').trim().to_string();
                    break;
                }
            }
            l
        })
        .collect();

    // Remove Markdown formatting: **bold**, *italic*, `code`, ~~strikethrough~~
    text = lines.join("\n");
    text = remove_md_formatting(&text);
    text
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

/// Parameters passed alongside the result for metadata generation.
pub struct MetaParams {
    pub backend: String,
    pub pages: u32,
    pub ocr: bool,
    pub formula: bool,
    pub table: bool,
    pub language: String,
}

/// Minimal cleanup of Gradio-generated markdown for LLM consumption:
/// - Normalise excessive blank lines
/// - Remove trailing whitespace
/// - Preserve LaTeX math blocks (\\[ \\] and \\( \\))
fn clean_markdown(markdown: &str) -> String {
    let mut out = String::with_capacity(markdown.len());
    let mut blank_count = 0usize;

    for line in markdown.lines() {
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            blank_count += 1;
            // Allow at most two consecutive blank lines
            if blank_count <= 2 {
                out.push('\n');
            }
        } else {
            blank_count = 0;
            out.push_str(trimmed);
            out.push('\n');
        }
    }

    out.trim().to_string()
}

fn remove_md_formatting(text: &str) -> String {
    // Very lightweight regex-free removal
    let mut s = text.to_string();

    // Remove code fences
    while let Some(start) = s.find("```") {
        if let Some(end) = s[start + 3..].find("```") {
            let code_block = &s[start..start + 3 + end + 3].to_string();
            // Keep the code content, strip the fences
            let inner = &s[start + 3..start + 3 + end];
            // Skip the language identifier on the first line
            let code_content = inner
                .trim_start_matches(|c: char| c.is_alphabetic() || c == '_' || c == '-')
                .trim_start_matches('\n')
                .to_string();
            s = s.replacen(code_block, &code_content, 1);
        } else {
            break;
        }
    }

    // Remove bold/italic markers: **text** -> text, *text* -> text
    let mut result = String::with_capacity(s.len());
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if i + 1 < chars.len() && chars[i] == '*' && chars[i + 1] == '*' {
            i += 2;
            continue;
        }
        if chars[i] == '*' {
            i += 1;
            continue;
        }
        result.push(chars[i]);
        i += 1;
    }

    result
}
