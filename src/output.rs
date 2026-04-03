use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use chrono::Utc;
use std::path::Path;

use crate::models::{ConversionResult, DocumentMeta, JsonOutput, OutputFormat};

/// How to handle `![](images/xxx.jpg)` references in the output.
pub enum ImageMode {
    /// Replace with compact text tags: `[📷 图片 1: xxx.jpg (45 KB)]`.
    /// Default for stdout — tells the LLM "there is an image here" without bloating context.
    Tag,
    /// Keep the original relative paths unchanged.
    Keep,
    /// Replace with `data:image/...;base64,...` URIs (for vision-capable LLMs).
    Base64,
    /// Replace `images/` prefix with a custom prefix (for --output-dir mode).
    RelativePath { prefix: String },
}

/// Render the conversion result into the requested output format.
pub fn render(
    result: &ConversionResult,
    format: &OutputFormat,
    source_path: &Path,
    meta_params: &MetaParams,
    image_mode: ImageMode,
) -> String {
    let markdown = result
        .markdown
        .as_deref()
        .unwrap_or("")
        .trim()
        .to_string();

    let markdown = rewrite_images(&markdown, &result.images, &image_mode);

    match format {
        OutputFormat::Markdown => render_markdown(&markdown, source_path, meta_params),
        OutputFormat::Json => render_json(&markdown, result, source_path, meta_params),
        OutputFormat::Plain => render_plain(&markdown),
    }
}

// ─── Image rewriting ──────────────────────────────────────────────────────────

/// Replace `![alt](images/fname)` with `[🖼 Image N: alt (X KB)]` text tags.
fn replace_image_tags_with_text(
    line: &str,
    images: &std::collections::HashMap<String, Vec<u8>>,
) -> String {
    // Simple state-machine replacement of ![...](images/...) patterns.
    let mut result = String::with_capacity(line.len());
    let mut pos = 0;
    let mut img_index = 0usize;
    let bytes = line.as_bytes();

    while pos < bytes.len() {
        // Look for `![`
        if pos + 1 < bytes.len() && bytes[pos] == b'!' && bytes[pos + 1] == b'[' {
            if let Some(alt_end) = line[pos + 2..].find("](") {
                let alt = &line[pos + 2..pos + 2 + alt_end];
                let after_bracket = pos + 2 + alt_end + 2; // skip "]("
                if let Some(path_end) = line[after_bracket..].find(')') {
                    let img_path = &line[after_bracket..after_bracket + path_end];
                    // Extract just the filename
                    let fname = img_path
                        .split('/')
                        .last()
                        .unwrap_or(img_path);
                    let size_hint = images.get(fname)
                        .map(|b| {
                            let kb = b.len() / 1024;
                            if kb == 0 { format!("{} B", b.len()) } else { format!("{kb} KB") }
                        })
                        .unwrap_or_default();
                    img_index += 1;
                    let label = if alt.is_empty() {
                        format!("[🖼 Image {img_index}: {fname} {size_hint}]")
                    } else {
                        format!("[🖼 Image {img_index}: {alt} ({fname} {size_hint})]")
                    };
                    result.push_str(&label);
                    pos = after_bracket + path_end + 1; // skip past ")"
                    continue;
                }
            }
        }
        result.push(bytes[pos] as char);
        pos += 1;
    }
    result
}

/// Rewrite `![alt](images/fname)` references according to the chosen ImageMode.
fn rewrite_images(
    markdown: &str,
    images: &std::collections::HashMap<String, Vec<u8>>,
    mode: &ImageMode,
) -> String {
    match mode {
        ImageMode::Tag => {
            // Replace ![alt](images/fname) with a compact text tag for LLM context.
            // Pattern: keep the alt text if present, add filename and file size.
            let mut out = String::with_capacity(markdown.len());
            for line in markdown.lines() {
                let replaced = replace_image_tags_with_text(line, images);
                out.push_str(&replaced);
                out.push('\n');
            }
            out
        }
        ImageMode::Keep => markdown.to_string(),
        ImageMode::RelativePath { prefix } => {
            // Replace `images/` prefix with the given prefix
            markdown.replace("](images/", &format!("]({prefix}/"))
        }
        ImageMode::Base64 => {
            let mut out = markdown.to_string();
            for (fname, bytes) in images {
                let original_ref = format!("images/{fname}");
                if out.contains(&original_ref) {
                    let mime = mime_guess::from_path(fname)
                        .first_or(mime_guess::mime::IMAGE_JPEG)
                        .to_string();
                    let b64 = BASE64.encode(bytes);
                    let data_uri = format!("data:{mime};base64,{b64}");
                    out = out.replace(&original_ref, &data_uri);
                }
            }
            out
        }
    }
}

// ─── Markdown ─────────────────────────────────────────────────────────────────

fn render_markdown(markdown: &str, source_path: &Path, meta: &MetaParams) -> String {
    let file_name = source_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("document");

    let processed_at = Utc::now().format("%Y-%m-%d %H:%M UTC").to_string();

    let header = format!(
        "<!-- source: {file_name} | processed: {processed_at} | backend: {} | pages: {} -->",
        meta.backend, meta.pages
    );

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
        image_count: result.images.len(),
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
    let text = clean_markdown(markdown);

    // Strip Markdown headings
    let lines: Vec<String> = text
        .lines()
        .map(|line| {
            let mut l = line.to_string();
            for prefix in &["######", "#####", "####", "###", "##", "#"] {
                if l.starts_with(prefix) {
                    l = l.trim_start_matches('#').trim().to_string();
                    break;
                }
            }
            l
        })
        .collect();

    let text = lines.join("\n");
    remove_md_formatting(&text)
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

fn clean_markdown(markdown: &str) -> String {
    let mut out = String::with_capacity(markdown.len());
    let mut blank_count = 0usize;

    for line in markdown.lines() {
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            blank_count += 1;
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
    let mut s = text.to_string();

    // Remove code fences, keeping content
    while let Some(start) = s.find("```") {
        if let Some(end) = s[start + 3..].find("```") {
            let code_block = s[start..start + 3 + end + 3].to_string();
            let inner = &s[start + 3..start + 3 + end];
            let code_content = inner
                .trim_start_matches(|c: char| c.is_alphabetic() || c == '_' || c == '-')
                .trim_start_matches('\n')
                .to_string();
            s = s.replacen(&code_block, &code_content, 1);
        } else {
            break;
        }
    }

    // Strip **bold** and *italic*
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

