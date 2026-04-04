---
name: mineru-ocr-cli
description: >
  Convert PDFs, scanned images, and Office documents to LLM-friendly Markdown
  using the free MinerU OCR API. Extracts equations, tables, and images with
  GPU-backed accuracy. No API key required. Supports batch processing, multiple
  output formats (Markdown / JSON / plain text), and LLM pipe workflows.
version: 1.0.0
metadata:
  openclaw:
    emoji: "📄"
    homepage: https://github.com/neipor/mineru-cli
    tags: [pdf, ocr, markdown, llm, document, extract]
    requires:
      bins:
        - mineru
    install:
      - kind: release
        url: "https://github.com/neipor/mineru-cli/releases/latest"
        bins: [mineru]
    config:
      env:
        MINERU_SERVER_URL:
          description: >
            Custom Gradio server URL. Overrides the default HuggingFace Space
            when it is blocked in your region (e.g. mainland China). Set to the
            base URL of any self-hosted or mirrored MinerU Gradio instance.
          required: false
          default: "https://opendatalab-mineru.hf.space"
        HTTPS_PROXY:
          description: >
            Standard HTTPS proxy URL (e.g. http://127.0.0.1:7890). The tool
            honours this automatically — useful for regions where hf.space is
            inaccessible. Common ports: Clash=7890, V2Ray=10809, SS=1080.
          required: false
        HTTP_PROXY:
          description: >
            Standard HTTP proxy fallback. Used when HTTPS_PROXY is not set.
          required: false
---

# mineru-ocr-cli

Convert **PDFs, scanned images, and Office documents** to **LLM-friendly
Markdown** using the [MinerU OCR HuggingFace Space](https://huggingface.co/spaces/opendatalab/MinerU)
— free, no API key, GPU-backed (L40S), with support for LaTeX formulas, tables,
and embedded images.

```
mineru paper.pdf                           # Markdown to stdout
mineru scan.pdf --ocr --lang en            # Force OCR
mineru *.pdf -f json -o ./output/          # Batch → JSON files
mineru report.pdf -f plain -q | llm "Summarize:"
```

---

## What this skill does

When you run `mineru <file>`, it:

1. **Validates** the file extension against supported formats
2. **Uploads** the file to the MinerU HuggingFace Space via multipart POST
3. **Queues** a conversion job on the remote GPU (L40S)
4. **Streams** the live status via Gradio 6 SSE queue events
5. **Downloads** the result ZIP and extracts Markdown + all images
6. **Formats** the output (Markdown / JSON / plain text) for LLM consumption

**Supported input formats:** PDF, DOCX, DOC, PPT, PPTX, PNG, JPG, JPEG, WebP, BMP, TIFF

---

## Installation

### Option A — One-liner install (macOS / Linux, recommended)

Pick the command for your platform and paste it into your terminal.
The binary is placed in `/usr/local/bin/` which is already on your `PATH`.

```bash
# macOS — Universal binary (Intel + Apple Silicon)
curl -fsSL https://github.com/neipor/mineru-cli/releases/latest/download/mineru-universal-apple-darwin.tar.gz \
  | tar -xz --strip-components=1 -C /usr/local/bin/ mineru-*/mineru
chmod +x /usr/local/bin/mineru && mineru --version
```

```bash
# Linux x86_64  (static musl — works on Ubuntu, Debian, Alpine, RHEL, …)
curl -fsSL https://github.com/neipor/mineru-cli/releases/latest/download/mineru-x86_64-unknown-linux-musl.tar.gz \
  | tar -xz --strip-components=1 -C /usr/local/bin/ mineru-*/mineru
chmod +x /usr/local/bin/mineru && mineru --version
```

```bash
# Linux ARM64  (Raspberry Pi 4/5, AWS Graviton, Oracle Ampere, …)
curl -fsSL https://github.com/neipor/mineru-cli/releases/latest/download/mineru-aarch64-unknown-linux-musl.tar.gz \
  | tar -xz --strip-components=1 -C /usr/local/bin/ mineru-*/mineru
chmod +x /usr/local/bin/mineru && mineru --version
```

> **Tip — install without root:** Replace `/usr/local/bin/` with `$HOME/.local/bin/`
> and make sure `$HOME/.local/bin` is in your `PATH` (add
> `export PATH="$HOME/.local/bin:$PATH"` to `~/.bashrc` or `~/.zshrc`).

### Option B — Windows (PowerShell)

```powershell
# Download and extract
$ver = (Invoke-RestMethod "https://api.github.com/repos/neipor/mineru-cli/releases/latest").tag_name
$url = "https://github.com/neipor/mineru-cli/releases/download/$ver/mineru-$ver-x86_64-pc-windows-msvc.zip"
Invoke-WebRequest -Uri $url -OutFile "$env:TEMP\mineru.zip"
Expand-Archive "$env:TEMP\mineru.zip" -DestinationPath "$env:TEMP\mineru-pkg" -Force

# Copy to a directory that is already on PATH (System32 alternative: %LOCALAPPDATA%\Programs\mineru)
$dest = "$env:LOCALAPPDATA\Programs\mineru"
New-Item -ItemType Directory -Force $dest | Out-Null
Copy-Item "$env:TEMP\mineru-pkg\*\mineru.exe" $dest

# Permanently add to user PATH (takes effect in new shells)
$currentPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($currentPath -notlike "*$dest*") {
    [Environment]::SetEnvironmentVariable("Path", "$currentPath;$dest", "User")
    Write-Host "Added $dest to PATH — restart your terminal."
}

# Verify
& "$dest\mineru.exe" --version
```

### Option C — Build from source (requires Rust 1.85+)

```bash
git clone https://github.com/neipor/mineru-cli.git
cd mineru-cli
cargo build --release

# Install to /usr/local/bin/ (system-wide)
sudo cp target/release/mineru /usr/local/bin/

# Or install to ~/.cargo/bin/ (user-level, already on PATH after `rustup` setup)
cargo install --path .
```

### Option D — cargo install (crates.io)

```bash
cargo install mineru-cli
# Binary is placed at ~/.cargo/bin/mineru
```

> If `~/.cargo/bin` is not on your `PATH`, add `export PATH="$HOME/.cargo/bin:$PATH"`
> to your shell profile.

---

## Verify the installation

```bash
mineru --version   # should print: mineru x.y.z
mineru --help      # full option reference
```

---

## 🌐 Network Environment

> **⚠️ China mainland and some regions: HuggingFace is blocked.**
>
> The default server (`https://opendatalab-mineru.hf.space`) requires access to
> `hf.space` domains, which are **not reachable from mainland China** and some
> corporate networks without a proxy.

### Workaround 1 — System proxy (simplest)

The tool automatically honours standard `HTTPS_PROXY` / `HTTP_PROXY` env vars:

```bash
# macOS / Linux — set once for the session
export HTTPS_PROXY=http://127.0.0.1:7890   # replace with your proxy port
mineru document.pdf

# Or inline for a single command
HTTPS_PROXY=http://127.0.0.1:7890 mineru document.pdf
```

```powershell
# Windows PowerShell
$env:HTTPS_PROXY = "http://127.0.0.1:7890"
mineru document.pdf
```

Common proxy ports: Clash / ClashX = **7890** · V2Ray / Xray = **10809** ·
Shadowsocks = **1080** · Trojan = **7890**

### Workaround 2 — `MINERU_SERVER_URL` env var

If you have access to a self-hosted or mirrored MinerU Gradio instance:

```bash
export MINERU_SERVER_URL=https://your-mirror.example.com
mineru document.pdf
```

Or pass it per-command:

```bash
mineru document.pdf --server-url https://your-mirror.example.com
```

### Workaround 3 — Self-host MinerU (requires a CUDA GPU)

```bash
pip install mineru[full]
python -m mineru.cli.gradio_app --server-name 0.0.0.0 --server-port 7860
mineru document.pdf --server-url http://localhost:7860
```

See the [MinerU GitHub](https://github.com/opendatalab/MinerU) for full Docker
and cloud deployment instructions.

---

## Full option reference

```
mineru [OPTIONS] <FILES>...

Arguments:
  <FILES>...    One or more input files (globs work in most shells: *.pdf)

Options:
  -f, --format <FORMAT>
          Output format [default: markdown]
          Possible values: markdown, json, plain

  -p, --pages <N>
          Maximum pages to process per document [default: 20]
          The public MinerU Space enforces a hard 20-page limit.

      --ocr
          Force OCR mode. Use when native text extraction fails (scanned PDFs,
          image-only documents). Slightly slower but more thorough.

      --no-formulas
          Disable LaTeX formula recognition (faster for documents without math).

      --no-tables
          Disable table structure recognition.

  -l, --lang <LANG>
          OCR language hint [default: "ch (Chinese, English, Chinese Traditional)"]
          Common values: ch, en, fr, de, ja, ko, ar
          Affects OCR character set; does not restrict document content.

  -b, --backend <BACKEND>
          Processing backend [default: hybrid-auto-engine]
          Possible values:
            pipeline          — Traditional multi-model pipeline, hallucination-free
            vlm-auto-engine   — VLM-based, Chinese/English only, highest accuracy
            hybrid-auto-engine — Hybrid, best overall multi-language (default)

  -o, --output-dir <DIR>
          Save output Markdown (or JSON / txt) plus an images/ sub-folder here.
          When omitted, content goes to stdout and image refs are shown as tags.

      --embed-images
          Inline all images as base64 data URIs in the Markdown output.
          Useful for multimodal LLMs that accept self-contained Markdown.
          Ignored when --output-dir is set (images are always saved as files).

      --server-url <URL>
          Custom Gradio server base URL [env: MINERU_SERVER_URL]
          [default: https://opendatalab-mineru.hf.space]

  -q, --quiet
          Suppress all progress output. Only the document content is written to
          stdout — ideal for shell pipelines.

  -h, --help      Print help
  -V, --version   Print version
```

---

## Usage examples

### Basic extraction

```bash
# Extract a PDF to stdout (Markdown)
mineru paper.pdf

# Extract only the first 5 pages
mineru report.pdf --pages 5

# Force OCR on a scanned document
mineru scan.pdf --ocr

# English-only document, disable formula/table detection for speed
mineru letter.pdf --lang en --no-formulas --no-tables
```

### Save to files

```bash
# Save Markdown + images to ./output/
mineru report.pdf -o ./output/

# Save as JSON (includes metadata block)
mineru report.pdf -f json -o ./output/

# Batch: convert all PDFs in current directory
mineru *.pdf -o ./extracted/

# Batch with progress suppressed (CI / scripting)
mineru *.pdf -o ./extracted/ -q
```

### LLM pipe workflows

```bash
# Summarize with the llm CLI
mineru paper.pdf -q | llm "Summarize in 5 bullet points"

# Chat with a document via ollama
mineru doc.pdf -f plain -q | ollama run llama3 "What are the key findings?"

# Feed a PDF to GPT-4 via the openai CLI
mineru paper.pdf -q | openai api chat.completions.create \
  -m gpt-4o --message "$(cat)"

# Embed images for multimodal LLMs
mineru figure_heavy_paper.pdf --embed-images -q | llm -m gpt-4o "Describe the figures"
```

### Advanced

```bash
# VLM backend for highest Chinese accuracy
mineru chinese_report.pdf -b vlm-auto-engine -l ch

# Process a PPTX presentation
mineru slides.pptx -o ./slides_output/

# Via proxy (China mainland)
HTTPS_PROXY=http://127.0.0.1:7890 mineru document.pdf -o ./output/

# Self-hosted server
mineru document.pdf --server-url http://192.168.1.100:7860
```

---

## Output formats

### `markdown` (default) — LLM-optimised

```markdown
<!-- source: paper.pdf | processed: 2026-04-03 14:26 UTC | backend: hybrid-auto-engine | pages: 3 -->

# Attention Is All You Need

## Abstract
The dominant sequence transduction models...

$$\text{Attention}(Q, K, V) = \text{softmax}\!\left(\frac{QK^T}{\sqrt{d_k}}\right)V$$
```

Headings, LaTeX math (`\(...\)` / `\[...\]`), tables, and lists are preserved.

### `json` — structured with metadata

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

### `plain` — stripped text

Markdown syntax removed — suitable for pure-text pipelines or word-count tools.

---

## Output directory layout (when `-o` is used)

```
output/
├── document.md            ← Markdown with relative image links
└── document/
    └── images/
        ├── abc123.jpg     ← Extracted images referenced from the .md
        └── def456.jpg
```

When writing to stdout (no `-o`), image references appear as compact tags:
```
[🖼 Image 1: figure1.jpg (45 KB)]
```

Use `--embed-images` to replace these with base64 `data:` URIs for multimodal
LLM input.

---

## Backends

| Backend | Best for | Notes |
|---|---|---|
| `hybrid-auto-engine` | General use, multi-language | **Default** |
| `pipeline` | Hallucination-sensitive workloads | Traditional multi-model, no generative steps |
| `vlm-auto-engine` | Highest accuracy (Chinese / English) | VLM-based; slower but most precise |

---

## Limits and performance

| Constraint | Value |
|---|---|
| Max pages (public Space) | 20 per document |
| Typical processing time | 1–5 seconds / page on L40S GPU |
| API key required | None |
| Internet required | Yes (calls HuggingFace Space) |
| Max file size | ~50 MB (Space limit) |

---

## How it works (internals)

The tool calls the public [MinerU HuggingFace Space](https://huggingface.co/spaces/opendatalab/MinerU)
via the Gradio 6 queue API:

1. **Upload** — `POST /gradio_api/upload` with the file as multipart/form-data
2. **Queue** — `POST /gradio_api/queue/join` (fn_index=8, `convert_to_markdown_stream`)
3. **Stream** — `GET /gradio_api/queue/data` SSE stream, parsing Gradio 6 patch events
4. **Extract** — downloads the result ZIP, extracts the `.md` file and all images
5. **Format** — cleans and re-formats Markdown for LLM consumption

The Space runs `opendatalab/PDF-Extract-Kit-1.0` and
`opendatalab/MinerU2.5-2509-1.2B` models on an L40S GPU.
