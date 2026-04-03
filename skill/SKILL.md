---
name: mineru-ocr-cli
description: Convert PDFs, scanned images, and Office documents to LLM-friendly Markdown using the free MinerU OCR API. Extracts equations, tables, and images. No API key required.
version: 1.0.0
metadata:
  openclaw:
    emoji: "📄"
    homepage: https://github.com/neipor/mineru-cli
    requires:
      bins:
        - mineru
    install:
      - kind: brew
        formula: mineru
        bins: [mineru]
    config:
      env:
        MINERU_SERVER_URL:
          description: "Custom Gradio server URL. Override the default HuggingFace Space if it is blocked in your region (see Network section below)."
          required: false
          default: "https://opendatalab-mineru.hf.space"
        HTTPS_PROXY:
          description: "Standard HTTPS proxy. Set this to route requests through a proxy server."
          required: false
        HTTP_PROXY:
          description: "Standard HTTP proxy fallback."
          required: false
---

# mineru-ocr-cli

Convert **PDFs, scanned images, and Office documents** to **LLM-friendly Markdown** using the [MinerU OCR](https://huggingface.co/spaces/opendatalab/MinerU) API — free, no API key, GPU-backed (L40S), supports LaTeX formulas and tables.

## What this skill does

When you run `mineru <file>`, it:

1. Uploads the file to the MinerU HuggingFace Space
2. Waits for GPU-backed OCR processing (formulas, tables, images, text)
3. Downloads the result ZIP and extracts the Markdown + all images
4. Outputs clean Markdown ready for LLM consumption

**Supported formats:** PDF, DOCX, DOC, PPT, PPTX, PNG, JPG, JPEG, WebP, BMP, TIFF

---

## Installation

### Option A — Download prebuilt binary (recommended, no Rust needed)

Go to [GitHub Releases](https://github.com/neipor/mineru-cli/releases/latest) and download the binary for your platform:

| Platform | File |
|---|---|
| macOS Apple Silicon (M1/M2/M3) | `mineru-aarch64-apple-darwin.tar.gz` |
| macOS Intel | `mineru-x86_64-apple-darwin.tar.gz` |
| macOS Universal (both) | `mineru-universal-apple-darwin.tar.gz` |
| Linux x86_64 | `mineru-x86_64-unknown-linux-musl.tar.gz` |
| Linux ARM64 | `mineru-aarch64-unknown-linux-musl.tar.gz` |
| Windows x86_64 | `mineru-x86_64-pc-windows-msvc.zip` |

```bash
# Example: macOS Apple Silicon
curl -L https://github.com/neipor/mineru-cli/releases/latest/download/mineru-aarch64-apple-darwin.tar.gz \
  | tar -xz -C /usr/local/bin/
chmod +x /usr/local/bin/mineru
mineru --version
```

```bash
# Example: Linux x86_64
curl -L https://github.com/neipor/mineru-cli/releases/latest/download/mineru-x86_64-unknown-linux-musl.tar.gz \
  | tar -xz -C /usr/local/bin/
chmod +x /usr/local/bin/mineru
mineru --version
```

```powershell
# Windows (PowerShell)
Invoke-WebRequest -Uri https://github.com/neipor/mineru-cli/releases/latest/download/mineru-x86_64-pc-windows-msvc.zip -OutFile mineru.zip
Expand-Archive mineru.zip -DestinationPath $env:USERPROFILE\bin\
# Add $env:USERPROFILE\bin to your PATH
```

### Option B — Build from source (requires Rust 1.85+)

```bash
git clone https://github.com/neipor/mineru-cli.git
cd mineru-cli
cargo build --release
cp target/release/mineru /usr/local/bin/
```

---

## 🌐 Network Environment

> **⚠️ China mainland and some regions: HuggingFace is blocked.**
>
> The default server (`https://opendatalab-mineru.hf.space`) requires access to `hf.space` domains.
> These are **not accessible from mainland China** and some corporate networks without a proxy.

### Workaround 1 — System proxy (simplest)

Set a proxy before running the tool. The tool respects standard proxy env vars:

```bash
# macOS / Linux
export HTTPS_PROXY=http://127.0.0.1:7890   # replace with your proxy port
mineru document.pdf

# Or inline, single command
HTTPS_PROXY=http://127.0.0.1:7890 mineru document.pdf
```

Common proxy ports: Clash=7890, V2Ray/Xray=10809, Shadowsocks=1080, Trojan=7890.

### Workaround 2 — `MINERU_SERVER_URL` env var

If you have a self-hosted or mirrored MinerU Gradio instance:

```bash
export MINERU_SERVER_URL=https://your-mirror.example.com
mineru document.pdf
```

Or per-command with `--server-url`:

```bash
mineru document.pdf --server-url https://your-mirror.example.com
```

### Workaround 3 — Self-host MinerU

Deploy your own MinerU Gradio Space:

```bash
# Requires a machine with CUDA GPU
pip install mineru[full]
python -m mineru.cli.gradio_app --server-name 0.0.0.0 --server-port 7860
# Then use: --server-url http://your-machine:7860
```

See the [MinerU GitHub](https://github.com/opendatalab/MinerU) for Docker deployment.

---

## Usage

```
mineru [OPTIONS] <FILES>...

Options:
  -f, --format <FORMAT>       Output format: markdown (default), json, plain
  -p, --pages <N>             Max pages to process [default: 20]
      --ocr                   Force OCR mode (for scanned documents)
      --no-formulas           Disable LaTeX formula recognition
      --no-tables             Disable table recognition
  -l, --lang <LANG>           OCR language [default: ch]
  -b, --backend <BACKEND>     pipeline | vlm-auto-engine | hybrid-auto-engine
  -o, --output-dir <DIR>      Save output folder here (markdown + images/)
      --embed-images          Inline images as base64 data URIs
  -q, --quiet                 Suppress progress; pipe-friendly
      --server-url <URL>      Custom Gradio server [default: HuggingFace Space]
  -h, --help                  Print help
```

---

## Examples for LLM workflows

```bash
# Summarize a PDF with any LLM CLI
mineru paper.pdf -q | llm "Summarize this paper in 3 bullet points"

# Extract and save with images to a folder
mineru report.pdf -o ./output/

# OCR a scanned document (Chinese)
mineru scan.pdf --ocr --lang ch

# Process multiple PDFs to JSON
mineru *.pdf -f json -o ./extracted/

# Pipe to ollama
mineru doc.pdf -f plain -q | ollama run llama3 "What are the key points?"

# Use with a proxy (China mainland)
HTTPS_PROXY=http://127.0.0.1:7890 mineru document.pdf -o ./output/
```

## Output structure (with `-o`)

```
output/
├── document.md            ← Markdown with relative image links
└── document/
    └── images/
        ├── abc123.jpg     ← Extracted images
        └── def456.jpg
```

## Output structure (stdout, default)

Clean Markdown is printed to stdout. Image references are replaced with text tags:
```
[🖼 Image 1: figure1.jpg (45 KB)]
```

This keeps the output compact and LLM-readable. Use `--embed-images` for base64 inline images when you need multimodal LLM input.

---

## Backends

| Backend | Use case |
|---|---|
| `hybrid-auto-engine` | Best overall, multi-language **(default)** |
| `pipeline` | Traditional pipeline, no hallucinations |
| `vlm-auto-engine` | Highest accuracy (Chinese/English only) |

## Limits

- Max **20 pages** per document (public Space limit)
- Processing: ~1–5 seconds/page on L40S GPU
- No API key needed
