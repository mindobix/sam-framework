use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::error::{Result, SamError};

// ---------------------------------------------------------------------------
// Request / Response types — matched to actual MonoGraph API
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolveRequest {
    pub domains: Vec<String>,
    pub auto_include: Vec<String>,
    pub ai_infer: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cochange_commits: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cochange_min_score: Option<f64>,
}

// MonoGraph /resolve returns: {"resolved": [...], "inferred": [...], "inference_detail": [...]}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolveApiResponse {
    pub resolved: Vec<String>,
    #[serde(default)]
    pub inferred: Vec<String>,
    #[serde(default)]
    pub inference_detail: Vec<InferenceDetail>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceDetail {
    pub domain: String,
    pub reason: String,
    #[serde(default)]
    pub from: Option<String>,
}

// Our internal resolved domain type (used by CLI)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedDomain {
    pub path: String,
    pub reason: String,
    #[serde(default)]
    pub score: Option<f64>,
    #[serde(default)]
    pub file_count: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolveResponse {
    pub domains: Vec<ResolvedDomain>,
}

// MonoGraph /impact returns: {"affected": [{"domain", "risk", "type", "calls_per_day"}]}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImpactEntry {
    pub domain: String,
    pub risk: String,
    #[serde(default, rename = "type")]
    pub impact_type: Option<String>,
    #[serde(default)]
    pub calls_per_day: Option<u64>,
    #[serde(default)]
    pub teams: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImpactResponse {
    pub entries: Vec<ImpactEntry>,
}

// MonoGraph /impact raw API shape
#[derive(Debug, Clone, Deserialize)]
struct ImpactApiResponse {
    affected: Vec<ImpactEntry>,
}

// MonoGraph /graph returns: {"domain", "edges": [{"to", "type", "weight", "commit_count"}]}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    pub to: String,
    #[serde(default, rename = "type")]
    pub edge_type: Option<String>,
    #[serde(default)]
    pub weight: Option<f64>,
    #[serde(default)]
    pub commit_count: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphResponse {
    pub domain: String,
    pub edges: Vec<GraphEdge>,
}

// Legacy tree types for CLI rendering
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    pub path: String,
    pub node_type: String,
    pub children: Vec<GraphNode>,
    #[serde(default)]
    pub score: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CochangeEntry {
    pub file: String,
    pub score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CochangeResponse {
    pub entries: Vec<CochangeEntry>,
}

// ---------------------------------------------------------------------------
// Client
// ---------------------------------------------------------------------------

pub struct Client {
    base_url: String,
    client: reqwest::blocking::Client,
}

impl Client {
    pub fn new(address: &str, timeout: Duration) -> Self {
        let base_url = if address.starts_with("http") {
            address.to_string()
        } else {
            format!("http://{address}")
        };
        let client = reqwest::blocking::Client::builder()
            .timeout(timeout)
            .build()
            .unwrap_or_else(|_| reqwest::blocking::Client::new());
        Self { base_url, client }
    }

    pub fn health(&self) -> bool {
        self.client
            .get(format!("{}/health", self.base_url))
            .send()
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }

    pub fn resolve(&self, req: &ResolveRequest) -> Result<ResolveResponse> {
        let resp = self
            .client
            .post(format!("{}/resolve", self.base_url))
            .json(req)
            .send()
            .map_err(|e| SamError::MonoGraphError(format!("resolve request failed: {e}")))?;
        let api: ResolveApiResponse = resp
            .json()
            .map_err(|e| SamError::MonoGraphError(format!("resolve parse failed: {e}")))?;

        // Convert API response to our internal format
        let detail_map: std::collections::HashMap<String, String> = api
            .inference_detail
            .iter()
            .map(|d| (d.domain.clone(), d.reason.clone()))
            .collect();

        let domains = api
            .resolved
            .iter()
            .map(|path| {
                let reason = if req.domains.contains(path) {
                    "profile".to_string()
                } else if req.auto_include.contains(path) {
                    "auto_include".to_string()
                } else {
                    detail_map
                        .get(path)
                        .cloned()
                        .unwrap_or_else(|| "ai_inferred".to_string())
                };
                ResolvedDomain {
                    path: path.clone(),
                    reason,
                    score: None,
                    file_count: None,
                }
            })
            .collect();

        Ok(ResolveResponse { domains })
    }

    pub fn impact(&self, files: &[String]) -> Result<ImpactResponse> {
        let resp = self
            .client
            .post(format!("{}/impact", self.base_url))
            .json(&serde_json::json!({ "changed_files": files }))
            .send()
            .map_err(|e| SamError::MonoGraphError(format!("impact request failed: {e}")))?;
        let api: ImpactApiResponse = resp
            .json()
            .map_err(|e| SamError::MonoGraphError(format!("impact parse failed: {e}")))?;
        Ok(ImpactResponse {
            entries: api.affected,
        })
    }

    pub fn graph(&self, domain: &str) -> Result<GraphResponse> {
        let resp = self
            .client
            .get(format!("{}/graph", self.base_url))
            .query(&[("domain", domain)])
            .send()
            .map_err(|e| SamError::MonoGraphError(format!("graph request failed: {e}")))?;
        let body = resp
            .json::<GraphResponse>()
            .map_err(|e| SamError::MonoGraphError(format!("graph parse failed: {e}")))?;
        Ok(body)
    }

    pub fn cochange(&self, file: &str) -> Result<CochangeResponse> {
        let resp = self
            .client
            .get(format!("{}/cochange", self.base_url))
            .query(&[("file", file)])
            .send()
            .map_err(|e| SamError::MonoGraphError(format!("cochange request failed: {e}")))?;
        let body = resp
            .json::<CochangeResponse>()
            .map_err(|e| SamError::MonoGraphError(format!("cochange parse failed: {e}")))?;
        Ok(body)
    }

    pub fn analyze(&self, repo_path: &str) -> Result<()> {
        let resp = self
            .client
            .post(format!("{}/analyze", self.base_url))
            .json(&serde_json::json!({ "repo_path": repo_path }))
            .send()
            .map_err(|e| SamError::MonoGraphError(format!("analyze request failed: {e}")))?;
        if !resp.status().is_success() {
            return Err(SamError::MonoGraphError(format!(
                "analyze returned {}",
                resp.status()
            )));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Fallback resolve (offline, no AI)
// ---------------------------------------------------------------------------

pub fn fallback_resolve(domains: &[String], auto_include: &[String]) -> ResolveResponse {
    let mut resolved = Vec::new();
    for d in domains {
        resolved.push(ResolvedDomain {
            path: d.clone(),
            reason: "profile".to_string(),
            score: None,
            file_count: None,
        });
    }
    for d in auto_include {
        if !domains.contains(d) {
            resolved.push(ResolvedDomain {
                path: d.clone(),
                reason: "auto_include".to_string(),
                score: None,
                file_count: None,
            });
        }
    }
    ResolveResponse { domains: resolved }
}
