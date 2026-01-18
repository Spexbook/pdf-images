use aws_sdk_s3::{
    self as s3, error::SdkError, operation::put_object::PutObjectError, primitives::ByteStream,
};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Multipart, Query, State, multipart::MultipartError},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use parenv::Environment;
use pdfium_render::prelude::{PdfRenderConfig, Pdfium, PdfiumError};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
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

struct PageSelection(HashSet<usize>);

impl PageSelection {
    fn contains(&self, page: usize) -> bool {
        self.0.contains(&page)
    }

    fn validate(&self, total_pages: usize) -> Result<(), AppError> {
        if let Some(&max_page) = self.0.iter().max()
            && max_page >= total_pages
        {
            return Err(AppError::InvalidPageRange(format!(
                "page {} is out of range (document has {} pages)",
                max_page + 1,
                total_pages
            )));
        }
        Ok(())
    }
}

impl std::str::FromStr for PageSelection {
    type Err = AppError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let mut pages = HashSet::new();

        for part in input.split(',') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }

            if let Some((start, end)) = part.split_once('-') {
                let start: usize = start.trim().parse().map_err(|_| {
                    AppError::InvalidPageRange(format!("invalid number: {}", start))
                })?;
                let end: usize = end
                    .trim()
                    .parse()
                    .map_err(|_| AppError::InvalidPageRange(format!("invalid number: {}", end)))?;

                if start == 0 || end == 0 {
                    return Err(AppError::InvalidPageRange(
                        "page numbers must be 1 or greater".to_string(),
                    ));
                }
                if start > end {
                    return Err(AppError::InvalidPageRange(format!(
                        "invalid range: start ({}) > end ({})",
                        start, end
                    )));
                }

                // Convert from 1-indexed to 0-indexed
                for page in (start - 1)..end {
                    pages.insert(page);
                }
            } else {
                let page: usize = part
                    .parse()
                    .map_err(|_| AppError::InvalidPageRange(format!("invalid number: {}", part)))?;

                if page == 0 {
                    return Err(AppError::InvalidPageRange(
                        "page numbers must be 1 or greater".to_string(),
                    ));
                }

                // Convert from 1-indexed to 0-indexed
                pages.insert(page - 1);
            }
        }

        if pages.is_empty() {
            return Err(AppError::InvalidPageRange(
                "no valid pages specified".to_string(),
            ));
        }

        Ok(PageSelection(pages))
    }
}

#[derive(Debug, Deserialize)]
struct UploadQuery {
    #[serde(default)]
    format: OutputFormat,
    token: Option<String>,
    pages: Option<String>,
    scale: Option<f32>,
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

fn process_pdf(bytes: &[u8], query: UploadQuery) -> Result<Vec<PdfImage>, AppError> {
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

    let total_pages = document.pages().len() as usize;
    let page_selection: Option<PageSelection> = query.pages.map(|p| p.parse()).transpose()?;
    if let Some(ref ps) = page_selection {
        ps.validate(total_pages)?;
    }

    if let Some(scale) = query.scale
        && !(0.1..=10.0).contains(&scale)
    {
        return Err(AppError::InvalidScale(
            "scale must be between 0.1 and 10.0".to_string(),
        ));
    }

    let render_config = match query.scale {
        Some(scale) => PdfRenderConfig::new().scale_page_by_factor(scale),
        None => PdfRenderConfig::new(),
    };

    let id = blake3::hash(bytes).to_hex().to_string();
    let ext = query.format.extension();
    let image_format = query.format.as_image_format();

    let images = document
        .pages()
        .iter()
        .enumerate()
        .filter(|(idx, _)| {
            page_selection
                .as_ref()
                .map(|ps| ps.contains(*idx))
                .unwrap_or(true)
        })
        .flat_map(|(idx, page)| {
            let mut output = Cursor::new(Vec::new());

            page.render_with_config(&render_config)
                .ok()?
                .as_image()
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
        .route("/health", get(health_check))
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
    let images = tokio::task::spawn_blocking(move || process_pdf(&data, query)).await??;

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

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
}

async fn health_check() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
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
    #[error("invalid page range: {0}")]
    InvalidPageRange(String),
    #[error("invalid scale: {0}")]
    InvalidScale(String),
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
                "Invalid or missing token".to_owned(),
            ),
            AppError::InvalidPageRange(ref msg) => (
                StatusCode::BAD_REQUEST,
                format!("Invalid page range: {}", msg),
            ),
            AppError::InvalidScale(ref msg) => {
                (StatusCode::BAD_REQUEST, format!("Invalid scale: {}", msg))
            }
        };

        (status, Json(ErrorResponse { message })).into_response()
    }
}
