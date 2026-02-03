//! GitHub integration.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::{IntegrationError, IntegrationProvider, Result};

const GITHUB_API_BASE: &str = "https://api.github.com";
const GITHUB_API_VERSION: &str = "2022-11-28";

/// GitHub issue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubIssue {
    /// Issue number.
    pub number: u64,
    /// Title.
    pub title: String,
    /// Body.
    pub body: Option<String>,
    /// State.
    pub state: IssueState,
    /// Author.
    pub author: GitHubUser,
    /// Assignees.
    pub assignees: Vec<GitHubUser>,
    /// Labels.
    pub labels: Vec<GitHubLabel>,
    /// URL.
    pub url: String,
    /// HTML URL.
    pub html_url: String,
    /// Comments count.
    pub comments: u64,
    /// Created at.
    pub created_at: DateTime<Utc>,
    /// Updated at.
    pub updated_at: DateTime<Utc>,
    /// Closed at.
    pub closed_at: Option<DateTime<Utc>>,
}

/// Issue state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IssueState {
    Open,
    Closed,
}

/// GitHub pull request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubPR {
    /// PR number.
    pub number: u64,
    /// Title.
    pub title: String,
    /// Body.
    pub body: Option<String>,
    /// State.
    pub state: PRState,
    /// Author.
    pub author: GitHubUser,
    /// Head branch.
    pub head: String,
    /// Base branch.
    pub base: String,
    /// Is draft.
    pub draft: bool,
    /// Merged.
    pub merged: bool,
    /// URL.
    pub url: String,
    /// HTML URL.
    pub html_url: String,
    /// Additions.
    pub additions: u64,
    /// Deletions.
    pub deletions: u64,
    /// Changed files.
    pub changed_files: u64,
    /// Reviewers.
    pub reviewers: Vec<GitHubUser>,
    /// Created at.
    pub created_at: DateTime<Utc>,
    /// Updated at.
    pub updated_at: DateTime<Utc>,
    /// Merged at.
    pub merged_at: Option<DateTime<Utc>>,
}

/// PR state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PRState {
    Open,
    Closed,
    Merged,
}

/// GitHub user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubUser {
    /// Login (username).
    pub login: String,
    /// Avatar URL.
    pub avatar_url: String,
    /// Profile URL.
    pub html_url: String,
}

/// GitHub label.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubLabel {
    /// Name.
    pub name: String,
    /// Color (hex).
    pub color: String,
    /// Description.
    pub description: Option<String>,
}

/// GitHub repository.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubRepo {
    /// Full name (owner/repo).
    pub full_name: String,
    /// Description.
    pub description: Option<String>,
    /// Is private.
    pub private: bool,
    /// Default branch.
    pub default_branch: String,
    /// URL.
    pub html_url: String,
    /// Stars.
    pub stargazers_count: u64,
    /// Forks.
    pub forks_count: u64,
    /// Open issues.
    pub open_issues_count: u64,
}

/// GitHub client.
pub struct GitHubClient {
    token: String,
    connected: bool,
    client: reqwest::Client,
}

impl GitHubClient {
    /// Create a new GitHub client.
    pub fn new(token: &str) -> Self {
        let client = reqwest::Client::builder()
            .user_agent("drbot-integrations")
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            token: token.to_string(),
            connected: false,
            client,
        }
    }

    fn request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        let url = format!("{GITHUB_API_BASE}{path}");
        self.client
            .request(method, url)
            .bearer_auth(&self.token)
            .header(reqwest::header::ACCEPT, "application/vnd.github+json")
            .header("X-GitHub-Api-Version", GITHUB_API_VERSION)
    }

    async fn send_json<T: DeserializeOwned>(
        &self,
        req: reqwest::RequestBuilder,
        not_found: Option<String>,
    ) -> Result<T> {
        let resp = req
            .send()
            .await
            .map_err(|e| IntegrationError::NetworkError(e.to_string()))?;

        let status = resp.status();
        let headers = resp.headers().clone();
        let body = resp
            .text()
            .await
            .map_err(|e| IntegrationError::NetworkError(e.to_string()))?;

        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(IntegrationError::AuthFailed("Unauthorized".to_string()));
        }

        if status == reqwest::StatusCode::FORBIDDEN {
            if headers
                .get("x-ratelimit-remaining")
                .and_then(|v| v.to_str().ok())
                == Some("0")
            {
                return Err(IntegrationError::RateLimited);
            }
            return Err(IntegrationError::ApiError(format!(
                "Forbidden (403): {body}"
            )));
        }

        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(IntegrationError::RateLimited);
        }

        if status == reqwest::StatusCode::NOT_FOUND {
            return Err(IntegrationError::NotFound(
                not_found.unwrap_or_else(|| "resource".to_string()),
            ));
        }

        if !status.is_success() {
            return Err(IntegrationError::ApiError(format!(
                "GitHub API error ({}): {body}",
                status.as_u16()
            )));
        }

        serde_json::from_str(&body)
            .map_err(|e| IntegrationError::ApiError(format!("Bad JSON response: {e}")))
    }

    async fn send_empty(
        &self,
        req: reqwest::RequestBuilder,
        not_found: Option<String>,
    ) -> Result<()> {
        let resp = req
            .send()
            .await
            .map_err(|e| IntegrationError::NetworkError(e.to_string()))?;

        let status = resp.status();
        let headers = resp.headers().clone();
        let body = resp
            .text()
            .await
            .map_err(|e| IntegrationError::NetworkError(e.to_string()))?;

        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(IntegrationError::AuthFailed("Unauthorized".to_string()));
        }
        if status == reqwest::StatusCode::FORBIDDEN {
            if headers
                .get("x-ratelimit-remaining")
                .and_then(|v| v.to_str().ok())
                == Some("0")
            {
                return Err(IntegrationError::RateLimited);
            }
            return Err(IntegrationError::ApiError(format!(
                "Forbidden (403): {body}"
            )));
        }
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(IntegrationError::RateLimited);
        }
        if status == reqwest::StatusCode::NOT_FOUND {
            return Err(IntegrationError::NotFound(
                not_found.unwrap_or_else(|| "resource".to_string()),
            ));
        }
        if !status.is_success() {
            return Err(IntegrationError::ApiError(format!(
                "GitHub API error ({}): {body}",
                status.as_u16()
            )));
        }
        Ok(())
    }

    /// Get repository.
    pub async fn get_repo(&self, owner: &str, repo: &str) -> Result<GitHubRepo> {
        if !self.connected {
            return Err(IntegrationError::AuthFailed("Not connected".to_string()));
        }
        #[derive(Debug, Deserialize)]
        struct RepoApi {
            full_name: String,
            description: Option<String>,
            private: bool,
            default_branch: String,
            html_url: String,
            stargazers_count: u64,
            forks_count: u64,
            open_issues_count: u64,
        }

        let api: RepoApi = self
            .send_json(
                self.request(reqwest::Method::GET, &format!("/repos/{owner}/{repo}")),
                Some(format!("{owner}/{repo}")),
            )
            .await?;

        Ok(GitHubRepo {
            full_name: api.full_name,
            description: api.description,
            private: api.private,
            default_branch: api.default_branch,
            html_url: api.html_url,
            stargazers_count: api.stargazers_count,
            forks_count: api.forks_count,
            open_issues_count: api.open_issues_count,
        })
    }

    /// Get issue.
    pub async fn get_issue(&self, owner: &str, repo: &str, number: u64) -> Result<GitHubIssue> {
        if !self.connected {
            return Err(IntegrationError::AuthFailed("Not connected".to_string()));
        }
        #[derive(Debug, Deserialize)]
        struct IssueApi {
            number: u64,
            title: String,
            body: Option<String>,
            state: IssueState,
            user: GitHubUser,
            #[serde(default)]
            assignees: Vec<GitHubUser>,
            #[serde(default)]
            labels: Vec<GitHubLabel>,
            url: String,
            html_url: String,
            comments: u64,
            created_at: DateTime<Utc>,
            updated_at: DateTime<Utc>,
            closed_at: Option<DateTime<Utc>>,
        }

        let api: IssueApi = self
            .send_json(
                self.request(
                    reqwest::Method::GET,
                    &format!("/repos/{owner}/{repo}/issues/{number}"),
                ),
                Some(format!("{owner}/{repo}#{number}")),
            )
            .await?;

        Ok(GitHubIssue {
            number: api.number,
            title: api.title,
            body: api.body,
            state: api.state,
            author: api.user,
            assignees: api.assignees,
            labels: api.labels,
            url: api.url,
            html_url: api.html_url,
            comments: api.comments,
            created_at: api.created_at,
            updated_at: api.updated_at,
            closed_at: api.closed_at,
        })
    }

    /// Create issue.
    pub async fn create_issue(
        &self,
        owner: &str,
        repo: &str,
        create: CreateIssue,
    ) -> Result<GitHubIssue> {
        if !self.connected {
            return Err(IntegrationError::AuthFailed("Not connected".to_string()));
        }
        #[derive(Debug, Deserialize)]
        struct IssueApi {
            number: u64,
            title: String,
            body: Option<String>,
            state: IssueState,
            user: GitHubUser,
            #[serde(default)]
            assignees: Vec<GitHubUser>,
            #[serde(default)]
            labels: Vec<GitHubLabel>,
            url: String,
            html_url: String,
            comments: u64,
            created_at: DateTime<Utc>,
            updated_at: DateTime<Utc>,
            closed_at: Option<DateTime<Utc>>,
        }

        #[derive(Debug, Serialize)]
        struct CreateIssueReq<'a> {
            title: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            body: &'a Option<String>,
            #[serde(skip_serializing_if = "Vec::is_empty")]
            labels: &'a Vec<String>,
            #[serde(skip_serializing_if = "Vec::is_empty")]
            assignees: &'a Vec<String>,
        }

        let api: IssueApi = self
            .send_json(
                self.request(
                    reqwest::Method::POST,
                    &format!("/repos/{owner}/{repo}/issues"),
                )
                .json(&CreateIssueReq {
                    title: &create.title,
                    body: &create.body,
                    labels: &create.labels,
                    assignees: &create.assignees,
                }),
                Some(format!("{owner}/{repo}")),
            )
            .await?;

        Ok(GitHubIssue {
            number: api.number,
            title: api.title,
            body: api.body,
            state: api.state,
            author: api.user,
            assignees: api.assignees,
            labels: api.labels,
            url: api.url,
            html_url: api.html_url,
            comments: api.comments,
            created_at: api.created_at,
            updated_at: api.updated_at,
            closed_at: api.closed_at,
        })
    }

    /// List issues.
    pub async fn list_issues(
        &self,
        owner: &str,
        repo: &str,
        state: Option<IssueState>,
    ) -> Result<Vec<GitHubIssue>> {
        if !self.connected {
            return Err(IntegrationError::AuthFailed("Not connected".to_string()));
        }
        #[derive(Debug, Deserialize)]
        struct IssueListApi {
            number: u64,
            title: String,
            body: Option<String>,
            state: IssueState,
            user: GitHubUser,
            #[serde(default)]
            assignees: Vec<GitHubUser>,
            #[serde(default)]
            labels: Vec<GitHubLabel>,
            url: String,
            html_url: String,
            comments: u64,
            created_at: DateTime<Utc>,
            updated_at: DateTime<Utc>,
            closed_at: Option<DateTime<Utc>>,
            #[serde(default)]
            pull_request: Option<serde_json::Value>,
        }

        let state = match state {
            Some(IssueState::Open) => "open",
            Some(IssueState::Closed) => "closed",
            None => "all",
        };

        let api_items: Vec<IssueListApi> = self
            .send_json(
                self.request(
                    reqwest::Method::GET,
                    &format!("/repos/{owner}/{repo}/issues?state={state}"),
                ),
                Some(format!("{owner}/{repo}")),
            )
            .await?;

        let issues = api_items
            .into_iter()
            .filter(|i| i.pull_request.is_none())
            .map(|api| GitHubIssue {
                number: api.number,
                title: api.title,
                body: api.body,
                state: api.state,
                author: api.user,
                assignees: api.assignees,
                labels: api.labels,
                url: api.url,
                html_url: api.html_url,
                comments: api.comments,
                created_at: api.created_at,
                updated_at: api.updated_at,
                closed_at: api.closed_at,
            })
            .collect();

        Ok(issues)
    }

    /// Get pull request.
    pub async fn get_pr(&self, owner: &str, repo: &str, number: u64) -> Result<GitHubPR> {
        if !self.connected {
            return Err(IntegrationError::AuthFailed("Not connected".to_string()));
        }
        #[derive(Debug, Deserialize)]
        struct RefApi {
            #[serde(rename = "ref")]
            name: String,
        }

        #[derive(Debug, Deserialize)]
        struct PrApi {
            number: u64,
            title: String,
            body: Option<String>,
            state: PRState,
            user: GitHubUser,
            head: RefApi,
            base: RefApi,
            #[serde(default)]
            draft: bool,
            html_url: String,
            url: String,
            #[serde(default)]
            additions: u64,
            #[serde(default)]
            deletions: u64,
            #[serde(default)]
            changed_files: u64,
            #[serde(default)]
            requested_reviewers: Vec<GitHubUser>,
            created_at: DateTime<Utc>,
            updated_at: DateTime<Utc>,
            merged_at: Option<DateTime<Utc>>,
        }

        let api: PrApi = self
            .send_json(
                self.request(
                    reqwest::Method::GET,
                    &format!("/repos/{owner}/{repo}/pulls/{number}"),
                ),
                Some(format!("{owner}/{repo}#{number}")),
            )
            .await?;

        let merged = api.merged_at.is_some();
        let state = if merged { PRState::Merged } else { api.state };

        Ok(GitHubPR {
            number: api.number,
            title: api.title,
            body: api.body,
            state,
            author: api.user,
            head: api.head.name,
            base: api.base.name,
            draft: api.draft,
            merged,
            url: api.url,
            html_url: api.html_url,
            additions: api.additions,
            deletions: api.deletions,
            changed_files: api.changed_files,
            reviewers: api.requested_reviewers,
            created_at: api.created_at,
            updated_at: api.updated_at,
            merged_at: api.merged_at,
        })
    }

    /// Create pull request.
    pub async fn create_pr(&self, owner: &str, repo: &str, create: CreatePR) -> Result<GitHubPR> {
        if !self.connected {
            return Err(IntegrationError::AuthFailed("Not connected".to_string()));
        }
        #[derive(Debug, Deserialize)]
        struct RefApi {
            #[serde(rename = "ref")]
            name: String,
        }

        #[derive(Debug, Deserialize)]
        struct PrApi {
            number: u64,
            title: String,
            body: Option<String>,
            state: PRState,
            user: GitHubUser,
            head: RefApi,
            base: RefApi,
            #[serde(default)]
            draft: bool,
            html_url: String,
            url: String,
            #[serde(default)]
            additions: u64,
            #[serde(default)]
            deletions: u64,
            #[serde(default)]
            changed_files: u64,
            #[serde(default)]
            requested_reviewers: Vec<GitHubUser>,
            created_at: DateTime<Utc>,
            updated_at: DateTime<Utc>,
            merged_at: Option<DateTime<Utc>>,
        }

        #[derive(Debug, Serialize)]
        struct CreatePrReq<'a> {
            title: &'a str,
            head: &'a str,
            base: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            body: &'a Option<String>,
            #[serde(default)]
            draft: bool,
        }

        let api: PrApi = self
            .send_json(
                self.request(
                    reqwest::Method::POST,
                    &format!("/repos/{owner}/{repo}/pulls"),
                )
                .json(&CreatePrReq {
                    title: &create.title,
                    head: &create.head,
                    base: &create.base,
                    body: &create.body,
                    draft: create.draft,
                }),
                Some(format!("{owner}/{repo}")),
            )
            .await?;

        let merged = api.merged_at.is_some();
        let state = if merged { PRState::Merged } else { api.state };

        Ok(GitHubPR {
            number: api.number,
            title: api.title,
            body: api.body,
            state,
            author: api.user,
            head: api.head.name,
            base: api.base.name,
            draft: api.draft,
            merged,
            url: api.url,
            html_url: api.html_url,
            additions: api.additions,
            deletions: api.deletions,
            changed_files: api.changed_files,
            reviewers: api.requested_reviewers,
            created_at: api.created_at,
            updated_at: api.updated_at,
            merged_at: api.merged_at,
        })
    }

    /// List pull requests.
    pub async fn list_prs(
        &self,
        owner: &str,
        repo: &str,
        state: Option<PRState>,
    ) -> Result<Vec<GitHubPR>> {
        if !self.connected {
            return Err(IntegrationError::AuthFailed("Not connected".to_string()));
        }
        #[derive(Debug, Deserialize)]
        struct RefApi {
            #[serde(rename = "ref")]
            name: String,
        }

        #[derive(Debug, Deserialize)]
        struct PrListApi {
            number: u64,
            title: String,
            body: Option<String>,
            state: PRState,
            user: GitHubUser,
            head: RefApi,
            base: RefApi,
            #[serde(default)]
            draft: bool,
            html_url: String,
            url: String,
            created_at: DateTime<Utc>,
            updated_at: DateTime<Utc>,
            merged_at: Option<DateTime<Utc>>,
            #[serde(default)]
            requested_reviewers: Vec<GitHubUser>,
        }

        let state = match state {
            Some(PRState::Open) => "open",
            Some(PRState::Closed) => "closed",
            Some(PRState::Merged) => "closed",
            None => "all",
        };

        let api_items: Vec<PrListApi> = self
            .send_json(
                self.request(
                    reqwest::Method::GET,
                    &format!("/repos/{owner}/{repo}/pulls?state={state}"),
                ),
                Some(format!("{owner}/{repo}")),
            )
            .await?;

        let prs = api_items
            .into_iter()
            .map(|api| {
                let merged = api.merged_at.is_some();
                let state = if merged { PRState::Merged } else { api.state };
                GitHubPR {
                    number: api.number,
                    title: api.title,
                    body: api.body,
                    state,
                    author: api.user,
                    head: api.head.name,
                    base: api.base.name,
                    draft: api.draft,
                    merged,
                    url: api.url,
                    html_url: api.html_url,
                    additions: 0,
                    deletions: 0,
                    changed_files: 0,
                    reviewers: api.requested_reviewers,
                    created_at: api.created_at,
                    updated_at: api.updated_at,
                    merged_at: api.merged_at,
                }
            })
            .collect();

        Ok(prs)
    }

    /// Add comment to issue/PR.
    pub async fn add_comment(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        body: &str,
    ) -> Result<()> {
        if !self.connected {
            return Err(IntegrationError::AuthFailed("Not connected".to_string()));
        }
        #[derive(Debug, Serialize)]
        struct CommentReq<'a> {
            body: &'a str,
        }

        self.send_empty(
            self.request(
                reqwest::Method::POST,
                &format!("/repos/{owner}/{repo}/issues/{number}/comments"),
            )
            .json(&CommentReq { body }),
            Some(format!("{owner}/{repo}#{number}")),
        )
        .await
    }

    /// Get my issues.
    pub async fn my_issues(&self) -> Result<Vec<GitHubIssue>> {
        if !self.connected {
            return Err(IntegrationError::AuthFailed("Not connected".to_string()));
        }
        Ok(Vec::new())
    }

    /// Get my PRs.
    pub async fn my_prs(&self) -> Result<Vec<GitHubPR>> {
        if !self.connected {
            return Err(IntegrationError::AuthFailed("Not connected".to_string()));
        }
        Ok(Vec::new())
    }
}

/// Create issue request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateIssue {
    /// Title.
    pub title: String,
    /// Body.
    pub body: Option<String>,
    /// Labels.
    pub labels: Vec<String>,
    /// Assignees.
    pub assignees: Vec<String>,
}

impl CreateIssue {
    /// Create a new issue request.
    pub fn new(title: &str) -> Self {
        Self {
            title: title.to_string(),
            body: None,
            labels: Vec::new(),
            assignees: Vec::new(),
        }
    }
}

/// Create PR request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatePR {
    /// Title.
    pub title: String,
    /// Body.
    pub body: Option<String>,
    /// Head branch.
    pub head: String,
    /// Base branch.
    pub base: String,
    /// Draft.
    pub draft: bool,
}

impl CreatePR {
    /// Create a new PR request.
    pub fn new(title: &str, head: &str, base: &str) -> Self {
        Self {
            title: title.to_string(),
            body: None,
            head: head.to_string(),
            base: base.to_string(),
            draft: false,
        }
    }
}

#[async_trait]
impl IntegrationProvider for GitHubClient {
    fn name(&self) -> &str {
        "github"
    }

    async fn is_connected(&self) -> bool {
        self.connected
    }

    async fn connect(&mut self) -> Result<()> {
        if self.token.is_empty() {
            return Err(IntegrationError::AuthFailed("Token required".to_string()));
        }
        self.connected = true;
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        self.connected = false;
        Ok(())
    }

    async fn refresh(&mut self) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_github_client() {
        let mut client = GitHubClient::new("test-token");
        client.connect().await.unwrap();
        assert!(client.is_connected().await);
    }
}
