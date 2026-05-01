#![forbid(unsafe_code)]

use serde::{de::DeserializeOwned, Serialize};
use uuid::Uuid;
use wasm_bindgen::{prelude::wasm_bindgen, JsError, JsValue};

use hera_sdk_core::{HeraClient, HeraClientConfig, PaginationParams};
use hera_types::api::{CreateCaseRequest, CreateWorkspaceRequest, ImportViewingKeyRequest};

#[wasm_bindgen(js_name = HeraClient)]
pub struct HeraWasmClient {
    inner: HeraClient,
}

#[wasm_bindgen(js_class = HeraClient)]
impl HeraWasmClient {
    #[wasm_bindgen(constructor)]
    pub fn new(base_url: String, api_key: String) -> Self {
        Self {
            inner: HeraClient::new(HeraClientConfig { base_url, api_key }),
        }
    }

    #[wasm_bindgen(js_name = listWorkspaces)]
    pub async fn list_workspaces(
        &self,
        pagination_json: Option<String>,
    ) -> Result<String, JsValue> {
        let response = self
            .inner
            .list_workspaces(deserialize_pagination(pagination_json.as_deref()).map_err(js_error)?)
            .await
            .map_err(js_error)?;
        serialize_json(&response).map_err(js_error)
    }

    #[wasm_bindgen(js_name = createWorkspace)]
    pub async fn create_workspace(&self, request_json: String) -> Result<String, JsValue> {
        let request: CreateWorkspaceRequest = deserialize_json(&request_json).map_err(js_error)?;
        let response = self
            .inner
            .create_workspace(&request)
            .await
            .map_err(js_error)?;
        serialize_json(&response).map_err(js_error)
    }

    #[wasm_bindgen(js_name = createCase)]
    pub async fn create_case(&self, request_json: String) -> Result<String, JsValue> {
        let request: CreateCaseRequest = deserialize_json(&request_json).map_err(js_error)?;
        let response = self.inner.create_case(&request).await.map_err(js_error)?;
        serialize_json(&response).map_err(js_error)
    }

    #[wasm_bindgen(js_name = getCase)]
    pub async fn get_case(&self, case_id: String) -> Result<String, JsValue> {
        let response = self
            .inner
            .get_case(parse_uuid(&case_id).map_err(js_error)?)
            .await
            .map_err(js_error)?;
        serialize_json(&response).map_err(js_error)
    }

    #[wasm_bindgen(js_name = listCasesForWorkspace)]
    pub async fn list_cases_for_workspace(
        &self,
        workspace_id: String,
        pagination_json: Option<String>,
    ) -> Result<String, JsValue> {
        let response = self
            .inner
            .list_cases_for_workspace(
                parse_uuid(&workspace_id).map_err(js_error)?,
                deserialize_pagination(pagination_json.as_deref()).map_err(js_error)?,
            )
            .await
            .map_err(js_error)?;
        serialize_json(&response).map_err(js_error)
    }

    #[wasm_bindgen(js_name = importZcashViewKey)]
    pub async fn import_zcash_view_key(
        &self,
        case_id: String,
        request_json: String,
    ) -> Result<String, JsValue> {
        let request: ImportViewingKeyRequest = deserialize_json(&request_json).map_err(js_error)?;
        let response = self
            .inner
            .import_zcash_view_key(parse_uuid(&case_id).map_err(js_error)?, &request)
            .await
            .map_err(js_error)?;
        serialize_json(&response).map_err(js_error)
    }

    #[wasm_bindgen(js_name = importNamadaViewKey)]
    pub async fn import_namada_view_key(
        &self,
        case_id: String,
        request_json: String,
    ) -> Result<String, JsValue> {
        let request: ImportViewingKeyRequest = deserialize_json(&request_json).map_err(js_error)?;
        let response = self
            .inner
            .import_namada_view_key(parse_uuid(&case_id).map_err(js_error)?, &request)
            .await
            .map_err(js_error)?;
        serialize_json(&response).map_err(js_error)
    }

    #[wasm_bindgen(js_name = scanCase)]
    pub async fn scan_case(&self, case_id: String) -> Result<String, JsValue> {
        let response = self
            .inner
            .scan_case(parse_uuid(&case_id).map_err(js_error)?)
            .await
            .map_err(js_error)?;
        serialize_json(&response).map_err(js_error)
    }

    #[wasm_bindgen(js_name = getCaseStatus)]
    pub async fn get_case_status(&self, case_id: String) -> Result<String, JsValue> {
        let response = self
            .inner
            .get_case_status(parse_uuid(&case_id).map_err(js_error)?)
            .await
            .map_err(js_error)?;
        serialize_json(&response).map_err(js_error)
    }

    #[wasm_bindgen(js_name = getCaseEvents)]
    pub async fn get_case_events(&self, case_id: String) -> Result<String, JsValue> {
        let response = self
            .inner
            .get_case_events(parse_uuid(&case_id).map_err(js_error)?)
            .await
            .map_err(js_error)?;
        serialize_json(&response).map_err(js_error)
    }
}

fn parse_uuid(value: &str) -> Result<Uuid, String> {
    Uuid::parse_str(value).map_err(|err| format!("invalid uuid `{value}`: {err}"))
}

fn deserialize_json<T>(value: &str) -> Result<T, String>
where
    T: DeserializeOwned,
{
    serde_json::from_str(value).map_err(|err| err.to_string())
}

fn deserialize_pagination(value: Option<&str>) -> Result<PaginationParams, String> {
    match value {
        None => Ok(PaginationParams::default()),
        Some(raw) if raw.trim().is_empty() => Ok(PaginationParams::default()),
        Some(raw) => deserialize_json(raw),
    }
}

fn serialize_json<T>(value: &T) -> Result<String, String>
where
    T: Serialize,
{
    serde_json::to_string(value).map_err(|err| err.to_string())
}

fn js_error(error: impl ToString) -> JsValue {
    JsError::new(&error.to_string()).into()
}

#[cfg(test)]
mod tests {
    use super::{deserialize_pagination, parse_uuid};

    #[test]
    fn parse_uuid_rejects_invalid_input() {
        assert!(parse_uuid("not-a-uuid").is_err());
    }

    #[test]
    fn missing_pagination_defaults_cleanly() {
        let pagination = deserialize_pagination(None).expect("pagination should default");
        assert_eq!(pagination.limit, None);
        assert_eq!(pagination.offset, None);
    }
}
