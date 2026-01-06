use pdfium_render::prelude::*;
use std::path::Path;

fn export_pdf_to_jpegs(path: impl AsRef<Path>) -> Result<(), PdfiumError> {
    let pdfium = Pdfium::default();

    let document = pdfium.load_pdf_from_file(&path, None)?;

    for (index, page) in document.pages().iter().enumerate() {
        page.render_with_config(&PdfRenderConfig::default())?
            .as_image()
            .into_rgb8()
            .save_with_format(format!("page-{}.jpg", index), image::ImageFormat::Jpeg)
            .map_err(|_| PdfiumError::ImageError)?;
    }

    Ok(())
}

fn main() {
    export_pdf_to_jpegs(
        "/Users/frectonz/Downloads/SURVEY (Certified) - Town North - 9.22.2025 (2).pdf",
    )
    .unwrap()
}
