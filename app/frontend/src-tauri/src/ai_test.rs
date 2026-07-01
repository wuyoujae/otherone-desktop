pub use otherone_backend_core::ai_test::{TestAiModelRequest, TestAiModelResponse};

#[tauri::command]
pub async fn test_ai_model(request: TestAiModelRequest) -> Result<TestAiModelResponse, String> {
    otherone_backend_core::ai_test::test_ai_model(request)
        .await
        .map_err(|error| error.to_string())
}
