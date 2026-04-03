# mineru-cli

A Rust CLI tool that extracts text from **PDFs, images, and Office documents** using the [MinerU OCR](https://huggingface.co/spaces/opendatalab/MinerU) HuggingFace Space, and outputs **LLM-friendly Markdown** (or JSON / plain text).

```
mineru paper.pdf                          # → stdout Markdown
mineru report.pdf --pages 5 -q           # quiet mode, pipe-friendly
mineru *.pdf -o ./output -f json         # batch → JSON files
echo "$(mineru scan.pdf)" | llm summarize
```

## Features

| Feature | Details |
|---|---|
| **Formats** | PDF, DOCX, DOC, PPT, PPTX, PNG, JPG, JPEG, WebP, BMP, TIFF |
| **Output modes** | Markdown (default), JSON (with metadata), plain text |
| **Progress display** | Animated spinner with live status (suppressed with `-q`) |
| **Batch processing** | Pass multiple files; each processed independently |
| **LLM-ready** | Clean Markdown with document metadata comment header |
| **Full accuracy** | Uses MinerU's GPU-backed models (equations, tables, OCR) |

## Installation

### Prebuilt binaries (recommended)

Download from [GitHub Releases](https://github.com/neipor/mineru-cli/releases/latest):

| Platform | File |
|---|---|
| macOS Apple Silicon | `mineru-aarch64-apple-darwin.tar.gz` |
| macOS Intel | `mineru-x86_64-apple-darwin.tar.gz` |
| macOS Universal | `mineru-universal-apple-darwin.tar.gz` |
| Linux x86_64 | `mineru-x86_64-unknown-linux-musl.tar.gz` |
| Linux ARM64 | `mineru-aarch64-unknown-linux-musl.tar.gz` |
| Windows x86_64 | `mineru-x86_64-pc-windows-msvc.zip` |

```bash
# macOS Apple Silicon
curl -L https://github.com/neipor/mineru-cli/releases/latest/download/mineru-aarch64-apple-darwin.tar.gz \
  | tar -xz -C /usr/local/bin/
```

### OpenClaw / ClawHub

```bash
openclaw skills install mineru-ocr-cli
# or
clawhub install mineru-ocr-cli
```

### Build from source (requires Rust 1.85+)

```bash
git clone https://github.com/neipor/mineru-cli.git
cd mineru-cli && cargo build --release
cp target/release/mineru /usr/local/bin/
```

## 🌐 Network (China mainland / restricted regions)

> HuggingFace (`hf.space`) is **blocked in mainland China** and some corporate networks.

**Option 1 — System proxy** (Clash/V2Ray/etc.):
```bash
export HTTPS_PROXY=http://127.0.0.1:7890
mineru document.pdf
```

**Option 2 — Custom server** (self-hosted or mirror):
```bash
export MINERU_SERVER_URL=https://your-mirror.example.com
# or per-command:
mineru document.pdf --server-url https://your-mirror.example.com
```

**Option 3 — Self-host MinerU** (requires CUDA GPU):
```bash
pip install mineru[full]
python -m mineru.cli.gradio_app --server-name 0.0.0.0 --server-port 7860
mineru document.pdf --server-url http://localhost:7860
```

## Usage

```
mineru [OPTIONS] <FILES>...

Arguments:
  <FILES>...    Input files (PDF, DOCX, images, etc.)

Options:
  -f, --format <FORMAT>       Output format [default: markdown]
                              [possible values: markdown, json, plain]
  -p, --pages <N>             Max pages to process [default: 20]
      --ocr                   Force OCR (use when native text extraction fails)
      --no-formulas           Disable LaTeX formula recognition
      --no-tables             Disable table recognition
  -l, --lang <LANG>           OCR language [default: "ch (Chinese, English, Chinese Traditional)"]
  -b, --backend <BACKEND>     Processing backend [default: hybrid-auto-engine]
                              [possible values: pipeline, vlm-auto-engine, hybrid-auto-engine]
  -o, --output-dir <DIR>      Save output files + images here (stdout if omitted)
      --embed-images          Inline images as base64 data URIs
      --server-url <URL>      Custom Gradio server URL [env: MINERU_SERVER_URL]
  -q, --quiet                 Suppress progress; only content goes to stdout
  -h, --help                  Print help
  -V, --version               Print version
```

### Examples

```bash
# Extract a research paper (first 3 pages) to stdout
mineru paper.pdf --pages 3

# Force OCR on a scanned document
mineru scan.pdf --ocr --lang en

# Process a batch of PDFs to a directory as JSON
mineru docs/*.pdf -f json -o ./extracted/

# Pipe clean text into an LLM CLI (e.g. llm, ollama)
mineru report.pdf -f plain -q | ollama run llama3 "Summarize this:"

# Chinese document with VLM backend for higher accuracy
mineru chinese_doc.pdf -b vlm-auto-engine -l ch
```

## Output formats

### Markdown (default — LLM optimised)
```markdown
<!-- source: paper.pdf | processed: 2026-04-03 14:26 UTC | backend: hybrid-auto-engine | pages: 3 -->

# Attention Is All You Need

## Abstract
The dominant sequence transduction models are based on...
```
Headings, LaTeX math (`\(...\)` / `\[...\]`), tables, and lists are preserved.

### JSON
```json
{
  "meta": {
    "source_file": "paper.pdf",
    "processed_at": "2026-04-03T14:26:00Z",
    "backend": "hybrid-auto-engine",
    "pages": 3,
    "ocr": false,
    "formula": true,
    "table": true,
    "language": "ch (Chinese, English, Chinese Traditional)"
  },
  "content": "# Attention Is All You Need\n\n...",
  "status_log": ["Preparing request...", "Processing on server (0.0s)", "Completed"]
}
```

### Plain text
Markdown syntax stripped — suitable for pure text pipelines.

## How it works

This tool calls the public [MinerU OCR HuggingFace Space](https://huggingface.co/spaces/opendatalab/MinerU) via its Gradio 6 queue API:

1. **Upload** — `POST /gradio_api/upload` with the file as multipart
2. **Queue** — `POST /gradio_api/queue/join` (fn_index=8, `convert_to_markdown_stream`)
3. **Stream** — `GET /gradio_api/queue/data` SSE stream, parsing Gradio 6 patch events
4. **Extract** — downloads the result ZIP and extracts the `.md` file
5. **Format** — cleans and formats the markdown for LLM consumption

The Space uses `opendatalab/PDF-Extract-Kit-1.0` and `opendatalab/MinerU2.5-2509-1.2B` models running on L40S GPU.

## Backends

| Backend | Description |
|---|---|
| `hybrid-auto-engine` | High-precision hybrid parsing, multi-language **(default)** |
| `pipeline` | Traditional multi-model pipeline, hallucination-free |
| `vlm-auto-engine` | VLM-based, Chinese/English only, highest accuracy |

## Limitations

- Maximum 20 pages per document (Space limit)
- Requires internet access (calls HuggingFace Space)
- No API key required (public Space)
- Processing time: ~1-5 seconds per page on L40S GPU

## License

MIT
