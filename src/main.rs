use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Multipart, multipart::MultipartError},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
};
use pdfium_render::prelude::*;
use serde::Serialize;
use thiserror::Error;
use tokio::task::JoinError;
use tower_http::limit::RequestBodyLimitLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

fn export_pdf_to_jpegs(bytes: &[u8]) -> Result<String, AppError> {
    let pdfium = Pdfium::default();
    let document = pdfium.load_pdf_from_byte_slice(bytes, None)?;

    for (index, page) in document.pages().iter().enumerate() {
        page.render_with_config(&PdfRenderConfig::default())?
            .as_image()
            .into_rgb8()
            .save_with_format(format!("page-{}.jpg", index), image::ImageFormat::Jpeg)
            .map_err(|_| PdfiumError::ImageError)?;
    }

    let id = blake3::hash(bytes).to_hex().to_string();
    Ok(id)
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

async fn handle_pdf_upload(mut multipart: Multipart) -> Result<Json<UploadResponse>, AppError> {
    let field = multipart
        .next_field()
        .await?
        .ok_or_else(|| AppError::FieldNotFound)?;

    let data = field.bytes().await?;
    tokio::task::spawn_blocking(move || export_pdf_to_jpegs(&data)).await??;

    Ok(Json(UploadResponse { success: true }))
}

#[derive(Debug, Serialize)]
struct UploadResponse {
    success: bool,
}

#[derive(Debug, Error)]
enum AppError {
    #[error("pdfium error: {0}")]
    Pdfium(#[from] PdfiumError),
    #[error("form error: {0}")]
    Multipart(#[from] MultipartError),
    #[error("no field found in multipart form")]
    FieldNotFound,
    #[error("task error: {0}")]
    Task(#[from] JoinError),
}
#[derive(Serialize)]
struct ErrorResponse {
    message: String,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        tracing::error!("{self}");

        let (status, message) = match self {
            AppError::Pdfium(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Internal Server Error".to_owned(),
            ),
            AppError::Multipart(_) => (
                StatusCode::BAD_REQUEST,
                "Failed to read PDF file from request".to_owned(),
            ),
            AppError::FieldNotFound => (
                StatusCode::BAD_REQUEST,
                "Form does not contain any fields".to_owned(),
            ),
            AppError::Task(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Internal Server Error".to_owned(),
            ),
        };

        (status, Json(ErrorResponse { message })).into_response()
    }
}
