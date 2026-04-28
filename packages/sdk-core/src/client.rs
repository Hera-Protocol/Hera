use reqwest::{Method, RequestBuilder};
use serde::{de::DeserializeOwned, Serialize};
use uuid::Uuid;

use hera_types::{
    api::{
        CaseDetailResponse, CaseSummaryResponse, CreateCaseRequest, CreateCaseResponse,
        CreateWorkspaceRequest, CreateWorkspaceResponse, GetCaseStatusResponse, PaginatedResponse,
        ScanCaseResponse, WorkspaceSummaryResponse,
    },
    CanonicalEvent,
};

use crate::error::{HeraSdkError, Result};

#[derive(Debug, Clone)]
pub struct HeraClientConfig {
    pub base_url: String,
    pub api_key: String,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct PaginationParams {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct HeraClient {
    base_url: String,
    api_key: String,
    client: reqwest::Client,
}

impl HeraClient {
    pub fn new(config: HeraClientConfig) -> Self {
        Self {
            base_url: config.base_url.trim_end_matches('/').to_string(),
            api_key: config.api_key,
            client: reqwest::Client::new(),
        }
    }

    pub fn with_http_client(config: HeraClientConfig, client: reqwest::Client) -> Self {
        Self {
            base_url: config.base_url.trim_end_matches('/').to_string(),
            api_key: config.api_key,
            client,
        }
    }

    pub async fn list_workspaces(
        &self,
        pagination: PaginationParams,
    ) -> Result<PaginatedResponse<WorkspaceSummaryResponse>> {
        self.get_json("v1/workspaces", pagination).await
    }

    pub async fn create_workspace(
        &self,
        request: &CreateWorkspaceRequest,
    ) -> Result<CreateWorkspaceResponse> {
        self.post_json("v1/workspaces", request).await
    }

    pub async fn create_case(&self, request: &CreateCaseRequest) -> Result<CreateCaseResponse> {
        self.post_json("v1/cases", request).await
    }

    pub async fn get_case(&self, case_id: Uuid) -> Result<CaseDetailResponse> {
        self.get_json(&format!("v1/cases/{case_id}"), PaginationParams::default())
            .await
    }

    pub async fn list_cases_for_workspace(
        &self,
        workspace_id: Uuid,
        pagination: PaginationParams,
    ) -> Result<PaginatedResponse<CaseSummaryResponse>> {
        self.get_json(&format!("v1/workspaces/{workspace_id}/cases"), pagination)
            .await
    }

    pub async fn scan_case(&self, case_id: Uuid) -> Result<ScanCaseResponse> {
        self.post_json::<(), _>(&format!("v1/cases/{case_id}/scan"), &())
            .await
    }

    pub async fn get_case_status(&self, case_id: Uuid) -> Result<GetCaseStatusResponse> {
        self.get_json(
            &format!("v1/cases/{case_id}/status"),
            PaginationParams::default(),
        )
        .await
    }

    pub async fn get_case_events(&self, case_id: Uuid) -> Result<Vec<CanonicalEvent>> {
        self.get_json(
            &format!("v1/cases/{case_id}/events"),
            PaginationParams::default(),
        )
        .await
    }

    async fn get_json<T>(&self, path: &str, pagination: PaginationParams) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let mut request = self.request(Method::GET, path);
        if let Some(limit) = pagination.limit {
            request = request.query(&[("limit", limit)]);
        }
        if let Some(offset) = pagination.offset {
            request = request.query(&[("offset", offset)]);
        }

        self.execute_json(request).await
    }

    async fn post_json<B, T>(&self, path: &str, body: &B) -> Result<T>
    where
        B: Serialize + ?Sized,
        T: DeserializeOwned,
    {
        self.execute_json(self.request(Method::POST, path).json(body))
            .await
    }

    async fn execute_json<T>(&self, request: RequestBuilder) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let response = request.send().await?;
        let status = response.status();
        if !status.is_success() {
            return Err(HeraSdkError::Api {
                status,
                body: response.text().await.unwrap_or_default(),
            });
        }

        Ok(response.json().await?)
    }

    fn request(&self, method: Method, path: &str) -> RequestBuilder {
        self.client
            .request(method, self.endpoint(path))
            .bearer_auth(&self.api_key)
    }

    fn endpoint(&self, path: &str) -> String {
        format!("{}/{}", self.base_url, path.trim_start_matches('/'))
    }
}

#[cfg(test)]
mod tests {
    use super::{HeraClient, HeraClientConfig};

    #[test]
    fn normalizes_endpoint_slashes() {
        let client = HeraClient::new(HeraClientConfig {
            base_url: "https://api.hera.test/".to_string(),
            api_key: "test-key".to_string(),
        });

        assert_eq!(
            client.endpoint("/v1/workspaces"),
            "https://api.hera.test/v1/workspaces"
        );
    }
}
