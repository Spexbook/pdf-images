# pdf-images

A Rust web service that converts PDF pages to images and uploads them to Cloudflare R2 storage.

## Features

- **PDF to image conversion** — Renders each page of a PDF document as a high-quality image
- **Multiple output formats** — Supports PNG, JPEG, GIF, WebP, TIFF, BMP, and more
- **Cloudflare R2 integration** — Automatically uploads generated images to R2 object storage
- **Content-addressed naming** — Uses BLAKE3 hashing for deterministic, collision-free image names
- **Parallel uploads** — Uploads images concurrently for better performance
- **Large file support** — Accepts PDFs up to 250MB (configurable)

## API

### `POST /`

Upload a PDF file via multipart form data.

**Query Parameters:**

| Parameter | Description | Default |
|-----------|-------------|---------|
| `format`  | Output image format | `png` |
| `token`   | Security token (required if `PDF_TOKEN` is set) | — |

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

# Run the server
cargo run
```

The server starts on `http://127.0.0.1:3000`.

## Dependencies

This project requires the [PDFium](https://pdfium.googlesource.com/pdfium/) library for PDF rendering. Make sure `libpdfium` is available in your library path.
