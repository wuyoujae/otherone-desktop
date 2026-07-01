use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, patch, post, put};
use axum::{Json, Router};
use otherone_backend_core::ai_test::{
    self, TestAiModelError, TestAiModelErrorKind, TestAiModelRequest, TestAiModelResponse,
};
use otherone_backend_core::artifacts::{self, FileArtifact};
use otherone_backend_core::chat::{
    ChatError, ChatErrorKind, ChatRuntime, ChatStreamEvent, EnqueueChatMessageRequest,
    SendChatMessageRequest, SendChatMessageResponse,
};
use otherone_backend_core::memory::{self, MemoryTreeResponse};
use otherone_backend_core::plugins::{
    self, ImportMcpServersFromUrlRequest, ImportMcpServersRequest, ImportSkillFromUrlRequest,
    PluginEntry, PluginInstallRequest,
};
use otherone_backend_core::session::{self, AppSessionDetail, AppSessionSummary};
use otherone_backend_core::settings::{self, RuntimePaths, SaveEngineSettingsRequest};
use otherone_backend_core::storage::{self, ProviderConfig};
use otherone_backend_core::workflow::{
    self, CreateWorkflowTaskRequest, DeleteWorkflowTaskRequest, ListWorkflowTasksForRangeRequest,
    ModifyWorkflowTaskRequest, UpdateWorkflowTaskStatusRequest, WorkflowTask,
};
use serde::Deserialize;
use std::convert::Infallible;
use std::env;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::{Stream, StreamExt};
use tower_http::cors::CorsLayer;

#[derive(Clone)]
struct AppState {
    paths: RuntimePaths,
    artifact_events: broadcast::Sender<FileArtifact>,
    chat_events: broadcast::Sender<ChatStreamEvent>,
    chat_runtime: Arc<ChatRuntime>,
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn bad_request(message: String) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message,
        }
    }

    fn internal(message: String) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message,
        }
    }

    fn bad_gateway(message: String) -> Self {
        Self {
            status: StatusCode::BAD_GATEWAY,
            message,
        }
    }

    fn not_found(message: String) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message,
        }
    }

    fn from_ai_model_test(error: TestAiModelError) -> Self {
        match error.kind() {
            TestAiModelErrorKind::Validation => Self::bad_request(error.to_string()),
            TestAiModelErrorKind::Timeout | TestAiModelErrorKind::Upstream => {
                Self::bad_gateway(error.to_string())
            }
        }
    }

    fn from_chat(error: ChatError) -> Self {
        match error.kind() {
            ChatErrorKind::Validation => Self::bad_request(error.to_string()),
            ChatErrorKind::Conflict => Self {
                status: StatusCode::CONFLICT,
                message: error.to_string(),
            },
            ChatErrorKind::Upstream => Self::bad_gateway(error.to_string()),
            ChatErrorKind::Internal => Self::internal(error.to_string()),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = Json(serde_json::json!({
            "message": self.message,
        }));
        (self.status, body).into_response()
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SaveApiConfigsRequest {
    providers: Vec<ProviderConfig>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateSessionTitleBody {
    title: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListWorkflowTasksQuery {
    start_date: Option<String>,
    end_date: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateWorkflowTaskStatusBody {
    status: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModifyWorkflowTaskBody {
    prompt: String,
    model_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CancelChatMessageBody {
    session_id: String,
}

#[tokio::main]
async fn main() -> Result<(), String> {
    let (artifact_events, _) = broadcast::channel(128);
    let (chat_events, _) = broadcast::channel(512);
    let artifact_events_for_runtime = artifact_events.clone();
    let chat_events_for_runtime = chat_events.clone();
    let state = AppState {
        paths: web_runtime_paths()?,
        artifact_events,
        chat_events,
        chat_runtime: Arc::new(ChatRuntime::with_artifact_sink(
            move |event| {
                let _ = chat_events_for_runtime.send(event);
            },
            move |artifact| {
                let _ = artifact_events_for_runtime.send(artifact);
            },
        )),
    };
    let bind_addr = web_bind_addr()?;
    let app = build_router(state);
    let listener = tokio::net::TcpListener::bind(bind_addr)
        .await
        .map_err(|error| error.to_string())?;

    println!("otherone_web listening on http://{bind_addr}");

    axum::serve(listener, app)
        .await
        .map_err(|error| error.to_string())
}

fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/api/app-settings", get(load_app_settings))
        .route("/api/app-settings/engine", put(save_engine_settings))
        .route("/api/ai-model-test", post(test_ai_model))
        .route("/api/plugins", get(load_plugins))
        .route("/api/plugins/install", post(install_plugin_entry))
        .route("/api/plugins/uninstall", post(uninstall_plugin_entry))
        .route(
            "/api/plugins/skills/import-url",
            post(import_skill_from_url),
        )
        .route("/api/plugins/mcp/import", post(import_mcp_servers))
        .route(
            "/api/plugins/mcp/import-url",
            post(import_mcp_servers_from_url),
        )
        .route("/api/chat/messages", post(send_chat_message))
        .route("/api/chat/messages/enqueue", post(enqueue_chat_message))
        .route("/api/chat/messages/cancel", post(cancel_chat_message))
        .route("/api/chat/stream", get(stream_chat_events))
        .route(
            "/api/api-configs",
            get(load_api_configs).put(save_api_configs),
        )
        .route("/api/sessions", get(load_sessions))
        .route("/api/sessions/{session_id}", get(read_session))
        .route(
            "/api/sessions/{session_id}/artifacts",
            get(list_file_artifacts),
        )
        .route(
            "/api/sessions/{session_id}/title",
            patch(update_session_title),
        )
        .route("/api/artifacts/stream", get(stream_file_artifacts))
        .route(
            "/api/artifacts/{artifact_id}/download",
            get(download_file_artifact),
        )
        .route("/api/memory/tree", get(read_memory_tree))
        .route(
            "/api/workflow/tasks",
            get(list_workflow_tasks).post(create_workflow_task),
        )
        .route(
            "/api/workflow/tasks/{task_id}",
            delete(delete_workflow_task).patch(update_workflow_task),
        )
        .route(
            "/api/workflow/tasks/{task_id}/status",
            patch(update_workflow_task_status),
        )
        .layer(CorsLayer::permissive())
        .with_state(state)
}

async fn health() -> &'static str {
    "ok"
}

async fn load_app_settings(
    State(state): State<AppState>,
) -> Result<Json<settings::AppSettings>, ApiError> {
    settings::load_settings(&state.paths)
        .map(Json)
        .map_err(ApiError::internal)
}

async fn save_engine_settings(
    State(state): State<AppState>,
    Json(request): Json<SaveEngineSettingsRequest>,
) -> Result<Json<settings::AppSettings>, ApiError> {
    settings::save_engine_settings(&state.paths, request)
        .map(Json)
        .map_err(ApiError::internal)
}

async fn load_api_configs(
    State(state): State<AppState>,
) -> Result<Json<Vec<ProviderConfig>>, ApiError> {
    let data_root = settings::data_root(&state.paths).map_err(ApiError::internal)?;
    storage::load_api_configs(&data_root)
        .map(Json)
        .map_err(ApiError::internal)
}

async fn save_api_configs(
    State(state): State<AppState>,
    Json(request): Json<SaveApiConfigsRequest>,
) -> Result<StatusCode, ApiError> {
    let data_root = settings::data_root(&state.paths).map_err(ApiError::internal)?;
    storage::save_api_configs(&data_root, request.providers)
        .map(|_| StatusCode::NO_CONTENT)
        .map_err(ApiError::internal)
}

async fn test_ai_model(
    Json(request): Json<TestAiModelRequest>,
) -> Result<Json<TestAiModelResponse>, ApiError> {
    ai_test::test_ai_model(request)
        .await
        .map(Json)
        .map_err(ApiError::from_ai_model_test)
}

async fn load_plugins(State(state): State<AppState>) -> Result<Json<Vec<PluginEntry>>, ApiError> {
    let data_root = settings::data_root(&state.paths).map_err(ApiError::internal)?;
    plugins::load_plugin_list(&data_root)
        .map(Json)
        .map_err(ApiError::internal)
}

async fn install_plugin_entry(
    State(state): State<AppState>,
    Json(request): Json<PluginInstallRequest>,
) -> Result<StatusCode, ApiError> {
    let data_root = settings::data_root(&state.paths).map_err(ApiError::internal)?;
    plugins::install_plugin(&data_root, request)
        .map(|_| StatusCode::NO_CONTENT)
        .map_err(ApiError::bad_request)
}

async fn uninstall_plugin_entry(
    State(state): State<AppState>,
    Json(request): Json<PluginInstallRequest>,
) -> Result<StatusCode, ApiError> {
    let data_root = settings::data_root(&state.paths).map_err(ApiError::internal)?;
    plugins::uninstall_plugin(&data_root, request)
        .map(|_| StatusCode::NO_CONTENT)
        .map_err(ApiError::bad_request)
}

async fn import_skill_from_url(
    State(state): State<AppState>,
    Json(request): Json<ImportSkillFromUrlRequest>,
) -> Result<Json<PluginEntry>, ApiError> {
    let data_root = settings::data_root(&state.paths).map_err(ApiError::internal)?;
    plugins::import_skill_from_url(&data_root, request)
        .map(Json)
        .map_err(ApiError::bad_request)
}

async fn import_mcp_servers(
    State(state): State<AppState>,
    Json(request): Json<ImportMcpServersRequest>,
) -> Result<Json<Vec<PluginEntry>>, ApiError> {
    let data_root = settings::data_root(&state.paths).map_err(ApiError::internal)?;
    plugins::import_mcp_servers(&data_root, request)
        .map(Json)
        .map_err(ApiError::bad_request)
}

async fn import_mcp_servers_from_url(
    State(state): State<AppState>,
    Json(request): Json<ImportMcpServersFromUrlRequest>,
) -> Result<Json<Vec<PluginEntry>>, ApiError> {
    let data_root = settings::data_root(&state.paths).map_err(ApiError::internal)?;
    plugins::import_mcp_servers_from_url(&data_root, request)
        .map(Json)
        .map_err(ApiError::bad_request)
}

async fn send_chat_message(
    State(state): State<AppState>,
    Json(request): Json<SendChatMessageRequest>,
) -> Result<Json<SendChatMessageResponse>, ApiError> {
    let data_root = settings::data_root(&state.paths).map_err(ApiError::internal)?;
    let dialogue_root = settings::dialogue_root(&state.paths).map_err(ApiError::internal)?;
    let settings = settings::load_settings(&state.paths).map_err(ApiError::internal)?;
    let providers = storage::load_api_configs(&data_root).map_err(ApiError::internal)?;
    state
        .chat_runtime
        .send_chat_message(request, settings, providers, data_root, dialogue_root)
        .map(Json)
        .map_err(ApiError::from_chat)
}

async fn enqueue_chat_message(
    State(state): State<AppState>,
    Json(request): Json<EnqueueChatMessageRequest>,
) -> Result<StatusCode, ApiError> {
    state
        .chat_runtime
        .enqueue_chat_message(request)
        .await
        .map(|_| StatusCode::NO_CONTENT)
        .map_err(ApiError::from_chat)
}

async fn cancel_chat_message(
    State(state): State<AppState>,
    Json(body): Json<CancelChatMessageBody>,
) -> Result<StatusCode, ApiError> {
    state
        .chat_runtime
        .cancel_chat_message(body.session_id)
        .map(|_| StatusCode::NO_CONTENT)
        .map_err(ApiError::from_chat)
}

async fn stream_chat_events(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = BroadcastStream::new(state.chat_events.subscribe()).filter_map(|result| {
        result
            .ok()
            .and_then(|event| serde_json::to_string(&event).ok())
            .map(|json| Ok(Event::default().data(json)))
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}

async fn load_sessions(
    State(state): State<AppState>,
) -> Result<Json<Vec<AppSessionSummary>>, ApiError> {
    let data_root = settings::data_root(&state.paths).map_err(ApiError::internal)?;
    let dialogue_root = settings::dialogue_root(&state.paths).map_err(ApiError::internal)?;
    session::load_sessions(&data_root, &dialogue_root)
        .map(Json)
        .map_err(ApiError::internal)
}

async fn read_session(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<AppSessionDetail>, ApiError> {
    let data_root = settings::data_root(&state.paths).map_err(ApiError::internal)?;
    let dialogue_root = settings::dialogue_root(&state.paths).map_err(ApiError::internal)?;
    session::read_session(&data_root, &dialogue_root, &session_id)
        .map(Json)
        .map_err(ApiError::internal)
}

async fn update_session_title(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Json(body): Json<UpdateSessionTitleBody>,
) -> Result<StatusCode, ApiError> {
    let data_root = settings::data_root(&state.paths).map_err(ApiError::internal)?;
    session::update_session_title(
        &data_root,
        session::UpdateSessionTitleRequest {
            session_id,
            title: body.title,
        },
    )
    .map(|_| StatusCode::NO_CONTENT)
    .map_err(ApiError::internal)
}

async fn list_file_artifacts(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<Vec<FileArtifact>>, ApiError> {
    let data_root = settings::data_root(&state.paths).map_err(ApiError::internal)?;
    artifacts::list_file_artifacts(&data_root, &session_id)
        .map(Json)
        .map_err(ApiError::internal)
}

async fn stream_file_artifacts(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = BroadcastStream::new(state.artifact_events.subscribe()).filter_map(|result| {
        result
            .ok()
            .and_then(|artifact| serde_json::to_string(&artifact).ok())
            .map(|json| Ok(Event::default().data(json)))
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}

async fn download_file_artifact(
    State(state): State<AppState>,
    Path(artifact_id): Path<String>,
) -> Result<Response, ApiError> {
    let data_root = settings::data_root(&state.paths).map_err(ApiError::internal)?;
    let artifact =
        artifacts::read_file_artifact(&data_root, &artifact_id).map_err(ApiError::bad_request)?;
    let file_path = PathBuf::from(&artifact.file_path);
    let bytes = tokio::fs::read(&file_path)
        .await
        .map_err(|_| ApiError::not_found("Artifact file does not exist.".to_string()))?;
    let content_disposition = format!(
        "attachment; filename=\"{}\"",
        sanitize_download_filename(&artifact.name)
    );
    let mut response = Response::new(Body::from(bytes));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    response.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&content_disposition)
            .unwrap_or_else(|_| HeaderValue::from_static("attachment")),
    );
    Ok(response)
}

async fn read_memory_tree(
    State(state): State<AppState>,
) -> Result<Json<MemoryTreeResponse>, ApiError> {
    let dialogue_root = settings::dialogue_root(&state.paths).map_err(ApiError::internal)?;
    memory::read_memory_tree(&dialogue_root)
        .map(Json)
        .map_err(ApiError::internal)
}

async fn list_workflow_tasks(
    State(state): State<AppState>,
    Query(query): Query<ListWorkflowTasksQuery>,
) -> Result<Json<Vec<WorkflowTask>>, ApiError> {
    let data_root = settings::data_root(&state.paths).map_err(ApiError::internal)?;

    match (query.start_date, query.end_date) {
        (Some(start_date), Some(end_date)) => workflow::list_workflow_tasks_for_range(
            &data_root,
            ListWorkflowTasksForRangeRequest {
                start_date,
                end_date,
            },
        )
        .map(Json)
        .map_err(ApiError::internal),
        (None, None) => workflow::list_workflow_tasks(&data_root)
            .map(Json)
            .map_err(ApiError::internal),
        _ => Err(ApiError::bad_request(
            "startDate and endDate must be provided together.".to_string(),
        )),
    }
}

async fn create_workflow_task(
    State(state): State<AppState>,
    Json(request): Json<CreateWorkflowTaskRequest>,
) -> Result<Json<WorkflowTask>, ApiError> {
    let data_root = settings::data_root(&state.paths).map_err(ApiError::internal)?;
    let providers = storage::load_api_configs(&data_root).map_err(ApiError::internal)?;
    workflow::create_workflow_task(&data_root, request, &providers)
        .await
        .map(Json)
        .map_err(ApiError::bad_request)
}

async fn update_workflow_task(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
    Json(body): Json<ModifyWorkflowTaskBody>,
) -> Result<Json<Vec<WorkflowTask>>, ApiError> {
    let data_root = settings::data_root(&state.paths).map_err(ApiError::internal)?;
    let providers = storage::load_api_configs(&data_root).map_err(ApiError::internal)?;
    workflow::update_workflow_task(
        &data_root,
        ModifyWorkflowTaskRequest {
            id: task_id,
            prompt: body.prompt,
            model_id: body.model_id,
        },
        &providers,
    )
    .await
    .map(Json)
    .map_err(ApiError::bad_request)
}

async fn update_workflow_task_status(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
    Json(body): Json<UpdateWorkflowTaskStatusBody>,
) -> Result<Json<WorkflowTask>, ApiError> {
    let data_root = settings::data_root(&state.paths).map_err(ApiError::internal)?;
    workflow::update_workflow_task_status(
        &data_root,
        UpdateWorkflowTaskStatusRequest {
            id: task_id,
            status: body.status,
        },
    )
    .map(Json)
    .map_err(ApiError::internal)
}

async fn delete_workflow_task(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let data_root = settings::data_root(&state.paths).map_err(ApiError::internal)?;
    workflow::delete_workflow_task(&data_root, DeleteWorkflowTaskRequest { id: task_id })
        .map(|_| StatusCode::NO_CONTENT)
        .map_err(ApiError::internal)
}

fn web_runtime_paths() -> Result<RuntimePaths, String> {
    let data_root = env::var_os("OTHERONE_WEB_DATA_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(".otherone-web")
        });
    Ok(RuntimePaths::new(
        data_root.join("settings.json"),
        data_root,
    ))
}

fn web_bind_addr() -> Result<SocketAddr, String> {
    env::var("OTHERONE_WEB_BIND")
        .unwrap_or_else(|_| "127.0.0.1:17820".to_string())
        .parse::<SocketAddr>()
        .map_err(|error| error.to_string())
}

fn sanitize_download_filename(name: &str) -> String {
    let sanitized = name
        .chars()
        .map(|ch| match ch {
            '"' | '\\' | '/' | ':' | '*' | '?' | '<' | '>' | '|' | '\r' | '\n' => '_',
            _ => ch,
        })
        .collect::<String>();
    let sanitized = sanitized.trim();

    if sanitized.is_empty() {
        "artifact".to_string()
    } else {
        sanitized.to_string()
    }
}
