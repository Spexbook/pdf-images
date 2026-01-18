# pdf-images

A Rust web service that converts PDF pages to images and uploads them to Cloudflare R2 storage.

## Features

- **PDF to image conversion** — Renders each page of a PDF document as a high-quality image
- **Page range selection** — Convert specific pages instead of the entire document (e.g., `?pages=1-5,8,10`)
- **Resolution control** — Scale output images up or down (e.g., `?scale=2.0` for 2x size)
- **Multiple output formats** — Supports PNG, JPEG, GIF, WebP, TIFF, BMP, and more
- **Cloudflare R2 integration** — Automatically uploads generated images to R2 object storage
- **Content-addressed naming** — Uses BLAKE3 hashing for deterministic, collision-free image names
- **Parallel uploads** — Uploads images concurrently for better performance
- **Large file support** — Accepts PDFs up to 250MB (configurable)
- **Encrypted PDF support** — Open password-protected PDFs with the `password` parameter
- **Health check endpoint** — `GET /health` for Kubernetes and Docker healthchecks

## API

### `GET /health`

Health check endpoint for container orchestration (Kubernetes, Docker).

**Request:**

```bash
curl http://localhost:3000/health
```

**Response:**

```json
{
  "status": "ok"
}
```

Returns HTTP 200 when the service is running. No authentication required.

---

### `POST /`

Upload a PDF file via multipart form data.

**Query Parameters:**

| Parameter  | Description | Default |
|------------|-------------|---------|
| `format`   | Output image format | `png` |
| `pages`    | Page range to convert (e.g., `1-5,8,10`) | all pages |
| `scale`    | Scale factor for output images (0.1-10.0) | `1.0` |
| `password` | Password for encrypted PDFs | — |
| `token`    | Security token (required if `PDF_TOKEN` is set) | — |

**Page Range Format:**

| Example | Description |
|---------|-------------|
| `5` | Page 5 only |
| `1-5` | Pages 1 through 5 |
| `1-5,8,10` | Pages 1-5, 8, and 10 |
| _(omitted)_ | All pages |

**Supported Formats:**

| Format     | Value      | Extension |
|------------|------------|-----------|
| PNG        | `png`      | .png      |
| JPEG       | `jpeg`     | .jpg      |
| GIF        | `gif`      | .gif      |
| WebP       | `webp`     | .webp     |
| PNM        | `pnm`      | .pnm      |
| TIFF       | `tiff`     | .tiff     |
| TGA        | `tga`      | .tga      |
| BMP        | `bmp`      | .bmp      |
| ICO        | `ico`      | .ico      |
| HDR        | `hdr`      | .hdr      |
| OpenEXR    | `openexr`  | .exr      |
| Farbfeld   | `farbfeld` | .ff       |
| AVIF       | `avif`     | .avif     |
| QOI        | `qoi`      | .qoi      |

**Request:**

```bash
# Default (PNG)
curl -X POST http://localhost:3000 \
  -F "file=@document.pdf"

# JPEG format
curl -X POST "http://localhost:3000?format=jpeg" \
  -F "file=@document.pdf"

# WebP format
curl -X POST "http://localhost:3000?format=webp" \
  -F "file=@document.pdf"

# With security token (if PDF_TOKEN is configured)
curl -X POST "http://localhost:3000?token=your-secret-token" \
  -F "file=@document.pdf"

# With both format and token
curl -X POST "http://localhost:3000?format=jpeg&token=your-secret-token" \
  -F "file=@document.pdf"

# Convert only specific pages
curl -X POST "http://localhost:3000?pages=1-5" \
  -F "file=@document.pdf"

# Convert pages 1-3, 7, and 10-12
curl -X POST "http://localhost:3000?pages=1-3,7,10-12" \
  -F "file=@document.pdf"

# Scale output to 2x size (higher resolution)
curl -X POST "http://localhost:3000?scale=2.0" \
  -F "file=@document.pdf"

# Scale output to 0.5x size (smaller images)
curl -X POST "http://localhost:3000?scale=0.5" \
  -F "file=@document.pdf"

# Combine scale with format and pages
curl -X POST "http://localhost:3000?scale=2.0&format=jpeg&pages=1-3" \
  -F "file=@document.pdf"

# Open a password-protected PDF
curl -X POST "http://localhost:3000?password=secretpassword" \
  -F "file=@encrypted.pdf"
```

**Response:**

```json
{
  "success": true,
  "images": [
    "a1b2c3d4...-0.png",
    "a1b2c3d4...-1.png",
    "a1b2c3d4...-2.png"
  ]
}
```

Each image name follows the format `{blake3_hash}-{page_index}.{extension}`.

## Configuration

The service is configured via environment variables with the `PDF_` prefix:

| Variable           | Description                                           |
| ------------------ | ----------------------------------------------------- |
| `PDF_ACCOUNT_ID`   | Cloudflare R2 account ID                              |
| `PDF_KEY_ID`       | R2 access key ID                                      |
| `PDF_SECRET`       | R2 access key secret                                  |
| `PDF_BUCKET`       | R2 bucket name                                        |
| `PDF_BODY_LIMIT`   | Request body limit in MB (default: 250)               |
| `PDF_TOKEN`        | Security token for request authentication (optional)  |
| `PDF_ADDRESS`      | Server listen address (default: `127.0.0.1:3000`)     |

## Running

```bash
# Set required environment variables
export PDF_ACCOUNT_ID="your-account-id"
export PDF_KEY_ID="your-key-id"
export PDF_SECRET="your-secret"
export PDF_BUCKET="your-bucket"

# Optional: Enable token authentication
export PDF_TOKEN="your-secret-token"

# Optional: Customize server address (default: 127.0.0.1:3000)
export PDF_ADDRESS="0.0.0.0:8080"

# Run the server
cargo run
```

The server starts on `http://127.0.0.1:3000` by default, or on the address specified by `PDF_ADDRESS`.

### Docker

```bash
docker run \
  -e PDF_ACCOUNT_ID="your-account-id" \
  -e PDF_KEY_ID="your-key-id" \
  -e PDF_SECRET="your-secret" \
  -e PDF_BUCKET="your-bucket" \
  -e PDF_TOKEN="your-secret-token" \
  -e PDF_ADDRESS="0.0.0.0:3000" \
  -p 3000:3000 \
  frectonz/pdf-images:latest
```

## Dependencies

This project requires the [PDFium](https://pdfium.googlesource.com/pdfium/) library for PDF rendering. Make sure `libpdfium` is available in your library path.
