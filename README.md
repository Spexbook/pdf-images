# pdf-images

A Rust web service that converts PDF pages to PNG images and uploads them to Cloudflare R2 storage.

## Features

- **PDF to PNG conversion** — Renders each page of a PDF document as a high-quality PNG image
- **Cloudflare R2 integration** — Automatically uploads generated images to R2 object storage
- **Content-addressed naming** — Uses BLAKE3 hashing for deterministic, collision-free image names
- **Parallel uploads** — Uploads images concurrently for better performance
- **Large file support** — Accepts PDFs up to 250MB

## API

### `POST /`

Upload a PDF file via multipart form data.

**Request:**

```bash
curl -X POST http://localhost:3000 \
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

Each image name follows the format `{blake3_hash}-{page_index}.png`.

## Configuration

The service is configured via environment variables with the `PDF_` prefix:

| Variable           | Description                |
| ------------------ | -------------------------- |
| `PDF_ACCOUNT_ID`   | Cloudflare R2 account ID   |
| `PDF_KEY_ID`       | R2 access key ID           |
| `PDF_SECRET`       | R2 access key secret       |
| `PDF_BUCKET`       | R2 bucket name             |

## Running

```bash
# Set required environment variables
export PDF_ACCOUNT_ID="your-account-id"
export PDF_KEY_ID="your-key-id"
export PDF_SECRET="your-secret"
export PDF_BUCKET="your-bucket"

# Run the server
cargo run
```

The server starts on `http://127.0.0.1:3000`.

## Dependencies

This project requires the [PDFium](https://pdfium.googlesource.com/pdfium/) library for PDF rendering. Make sure `libpdfium` is available in your library path.
