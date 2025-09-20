use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum GitLitError {
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("unauthorized")]
    Unauthorized,
    #[error("auth error: {0}")]
    Auth(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Repository {
    pub _id: String,
    pub user: String,
    pub name: String,
    pub description: String,
    pub is_private: bool,
    pub created_at: String,
    pub updated_at: String,
    pub forked_from: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRepoRequest {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")] pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")] pub is_private: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OkResponse { pub ok: bool }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Branch { pub is_head: bool, pub name: String, pub oid: String, pub upstream: Option<String> }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchesResponse { pub branches: Vec<Branch> }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrancheDeleteResponse { pub message: String }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitInfo { pub hash: String, pub name: String, pub email: String, pub timestamp_secs: i64, pub subject: String }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum ContentResponse {
    #[serde(rename = "tree")]
    tree { entries: Vec<TreeEntry> },
    #[serde(rename = "blob")]
    blob { content_base64: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeEntry { pub mode: String, pub kind: serde_json::Value, pub oid: String, pub path: String, pub size: Option<i64> }

#[derive(Clone)]
pub struct GitLitClient {
    url: String,
    http: reqwest::Client,
    token_store: TokenStore,
}

impl GitLitClient {
    pub fn new(url: impl Into<String>) -> Result<Self, GitLitError> {
        let http = reqwest::Client::builder()
            .user_agent("gitlit")
            .build()?;
        Ok(Self {
            url: url.into().trim_end_matches('/').to_string(),
            http,
            token_store: TokenStore::new()?,
        })
    }

    fn host_key(&self) -> String {
        self.url.clone()
    }

    fn auth(&self, req: reqwest::RequestBuilder) -> std::pin::Pin<Box<dyn std::future::Future<Output=Result<reqwest::RequestBuilder, GitLitError>> + Send + '_>> {
        Box::pin(async move {
            let token = self.get_token().await?;
            Ok(req.bearer_auth(token))
        })
    }

    pub async fn login(&self, login: &str, password: &str) -> Result<String, GitLitError> {
        #[derive(Deserialize)]
        struct LoginResp { token: String }
        let url = format!("{}/api/v1/login", self.url);

        let res = self
            .http
            .post(url)
            .json(&serde_json::json!({
                "login": login,
                "password": password,
            }))
            .send()
            .await?;
        if !res.status().is_success() {
            return Err(GitLitError::Auth(format!("login failed: {}", res.status())));
        }
        let token: String = res.json::<LoginResp>().await?.token;
        self.token_store.save(&self.host_key(), &token)?;
        Ok(token)
    }

    pub async fn register(&self, username: &str, email:&str, password: &str) -> Result<String, GitLitError> {
        let url = format!("{}/api/v1/register", self.url);

        let res = self
            .http
            .post(url)
            .json(&serde_json::json!({
                "username": username,
                "email": email,
                "password": password,
            }))
            .send()
            .await?;
        if res.status() != reqwest::StatusCode::CREATED {
            return Err(GitLitError::Auth(format!("register failed: {}", res.status())));
        }

        Ok(res.text().await?)
    }


    async fn get_token(&self) -> Result<String, GitLitError> {
        if let Some(token) = self.token_store.load(&self.host_key())? {
            if !token.is_empty() {
                return Ok(token);
            }
        }
        Err(GitLitError::Auth("notoken".to_string()))
    }
    pub async fn list_repos(&self, owner: Option<&str>, filter: Option<&str>, q: Option<&str>) -> Result<Vec<Repository>, GitLitError> {
        let url = format!("{}/api/v1/repos", self.url);
        let mut req = self.http.get(url);
        if let Some(owner) = owner { req = req.query(&[("owner", owner)]); }
        if let Some(filter) = filter { req = req.query(&[("filter", filter)]); }
        if let Some(q) = q { req = req.query(&[("q", q)]); }
        let res = req.send().await?;
        if !res.status().is_success() {
            return Err(GitLitError::Auth(format!("list_repos failed: {}", res.status())));
        }
        Ok(res.json::<Vec<Repository>>().await?)
    }


    pub async fn create_repo(&self, name: &str, description: Option<&str>, is_private: Option<bool>) -> Result<Repository, GitLitError> {
        let url = format!("{}/api/v1/create", self.url);
        let req = self.http.post(url).json(&CreateRepoRequest{
            name: name.to_string(),
            description: description.map(|s| s.to_string()),
            is_private,
        });
        let req = self.auth(req).await?;
        let res = req.send().await?;
        if res.status() != reqwest::StatusCode::CREATED { return Err(GitLitError::Auth(format!("create_repo failed: {}", res.status())));}        
        Ok(res.json::<Repository>().await?)
    }


    pub async fn delete_repo(&self, id: &str) -> Result<OkResponse, GitLitError> {
        let url = format!("{}/api/v1/delete", self.url);
        let req = self.http.delete(url).query(&[("id", id)]);
        let req = self.auth(req).await?;
        let res = req.send().await?;
        if !res.status().is_success() { return Err(GitLitError::Auth(format!("delete_repo failed: {}", res.status())));}        
        Ok(res.json::<OkResponse>().await?)
    }


    pub async fn branches(&self, id: &str) -> Result<BranchesResponse, GitLitError> {
        let url = format!("{}/api/v1/branches", self.url);
        let res = self.http.get(url).query(&[("id", id)]).send().await?;
        if !res.status().is_success() { return Err(GitLitError::Auth(format!("branches failed: {}", res.status())));}        
        Ok(res.json::<BranchesResponse>().await?)
    }

    pub async fn delete_branch(&self, id: &str, branch: &str) -> Result<BrancheDeleteResponse, GitLitError> {
        let url = format!("{}/api/v1/branch", self.url);
        let req = self.http
            .delete(url)
            .query(&[("id", id), ("branch", branch)]);
        let req = self.auth(req).await?;
        let res = req.send().await?;
        if !res.status().is_success() {
            return Err(GitLitError::Auth(format!("delete_branch failed: {}", res.status())));
        }
        Ok(res.json::<BrancheDeleteResponse>().await?)
    }

    pub async fn commits(&self, id: &str, branch: Option<&str>, limit: Option<u32>) -> Result<Vec<CommitInfo>, GitLitError> {
        let url = format!("{}/api/v1/commits", self.url);
        let mut req = self.http.get(url).query(&[("id", id)]);
        if let Some(b) = branch { req = req.query(&[("branch", b)]); }
        if let Some(l) = limit { req = req.query(&[("limit", l)]); }
        let res = req.send().await?;
        if !res.status().is_success() { return Err(GitLitError::Auth(format!("commits failed: {}", res.status())));}        
        Ok(res.json::<Vec<CommitInfo>>().await?)
    }


    pub async fn content(&self, id: &str, path: Option<&str>, branch: Option<&str>, commit: Option<&str>) -> Result<ContentResponse, GitLitError> {
        let url = format!("{}/api/v1/content", self.url);
        let mut req = self.http.get(url).query(&[("id", id)]);
        if let Some(p) = path { req = req.query(&[("path", p)]); }
        if let Some(b) = branch { req = req.query(&[("branch", b)]); }
        if let Some(c) = commit { req = req.query(&[("commit", c)]); }
        let res = req.send().await?;
        if !res.status().is_success() { return Err(GitLitError::Auth(format!("content failed: {}", res.status())));}        
        Ok(res.json::<ContentResponse>().await?)
    }

    pub async fn download(&self, id: &str, path: Option<&str>, branch: Option<&str>, commit: Option<&str>) -> Result<Vec<u8>, GitLitError> {
        let url = format!("{}/api/v1/download", self.url);
        let mut req = self.http.get(url).query(&[("id", id)]);
        if let Some(p) = path { req = req.query(&[("path", p)]); }
        if let Some(b) = branch { req = req.query(&[("branch", b)]); }
        if let Some(c) = commit { req = req.query(&[("commit", c)]); }
        let res = req.send().await?;
        if !res.status().is_success() { return Err(GitLitError::Auth(format!("download failed: {}", res.status())));}        
        Ok(res.bytes().await?.to_vec())
    }

    pub async fn logout(&self) -> Result<(), GitLitError> {
        let token = match self.get_token().await {
            Ok(t) => t,
            Err(GitLitError::Auth(_)) => {
                return Err(GitLitError::Unauthorized);
            }
            Err(e) => return Err(e),
        };
        let url = format!("{}/api/v1/logout", self.url);
        let req = self
            .http
            .post(url)
            .bearer_auth(&token)
            .header("Accept", "application/json")
            .header("Content-Length", 0);

        let res = req
            .send()
            .await?;
        if res.status() == reqwest::StatusCode::UNAUTHORIZED {
            let _ = self.token_store.delete(&self.host_key());
            return Err(GitLitError::Unauthorized);
        }
        if !res.status().is_success() {
            return Err(GitLitError::Auth(format!("logout failed: {}", res.status())));
        }
        self.token_store.delete(&self.host_key())?;
        Ok(())
    }
}

#[derive(Clone)]
struct TokenStore {
    path: PathBuf,
}

impl TokenStore {
    fn new() -> Result<Self, GitLitError> {
        let proj = directories::ProjectDirs::from("com", "gitlit", "gitlit-cli")
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::Other, "no config dir"))?;
        let path = proj.config_dir().join("tokens");
        Ok(Self { path })
    }

    fn token_path(&self, host: &str) -> PathBuf {
        let sanitized = host
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect::<String>();
        self.path.join(format!("{}.token", sanitized))
    }

    fn load(&self, host: &str) -> Result<Option<String>, GitLitError> {
        let p = self.token_path(host);
        if !p.exists() { return Ok(None); }
        let data = std::fs::read_to_string(&p)?;
        let token = data.trim().to_string();
        if token.is_empty() { Ok(None) } else { Ok(Some(token)) }
    }

    fn save(&self, host: &str, token: &String) -> Result<(), GitLitError> {
        let p = self.token_path(host);
        if let Some(parent) = p.parent() { std::fs::create_dir_all(parent)?; }
        std::fs::write(&p, token.as_bytes())?;
        Ok(())
    }

    fn delete(&self, host: &str) -> Result<(), GitLitError> {
        let p = self.token_path(host);
        if p.exists() { std::fs::remove_file(p)?; }
        Ok(())
    }
}
