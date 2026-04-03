use anyhow::{Context, Result, bail};
use bytes::Bytes;
use futures_util::StreamExt;
use reqwest::Client;
use serde_json::Value;
use std::io::Read;
use std::path::Path;
use uuid::Uuid;

use crate::models::{ConversionResult, GradioFile, OutputIndex, QueueJoinResponse, SseEvent};

/// Base URL of the MinerU HuggingFace Space.
pub const SPACE_BASE: &str = "https://opendatalab-mineru.hf.space";

/// The `fn_index` of `convert_to_markdown_stream` in the Gradio dependency list.
/// Derived by inspecting the Space config: dependency[8] = convert_to_markdown_stream.
pub const FN_INDEX: u32 = 8;

// ─── Upload ───────────────────────────────────────────────────────────────────

/// Upload a local file to the Gradio `/upload` endpoint.
pub async fn upload_file(client: &Client, file_path: &Path) -> Result<GradioFile> {
    let file_name = file_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("document")
        .to_string();

    let mime = mime_guess::from_path(file_path)
        .first_or_octet_stream()
        .to_string();

    let file_bytes = tokio::fs::read(file_path)
        .await
        .with_context(|| format!("Cannot read file: {}", file_path.display()))?;

    let file_size = file_bytes.len() as u64;

    let part = reqwest::multipart::Part::bytes(file_bytes)
        .file_name(file_name.clone())
        .mime_str(&mime)?;

    let form = reqwest::multipart::Form::new().part("files", part);

    let url = format!("{SPACE_BASE}/gradio_api/upload");
    let resp = client
        .post(&url)
        .multipart(form)
        .send()
        .await
        .context("Upload request failed")?;

    if !resp.status().is_success() {
        bail!("Upload failed with status {}", resp.status());
    }

    let paths: Vec<String> = resp.json().await.context("Failed to parse upload response")?;
    let remote_path = paths
        .into_iter()
        .next()
        .context("Upload returned empty path list")?;

    let file_url = format!("{SPACE_BASE}/gradio_api/file={remote_path}");

    Ok(GradioFile {
        path: remote_path,
        url: Some(file_url),
        size: Some(file_size),
        orig_name: Some(file_name),
        mime_type: Some(mime),
        is_stream: false,
        meta: Some(serde_json::json!({"_type": "gradio.FileData"})),
    })
}

// ─── Queue Join ───────────────────────────────────────────────────────────────

/// Submit a conversion job to the Gradio queue.
pub async fn queue_join(
    client: &Client,
    file: &GradioFile,
    max_pages: u32,
    is_ocr: bool,
    formula_enable: bool,
    table_enable: bool,
    language: &str,
    backend: &str,
) -> Result<(String, String)> {
    let session_hash = Uuid::new_v4().simple().to_string()[..10].to_string();

    let data = serde_json::json!([
        serde_json::to_value(file)?,
        max_pages,
        is_ocr,
        formula_enable,
        table_enable,
        language,
        backend,
        "http://localhost:30000"
    ]);

    let body = serde_json::json!({
        "fn_index": FN_INDEX,
        "data": data,
        "event_data": null,
        "session_hash": session_hash,
        "trigger_id": null
    });

    let url = format!("{SPACE_BASE}/gradio_api/queue/join");
    let resp = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .context("Queue join request failed")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        bail!("Queue join failed ({status}): {text}");
    }

    let join_resp: QueueJoinResponse =
        resp.json().await.context("Failed to parse queue join response")?;
    Ok((join_resp.event_id, session_hash))
}

// ─── SSE Stream ───────────────────────────────────────────────────────────────

/// Connect to the SSE data stream and drive the conversion to completion.
pub async fn stream_result(
    client: &Client,
    session_hash: &str,
    on_status: impl Fn(&str),
) -> Result<ConversionResult> {
    let url = format!("{SPACE_BASE}/gradio_api/queue/data?session_hash={session_hash}");

    let resp = client
        .get(&url)
        .header("Accept", "text/event-stream")
        .header("Cache-Control", "no-cache")
        .send()
        .await
        .context("SSE connection failed")?;

    if !resp.status().is_success() {
        bail!("SSE endpoint returned {}", resp.status());
    }

    let mut stream = resp.bytes_stream();
    let mut buffer = String::new();
    let mut result = ConversionResult::default();

    'outer: while let Some(chunk_result) = stream.next().await {
        let chunk: Bytes = chunk_result.context("Stream error")?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));

        while let Some(pos) = buffer.find("\n\n") {
            let message = buffer[..pos].to_string();
            buffer = buffer[pos + 2..].to_string();
            let done = process_sse_message(&message, &mut result, &on_status)?;
            if done {
                break 'outer;
            }
        }
    }

    // Download and fully extract the result ZIP (markdown + images)
    if let Some(ref zip_url) = result.output_file_url.clone() {
        if !zip_url.is_empty() {
            match download_and_extract_zip(client, zip_url).await {
                Ok((md, images)) => {
                    result.markdown = Some(md);
                    result.images = images;
                }
                Err(e) => {
                    eprintln!("\nWarning: could not extract ZIP: {e}");
                }
            }
        }
    }

    if result.completed {
        Ok(result)
    } else {
        bail!("SSE stream ended without a completed result")
    }
}

/// Returns `true` when the stream should stop (process_completed or close_stream).
fn process_sse_message(
    message: &str,
    result: &mut ConversionResult,
    on_status: &impl Fn(&str),
) -> Result<bool> {
    let json_str: String = message
        .lines()
        .filter_map(|line| line.strip_prefix("data: "))
        .collect::<Vec<_>>()
        .join("");

    if json_str.is_empty() {
        return Ok(false);
    }

    let event: SseEvent = match serde_json::from_str(&json_str) {
        Ok(e) => e,
        Err(_) => return Ok(false),
    };

    match event.msg.as_str() {
        "send_hash" => {}
        "estimation" => {
            let rank = event.rank.unwrap_or(0);
            if rank > 0 {
                on_status(&format!("Queued (position {rank})"));
            }
        }
        "process_starts" => {
            on_status("Processing started…");
        }
        "process_generating" => {
            if let Some(output) = &event.output {
                if let Some(err) = &output.error {
                    bail!("Processing error: {err}");
                }
                if let Some(data) = &output.data {
                    extract_status_from_patch(data, result, on_status);
                    extract_file_from_patch(data, result);
                }
            }
        }
        "process_completed" => {
            if !event.success {
                if let Some(output) = &event.output {
                    if let Some(err) = &output.error {
                        bail!("Conversion failed: {err}");
                    }
                }
                bail!("Conversion failed (success=false)");
            }
            if let Some(output) = &event.output {
                if let Some(data) = &output.data {
                    extract_final_outputs(data, result, on_status);
                }
            }
            result.completed = true;
            return Ok(true);
        }
        "close_stream" => {
            return Ok(true);
        }
        "queue_full" => {
            bail!("MinerU queue is full. Please try again later.");
        }
        _ => {}
    }

    Ok(false)
}

// ─── Gradio 6 patch extraction ─────────────────────────────────────────────

/// Extract status text from Gradio 6 streaming patch operations on data[0].
/// Intermediate events use `[["append", [], "text"], ...]` or `[["replace", [], "full"]]`.
fn extract_status_from_patch(
    data: &[Value],
    result: &mut ConversionResult,
    on_status: &impl Fn(&str),
) {
    let Some(status_val) = data.get(OutputIndex::STATUS) else {
        return;
    };
    let Some(ops) = status_val.as_array() else {
        return;
    };
    for op in ops {
        let Some(op_arr) = op.as_array() else {
            continue;
        };
        let Some(kind) = op_arr.first().and_then(Value::as_str) else {
            continue;
        };
        let text = match kind {
            "append" => op_arr.get(2).and_then(Value::as_str),
            "replace" => {
                // "replace" gives the full accumulated text; take the last line
                op_arr.get(2).and_then(Value::as_str)
                    .and_then(|s| s.lines().last())
            }
            _ => None,
        };

        if let Some(raw) = text {
            let stripped = raw.trim_start_matches('\n').trim();
            if stripped.is_empty() {
                continue;
            }
            let last = result.status_messages.last().map(|s| s.as_str()).unwrap_or("");
            // Deduplicate; also collapse repetitive "Processing on server (x.xs)" into one entry
            let is_server_tick = stripped.starts_with("Processing on server (");
            let last_is_server_tick = last.starts_with("Processing on server (");
            if last == stripped || (is_server_tick && last_is_server_tick) {
                // Still call on_status so the spinner updates, but don't push to log
                on_status(stripped);
                continue;
            }
            result.status_messages.push(stripped.to_string());
            on_status(stripped);
        }
    }
}

/// Extract output_file URL from a Gradio 6 patch on data[1].
fn extract_file_from_patch(data: &[Value], result: &mut ConversionResult) {
    let Some(file_val) = data.get(OutputIndex::OUTPUT_FILE) else {
        return;
    };
    // During streaming: `[["replace", [], {path, url, ...}]]`
    if let Some(ops) = file_val.as_array() {
        for op in ops {
            let Some(op_arr) = op.as_array() else {
                continue;
            };
            let kind = op_arr.first().and_then(Value::as_str).unwrap_or("");
            if kind == "replace" {
                if let Some(obj) = op_arr.get(2) {
                    if let Some(url) = obj.pointer("/url").and_then(Value::as_str) {
                        result.output_file_url = Some(url.to_string());
                    }
                    if let Some(path) = obj.pointer("/path").and_then(Value::as_str) {
                        result.output_file_path = Some(path.to_string());
                    }
                }
            }
        }
    }
}

/// Extract all final values from the `process_completed` data array.
/// In this event, values are plain (string/dict), not patch operations.
fn extract_final_outputs(
    data: &[Value],
    result: &mut ConversionResult,
    on_status: &impl Fn(&str),
) {
    // data[0]: status string
    if let Some(s) = data.get(OutputIndex::STATUS).and_then(Value::as_str) {
        if let Some(last_line) = s.lines().last() {
            if result.status_messages.last().map(|x| x.as_str()) != Some(last_line) {
                result.status_messages.push(last_line.to_string());
                on_status(last_line);
            }
        }
    }

    // data[1]: output file descriptor
    if let Some(file_obj) = data.get(OutputIndex::OUTPUT_FILE) {
        if !file_obj.is_null() {
            if let Some(url) = file_obj.pointer("/url").and_then(Value::as_str) {
                result.output_file_url = Some(url.to_string());
            }
            if let Some(path) = file_obj.pointer("/path").and_then(Value::as_str) {
                result.output_file_path = Some(path.to_string());
            }
        }
    }

    // data[3]: md_text (raw markdown; may be empty for simple docs)
    if let Some(s) = data.get(OutputIndex::MD_TEXT).and_then(Value::as_str) {
        if !s.is_empty() {
            result.markdown = Some(s.to_string());
        }
    }

    // data[2]: md_render fallback
    if result.markdown.is_none() {
        if let Some(s) = data.get(OutputIndex::MD_RENDER).and_then(Value::as_str) {
            if !s.is_empty() {
                result.markdown = Some(s.to_string());
            }
        }
    }
}

// ─── ZIP extraction ───────────────────────────────────────────────────────────

/// Download the result ZIP and extract:
/// - The `.md` file as a String
/// - All `images/*` files as a map of filename → raw bytes
pub async fn download_and_extract_zip(
    client: &Client,
    zip_url: &str,
) -> Result<(String, std::collections::HashMap<String, Vec<u8>>)> {
    let resp = client
        .get(zip_url)
        .send()
        .await
        .context("Failed to download result ZIP")?;

    if !resp.status().is_success() {
        bail!("ZIP download returned {}", resp.status());
    }

    let bytes = resp.bytes().await.context("Failed to read ZIP bytes")?;
    extract_zip_contents(&bytes)
}

fn extract_zip_contents(
    bytes: &[u8],
) -> Result<(String, std::collections::HashMap<String, Vec<u8>>)> {
    let cursor = std::io::Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(cursor).context("Invalid ZIP archive")?;

    let mut markdown = String::new();
    let mut images: std::collections::HashMap<String, Vec<u8>> = std::collections::HashMap::new();

    // Collect names first (borrow checker)
    let names: Vec<String> = (0..archive.len())
        .filter_map(|i| archive.by_index(i).ok().map(|f| f.name().to_string()))
        .collect();

    for name in &names {
        // Skip directory entries
        if name.ends_with('/') {
            continue;
        }

        let mut file = archive.by_name(name)
            .with_context(|| format!("Cannot open {name} in ZIP"))?;

        if name.ends_with(".md") && !name.contains('/') {
            // Top-level .md file → main markdown content
            file.read_to_string(&mut markdown)
                .with_context(|| format!("Cannot read {name}"))?;
        } else if name.starts_with("images/") {
            // images/<hash>.jpg → extract filename only
            let fname = name
                .strip_prefix("images/")
                .unwrap_or(name)
                .to_string();
            if !fname.is_empty() {
                let mut buf = Vec::new();
                std::io::Read::read_to_end(&mut file, &mut buf)
                    .with_context(|| format!("Cannot read image {name}"))?;
                images.insert(fname, buf);
            }
        }
        // Other files (JSON, layout PDF, etc.) are silently skipped
    }

    if markdown.is_empty() {
        bail!("No top-level .md file found in result ZIP");
    }

    Ok((markdown, images))
}
