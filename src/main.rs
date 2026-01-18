use aws_sdk_s3::{
    self as s3, error::SdkError, operation::put_object::PutObjectError, primitives::ByteStream,
};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Multipart, Query, State, multipart::MultipartError},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
};
use parenv::Environment;
use pdfium_render::prelude::{PdfRenderConfig, Pdfium, PdfiumError};
use serde::{Deserialize, Serialize};
use std::io::Cursor;
use thiserror::Error;
use tokio::task::{JoinError, JoinSet};
use tower_http::limit::RequestBodyLimitLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

type BoxStr = Box<str>;

#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
enum OutputFormat {
    #[default]
    Png,
    Jpeg,
    Gif,
    #[serde(rename = "webp")]
    WebP,
    Pnm,
    Tiff,
    Tga,
    Bmp,
    Ico,
    Hdr,
    #[serde(rename = "openexr")]
    OpenExr,
    Farbfeld,
    Avif,
    Qoi,
}

impl OutputFormat {
    fn as_image_format(&self) -> image::ImageFormat {
        match self {
            OutputFormat::Png => image::ImageFormat::Png,
            OutputFormat::Jpeg => image::ImageFormat::Jpeg,
            OutputFormat::Gif => image::ImageFormat::Gif,
            OutputFormat::WebP => image::ImageFormat::WebP,
            OutputFormat::Pnm => image::ImageFormat::Pnm,
            OutputFormat::Tiff => image::ImageFormat::Tiff,
            OutputFormat::Tga => image::ImageFormat::Tga,
            OutputFormat::Bmp => image::ImageFormat::Bmp,
            OutputFormat::Ico => image::ImageFormat::Ico,
            OutputFormat::Hdr => image::ImageFormat::Hdr,
            OutputFormat::OpenExr => image::ImageFormat::OpenExr,
            OutputFormat::Farbfeld => image::ImageFormat::Farbfeld,
            OutputFormat::Avif => image::ImageFormat::Avif,
            OutputFormat::Qoi => image::ImageFormat::Qoi,
        }
    }

    fn extension(&self) -> &'static str {
        match self {
            OutputFormat::Png => "png",
            OutputFormat::Jpeg => "jpg",
            OutputFormat::Gif => "gif",
            OutputFormat::WebP => "webp",
            OutputFormat::Pnm => "pnm",
            OutputFormat::Tiff => "tiff",
            OutputFormat::Tga => "tga",
            OutputFormat::Bmp => "bmp",
            OutputFormat::Ico => "ico",
            OutputFormat::Hdr => "hdr",
            OutputFormat::OpenExr => "exr",
            OutputFormat::Farbfeld => "ff",
            OutputFormat::Avif => "avif",
            OutputFormat::Qoi => "qoi",
        }
    }
}

#[derive(Debug, Deserialize)]
struct UploadQuery {
    #[serde(default)]
    format: OutputFormat,
    token: Option<String>,
}

#[derive(Debug, Environment)]
#[parenv(prefix = "PDF_")]
struct Env {
    /// The R2 account ID.
    account_id: String,
    /// The R2 access key ID.
    key_id: String,
    /// The R2 access key secret.
    secret: String,
    /// The R2 bucket.
    bucket: String,
    /// The request body limit in megabytes.
    body_limit: Option<usize>,
    /// Optional security token for request authentication.
    token: Option<String>,
    /// The address the server will listen on.
    address: Option<String>,
}

#[derive(Clone)]
struct AppState {
    storage: ObjectStorage,
    token: Option<BoxStr>,
}

#[derive(Clone)]
struct ObjectStorage {
    bucket: BoxStr,
    client: s3::Client,
}

impl ObjectStorage {
    async fn new(env: &Env) -> Self {
        let config = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .endpoint_url(format!(
                "https://{}.r2.cloudflarestorage.com",
                env.account_id
            ))
            .credentials_provider(aws_sdk_s3::config::Credentials::new(
                env.key_id.to_owned(),
                env.secret.to_owned(),
                None,
                None,
                "R2",
            ))
            .region("auto")
            .load()
            .await;

        Self {
            bucket: env.bucket.to_owned().into_boxed_str(),
            client: s3::Client::new(&config),
        }
    }

    pub async fn put_image(&self, image: PdfImage) -> Result<String, AppError> {
        self.client
            .put_object()
            .bucket(self.bucket.as_ref())
            .key(&image.name)
            .body(image.stream)
            .send()
            .await
            .map_err(Box::new)?;

        Ok(image.name)
    }
}

struct PdfImage {
    name: String,
    stream: ByteStream,
}

fn process_pdf(bytes: &[u8], format: OutputFormat) -> Result<Vec<PdfImage>, AppError> {
    let env_bindings = std::env::var("PDFIUM_DYNAMIC_LIB_PATH")
        .map(|path| {
            let path = Pdfium::pdfium_platform_library_name_at_path(&path);
            Pdfium::bind_to_library(path)
        })
        .ok();

    let current_dir_bindings =
        Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path("./"));

    let system_bindings = Pdfium::bind_to_system_library();

    let bindings = env_bindings.unwrap_or(current_dir_bindings.or(system_bindings))?;

    let pdfium = Pdfium::new(bindings);
    let document = pdfium.load_pdf_from_byte_slice(bytes, None)?;

    let id = blake3::hash(bytes).to_hex().to_string();
    let ext = format.extension();
    let image_format = format.as_image_format();

    let images = document
        .pages()
        .iter()
        .enumerate()
        .flat_map(|(idx, page)| {
            let mut output = Cursor::new(Vec::new());

            page.render_with_config(&PdfRenderConfig::default())
                .ok()?
                .as_image()
                .adjust_contrast(0.1)
                .write_to(&mut output, image_format)
                .ok()?;

            let stream = ByteStream::from(output.into_inner());

            Some(PdfImage {
                name: format!("{id}-{idx}.{ext}"),
                stream,
            })
        })
        .collect::<Vec<_>>();

    Ok(images)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let env = Env::parse();
    let storage = ObjectStorage::new(&env).await;
    let state = AppState {
        storage,
        token: env.token.map(|t| t.into_boxed_str()),
    };

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                format!("{}=debug,tower_http=debug", env!("CARGO_CRATE_NAME")).into()
            }),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let body_limit = env.body_limit.unwrap_or(250) * 1024 * 1024;

    let app = Router::new()
        .route("/", post(handle_pdf_upload))
        .layer(DefaultBodyLimit::disable())
        .layer(RequestBodyLimitLayer::new(body_limit))
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(state);

    let address = env.address.as_deref().unwrap_or("127.0.0.1:3000");
    let listener = tokio::net::TcpListener::bind(address).await?;

    tracing::debug!("listening on {address}");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("failed to install Ctrl+C handler");
    tracing::info!("shutting down gracefully...")
}

async fn handle_pdf_upload(
    State(state): State<AppState>,
    Query(query): Query<UploadQuery>,
    mut multipart: Multipart,
) -> Result<Json<UploadResponse>, AppError> {
    // Validate token if one is configured
    if let Some(expected_token) = &state.token {
        match &query.token {
            Some(provided_token) if provided_token.as_str() == expected_token.as_ref() => {}
            _ => return Err(AppError::Unauthorized),
        }
    }

    let field = multipart
        .next_field()
        .await?
        .ok_or_else(|| AppError::FieldNotFound)?;

    let data = field.bytes().await?;
    let format = query.format;
    let images = tokio::task::spawn_blocking(move || process_pdf(&data, format)).await??;

    let mut set = JoinSet::new();

    for image in images {
        let storage = state.storage.clone();
        set.spawn(async move { storage.put_image(image).await });
    }

    let images = set
        .join_all()
        .await
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?;

    Ok(Json(UploadResponse {
        success: true,
        images,
    }))
}

#[derive(Debug, Serialize)]
struct UploadResponse {
    success: bool,
    images: Vec<String>,
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
    #[error("s3 error: {0}")]
    S3(#[from] Box<SdkError<PutObjectError>>),
    #[error("unauthorized: invalid or missing token")]
    Unauthorized,
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
            AppError::S3(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Internal Server Error".to_owned(),
            ),
            AppError::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                "Unauthorized: invalid or missing token".to_owned(),
            ),
        };

        (status, Json(ErrorResponse { message })).into_response()
    }
}
