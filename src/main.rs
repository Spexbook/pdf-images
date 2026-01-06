use axum::{
    Router,
    extract::{DefaultBodyLimit, Multipart},
    routing::post,
};
use pdfium_render::prelude::*;
use tower_http::limit::RequestBodyLimitLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

fn export_pdf_to_jpegs(bytes: &[u8]) -> Result<(), PdfiumError> {
    let pdfium = Pdfium::default();
    let document = pdfium.load_pdf_from_byte_slice(bytes, None)?;

    for (index, page) in document.pages().iter().enumerate() {
        page.render_with_config(&PdfRenderConfig::default())?
            .as_image()
            .into_rgb8()
            .save_with_format(format!("page-{}.jpg", index), image::ImageFormat::Jpeg)
            .map_err(|_| PdfiumError::ImageError)?;
    }

    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                format!("{}=debug,tower_http=debug", env!("CARGO_CRATE_NAME")).into()
            }),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let app = Router::new()
        .route("/", post(handle_pdf_upload))
        .layer(DefaultBodyLimit::disable())
        .layer(RequestBodyLimitLayer::new(
            250 * 1024 * 1024, /* 250mb */
        ))
        .layer(tower_http::trace::TraceLayer::new_for_http());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();

    tracing::debug!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await?;

    Ok(())
}

async fn handle_pdf_upload(mut multipart: Multipart) {
    while let Some(field) = multipart.next_field().await.unwrap() {
        let data = field.bytes().await.unwrap();
    }
}
