//! Linear integration.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::{IntegrationError, IntegrationProvider, Result};

const LINEAR_API_URL: &str = "https://api.linear.app/graphql";

/// Linear issue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinearIssue {
    /// Issue ID.
    pub id: String,
    /// Issue identifier (e.g., "ENG-123").
    pub identifier: String,
    /// Title.
    pub title: String,
    /// Description.
    pub description: Option<String>,
    /// Priority (0-4, 0 = no priority, 1 = urgent, 4 = low).
    pub priority: u8,
    /// State.
    pub state: IssueState,
    /// Assignee.
    pub assignee: Option<User>,
    /// Labels.
    pub labels: Vec<Label>,
    /// Project.
    pub project: Option<LinearProject>,
    /// Team.
    pub team: Team,
    /// URL.
    pub url: String,
    /// Due date.
    pub due_date: Option<DateTime<Utc>>,
    /// Created at.
    pub created_at: DateTime<Utc>,
    /// Updated at.
    pub updated_at: DateTime<Utc>,
}

/// Issue state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueState {
    /// State ID.
    pub id: String,
    /// State name.
    pub name: String,
    /// State type.
    pub state_type: StateType,
    /// Color.
    pub color: String,
}

/// State type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StateType {
    Backlog,
    Unstarted,
    Started,
    Completed,
    #[serde(rename = "canceled", alias = "cancelled")]
    Cancelled,
}

/// User.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    /// User ID.
    pub id: String,
    /// Name.
    pub name: String,
    /// Email.
    pub email: String,
    /// Avatar URL.
    pub avatar_url: Option<String>,
}

/// Label.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Label {
    /// Label ID.
    pub id: String,
    /// Name.
    pub name: String,
    /// Color.
    pub color: String,
}

/// Linear project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinearProject {
    /// Project ID.
    pub id: String,
    /// Name.
    pub name: String,
    /// Description.
    pub description: Option<String>,
    /// Icon.
    pub icon: Option<String>,
    /// Color.
    pub color: Option<String>,
    /// State.
    pub state: ProjectState,
    /// Progress (0-100).
    pub progress: u8,
    /// Target date.
    pub target_date: Option<DateTime<Utc>>,
}

/// Project state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectState {
    Planned,
    Started,
    Paused,
    Completed,
    #[serde(rename = "canceled", alias = "cancelled")]
    Cancelled,
}

/// Team.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Team {
    /// Team ID.
    pub id: String,
    /// Name.
    pub name: String,
    /// Key (e.g., "ENG").
    pub key: String,
}

/// Linear client.
pub struct LinearClient {
    api_key: String,
    connected: bool,
    client: reqwest::Client,
}

impl LinearClient {
    /// Create a new Linear client.
    pub fn new(api_key: &str) -> Self {
        let client = reqwest::Client::builder()
            .user_agent("drbot-integrations")
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            api_key: api_key.to_string(),
            connected: false,
            client,
        }
    }

    fn request(&self) -> reqwest::RequestBuilder {
        self.client
            .post(LINEAR_API_URL)
            .bearer_auth(&self.api_key)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
    }

    async fn graphql<T: DeserializeOwned>(
        &self,
        query: &str,
        variables: serde_json::Value,
    ) -> Result<T> {
        #[derive(Debug, Deserialize)]
        struct GraphQlError {
            message: String,
        }

        #[derive(Debug, Deserialize)]
        struct GraphQlResponse<T> {
            data: Option<T>,
            #[serde(default)]
            errors: Vec<GraphQlError>,
        }

        let resp = self
            .request()
            .json(&serde_json::json!({ "query": query, "variables": variables }))
            .send()
            .await
            .map_err(|e| IntegrationError::NetworkError(e.to_string()))?;

        let status = resp.status();
        let body = resp
            .text()
            .await
            .map_err(|e| IntegrationError::NetworkError(e.to_string()))?;

        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            return Err(IntegrationError::AuthFailed("Unauthorized".to_string()));
        }
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(IntegrationError::RateLimited);
        }
        if !status.is_success() {
            return Err(IntegrationError::ApiError(format!(
                "Linear API error ({}): {body}",
                status.as_u16()
            )));
        }

        let parsed: GraphQlResponse<T> = serde_json::from_str(&body)
            .map_err(|e| IntegrationError::ApiError(format!("Bad JSON response: {e}")))?;

        if !parsed.errors.is_empty() {
            let msg = parsed
                .errors
                .into_iter()
                .map(|e| e.message)
                .collect::<Vec<_>>()
                .join("; ");
            return Err(IntegrationError::ApiError(msg));
        }

        parsed
            .data
            .ok_or_else(|| IntegrationError::ApiError("Missing data".to_string()))
    }

    /// Get issue by identifier.
    pub async fn get_issue(&self, identifier: &str) -> Result<LinearIssue> {
        if !self.connected {
            return Err(IntegrationError::AuthFailed("Not connected".to_string()));
        }
        Err(IntegrationError::NotFound(identifier.to_string()))
    }

    /// Create issue.
    pub async fn create_issue(&self, create: CreateIssue) -> Result<LinearIssue> {
        if !self.connected {
            return Err(IntegrationError::AuthFailed("Not connected".to_string()));
        }
        #[derive(Debug, Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct IssueCreateData {
            issue_create: IssueCreatePayload,
        }

        #[derive(Debug, Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct IssueCreatePayload {
            success: bool,
            issue: Option<IssueNode>,
        }

        #[derive(Debug, Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct IssueNode {
            id: String,
            identifier: String,
            title: String,
            description: Option<String>,
            priority: u8,
            url: String,
            due_date: Option<String>,
            created_at: DateTime<Utc>,
            updated_at: DateTime<Utc>,
            state: StateNode,
            assignee: Option<UserNode>,
            labels: LabelsNode,
            team: TeamNode,
            project: Option<ProjectNode>,
        }

        #[derive(Debug, Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct StateNode {
            id: String,
            name: String,
            #[serde(rename = "type")]
            state_type: String,
            color: String,
        }

        #[derive(Debug, Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct UserNode {
            id: String,
            name: String,
            email: String,
            avatar_url: Option<String>,
        }

        #[derive(Debug, Deserialize)]
        struct LabelsNode {
            #[serde(default)]
            nodes: Vec<LabelNode>,
        }

        #[derive(Debug, Deserialize)]
        struct LabelNode {
            id: String,
            name: String,
            color: String,
        }

        #[derive(Debug, Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct TeamNode {
            id: String,
            name: String,
            key: String,
        }

        #[derive(Debug, Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct ProjectNode {
            id: String,
            name: String,
            description: Option<String>,
            icon: Option<String>,
            color: Option<String>,
            state: String,
            progress: u8,
            target_date: Option<String>,
        }

        let query = r#"
            mutation IssueCreate($input: IssueCreateInput!) {
              issueCreate(input: $input) {
                success
                issue {
                  id
                  identifier
                  title
                  description
                  priority
                  url
                  dueDate
                  createdAt
                  updatedAt
                  state { id name type color }
                  assignee { id name email avatarUrl }
                  labels { nodes { id name color } }
                  team { id name key }
                  project { id name description icon color state progress targetDate }
                }
              }
            }
        "#;

        let variables = serde_json::json!({
            "input": {
                "teamId": create.team_id,
                "title": create.title,
                "description": create.description,
                "priority": create.priority,
                "assigneeId": create.assignee_id,
                "projectId": create.project_id,
                "labelIds": create.label_ids,
            }
        });

        let data: IssueCreateData = self.graphql(query, variables).await?;
        if !data.issue_create.success {
            return Err(IntegrationError::ApiError(
                "Issue creation failed".to_string(),
            ));
        }

        let issue = data
            .issue_create
            .issue
            .ok_or_else(|| IntegrationError::ApiError("No issue returned".to_string()))?;

        Ok(LinearIssue {
            id: issue.id,
            identifier: issue.identifier,
            title: issue.title,
            description: issue.description,
            priority: issue.priority,
            state: IssueState {
                id: issue.state.id,
                name: issue.state.name,
                state_type: parse_state_type(&issue.state.state_type),
                color: issue.state.color,
            },
            assignee: issue.assignee.map(|u| User {
                id: u.id,
                name: u.name,
                email: u.email,
                avatar_url: u.avatar_url,
            }),
            labels: issue
                .labels
                .nodes
                .into_iter()
                .map(|l| Label {
                    id: l.id,
                    name: l.name,
                    color: l.color,
                })
                .collect(),
            project: issue.project.map(|p| LinearProject {
                id: p.id,
                name: p.name,
                description: p.description,
                icon: p.icon,
                color: p.color,
                state: parse_project_state(&p.state),
                progress: p.progress,
                target_date: parse_timeless_date(&p.target_date),
            }),
            team: Team {
                id: issue.team.id,
                name: issue.team.name,
                key: issue.team.key,
            },
            url: issue.url,
            due_date: parse_timeless_date(&issue.due_date),
            created_at: issue.created_at,
            updated_at: issue.updated_at,
        })
    }

    /// Update issue.
    pub async fn update_issue(&self, id: &str, update: UpdateIssue) -> Result<LinearIssue> {
        if !self.connected {
            return Err(IntegrationError::AuthFailed("Not connected".to_string()));
        }
        Err(IntegrationError::NotFound(id.to_string()))
    }

    /// Search issues.
    pub async fn search_issues(&self, query: &str) -> Result<Vec<LinearIssue>> {
        if !self.connected {
            return Err(IntegrationError::AuthFailed("Not connected".to_string()));
        }
        Ok(Vec::new())
    }

    /// Get my issues.
    pub async fn my_issues(&self) -> Result<Vec<LinearIssue>> {
        if !self.connected {
            return Err(IntegrationError::AuthFailed("Not connected".to_string()));
        }
        Ok(Vec::new())
    }

    /// Get team issues.
    pub async fn team_issues(&self, team_id: &str) -> Result<Vec<LinearIssue>> {
        if !self.connected {
            return Err(IntegrationError::AuthFailed("Not connected".to_string()));
        }
        Ok(Vec::new())
    }

    /// Get projects.
    pub async fn get_projects(&self) -> Result<Vec<LinearProject>> {
        if !self.connected {
            return Err(IntegrationError::AuthFailed("Not connected".to_string()));
        }
        Ok(Vec::new())
    }

    /// Get teams.
    pub async fn get_teams(&self) -> Result<Vec<Team>> {
        if !self.connected {
            return Err(IntegrationError::AuthFailed("Not connected".to_string()));
        }
        Ok(Vec::new())
    }
}

fn parse_state_type(s: &str) -> StateType {
    match s.to_ascii_lowercase().as_str() {
        "backlog" => StateType::Backlog,
        "unstarted" => StateType::Unstarted,
        "started" => StateType::Started,
        "completed" => StateType::Completed,
        "canceled" | "cancelled" => StateType::Cancelled,
        _ => StateType::Unstarted,
    }
}

fn parse_project_state(s: &str) -> ProjectState {
    match s.to_ascii_lowercase().as_str() {
        "planned" => ProjectState::Planned,
        "started" => ProjectState::Started,
        "paused" => ProjectState::Paused,
        "completed" => ProjectState::Completed,
        "canceled" | "cancelled" => ProjectState::Cancelled,
        _ => ProjectState::Planned,
    }
}

fn parse_timeless_date(s: &Option<String>) -> Option<DateTime<Utc>> {
    let s = s.as_deref()?;
    if let Ok(date) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return Some(DateTime::<Utc>::from_naive_utc_and_offset(
            date.and_hms_opt(0, 0, 0)?,
            Utc,
        ));
    }
    None
}

/// Create issue request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateIssue {
    /// Team ID.
    pub team_id: String,
    /// Title.
    pub title: String,
    /// Description.
    pub description: Option<String>,
    /// Priority.
    pub priority: Option<u8>,
    /// Assignee ID.
    pub assignee_id: Option<String>,
    /// Project ID.
    pub project_id: Option<String>,
    /// Label IDs.
    pub label_ids: Vec<String>,
}

impl CreateIssue {
    /// Create a new issue request.
    pub fn new(team_id: &str, title: &str) -> Self {
        Self {
            team_id: team_id.to_string(),
            title: title.to_string(),
            description: None,
            priority: None,
            assignee_id: None,
            project_id: None,
            label_ids: Vec::new(),
        }
    }
}

/// Update issue request.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateIssue {
    /// Title.
    pub title: Option<String>,
    /// Description.
    pub description: Option<String>,
    /// Priority.
    pub priority: Option<u8>,
    /// State ID.
    pub state_id: Option<String>,
    /// Assignee ID.
    pub assignee_id: Option<String>,
}

#[async_trait]
impl IntegrationProvider for LinearClient {
    fn name(&self) -> &str {
        "linear"
    }

    async fn is_connected(&self) -> bool {
        self.connected
    }

    async fn connect(&mut self) -> Result<()> {
        if self.api_key.is_empty() {
            return Err(IntegrationError::AuthFailed("API key required".to_string()));
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
    async fn test_linear_client() {
        let mut client = LinearClient::new("test-key");
        client.connect().await.unwrap();
        assert!(client.is_connected().await);
    }
}
