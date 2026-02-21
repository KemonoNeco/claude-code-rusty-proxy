//! Models listing endpoint.

use axum::Json;

use crate::adapter::model_map::available_models;
use crate::types::openai::{Model, ModelList};

/// GET /v1/models
pub async fn list_models() -> Json<ModelList> {
    let models: Vec<Model> = available_models()
        .into_iter()
        .map(|m| Model {
            id: m.display_name.to_string(),
            object: "model".to_string(),
            created: 1700000000,
            owned_by: "anthropic".to_string(),
        })
        .collect();

    Json(ModelList {
        object: "list".to_string(),
        data: models,
    })
}
