//! Notion integration.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::{IntegrationError, IntegrationProvider, Result};

const NOTION_API_BASE: &str = "https://api.notion.com/v1";
const NOTION_VERSION: &str = "2022-06-28";

/// Notion page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotionPage {
    /// Page ID.
    pub id: String,
    /// Title.
    pub title: String,
    /// Parent ID.
    pub parent_id: Option<String>,
    /// Parent type.
    pub parent_type: ParentType,
    /// URL.
    pub url: String,
    /// Icon.
    pub icon: Option<String>,
    /// Cover image.
    pub cover: Option<String>,
    /// Properties.
    pub properties: HashMap<String, serde_json::Value>,
    /// Created at.
    pub created_at: DateTime<Utc>,
    /// Updated at.
    pub updated_at: DateTime<Utc>,
    /// Created by.
    pub created_by: String,
    /// Updated by.
    pub updated_by: String,
}

/// Parent type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParentType {
    Workspace,
    Page,
    Database,
}

/// Notion database.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotionDatabase {
    /// Database ID.
    pub id: String,
    /// Title.
    pub title: String,
    /// Description.
    pub description: Option<String>,
    /// URL.
    pub url: String,
    /// Properties schema.
    pub properties: HashMap<String, PropertySchema>,
    /// Created at.
    pub created_at: DateTime<Utc>,
    /// Updated at.
    pub updated_at: DateTime<Utc>,
}

/// Property schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropertySchema {
    /// Property ID.
    pub id: String,
    /// Property name.
    pub name: String,
    /// Property type.
    pub property_type: PropertyType,
}

/// Property type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PropertyType {
    Title,
    RichText,
    Number,
    Select,
    MultiSelect,
    Date,
    People,
    Files,
    Checkbox,
    Url,
    Email,
    PhoneNumber,
    Formula,
    Relation,
    Rollup,
    CreatedTime,
    CreatedBy,
    LastEditedTime,
    LastEditedBy,
    Status,
}

/// Notion client.
pub struct NotionClient {
    api_key: String,
    connected: bool,
    client: reqwest::Client,
}

impl NotionClient {
    /// Create a new Notion client.
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

    fn request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        let url = format!("{NOTION_API_BASE}{path}");
        self.client
            .request(method, url)
            .bearer_auth(&self.api_key)
            .header("Notion-Version", NOTION_VERSION)
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
        if status == reqwest::StatusCode::NOT_FOUND {
            return Err(IntegrationError::NotFound(
                not_found.unwrap_or_else(|| "resource".to_string()),
            ));
        }
        if !status.is_success() {
            return Err(IntegrationError::ApiError(format!(
                "Notion API error ({}): {body}",
                status.as_u16()
            )));
        }

        serde_json::from_str(&body)
            .map_err(|e| IntegrationError::ApiError(format!("Bad JSON response: {e}")))
    }

    /// Search pages and databases.
    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        if !self.connected {
            return Err(IntegrationError::AuthFailed("Not connected".to_string()));
        }
        #[derive(Debug, Deserialize)]
        struct SearchResponse {
            #[serde(default)]
            results: Vec<serde_json::Value>,
        }

        let resp: SearchResponse = self
            .send_json(
                self.request(reqwest::Method::POST, "/search")
                    .json(&serde_json::json!({
                        "query": query,
                        "page_size": limit.min(100),
                    })),
                None,
            )
            .await?;

        let mut out = Vec::new();
        for item in resp.results {
            match item.get("object").and_then(|v| v.as_str()) {
                Some("page") => {
                    if let Ok(page_api) = serde_json::from_value::<NotionPageApi>(item.clone()) {
                        out.push(SearchResult::Page(page_api.into_page()));
                    }
                }
                Some("database") => {
                    if let Ok(db_api) = serde_json::from_value::<NotionDatabaseApi>(item.clone()) {
                        out.push(SearchResult::Database(db_api.into_database()));
                    }
                }
                _ => {}
            }
        }
        Ok(out)
    }

    /// Get page by ID.
    pub async fn get_page(&self, page_id: &str) -> Result<NotionPage> {
        if !self.connected {
            return Err(IntegrationError::AuthFailed("Not connected".to_string()));
        }
        let api: NotionPageApi = self
            .send_json(
                self.request(reqwest::Method::GET, &format!("/pages/{page_id}")),
                Some(page_id.to_string()),
            )
            .await?;
        Ok(api.into_page())
    }

    /// Create page.
    pub async fn create_page(
        &self,
        parent_id: &str,
        title: &str,
        content: &str,
    ) -> Result<NotionPage> {
        if !self.connected {
            return Err(IntegrationError::AuthFailed("Not connected".to_string()));
        }
        let body = serde_json::json!({
            "parent": { "page_id": parent_id },
            "properties": {
                "title": {
                    "title": [{
                        "type": "text",
                        "text": { "content": title }
                    }]
                }
            },
            "children": [{
                "object": "block",
                "type": "paragraph",
                "paragraph": {
                    "rich_text": [{
                        "type": "text",
                        "text": { "content": content }
                    }]
                }
            }]
        });

        let api: NotionPageApi = self
            .send_json(
                self.request(reqwest::Method::POST, "/pages").json(&body),
                None,
            )
            .await?;
        Ok(api.into_page())
    }

    /// Update page.
    pub async fn update_page(
        &self,
        page_id: &str,
        properties: HashMap<String, serde_json::Value>,
    ) -> Result<NotionPage> {
        if !self.connected {
            return Err(IntegrationError::AuthFailed("Not connected".to_string()));
        }
        let api: NotionPageApi = self
            .send_json(
                self.request(reqwest::Method::PATCH, &format!("/pages/{page_id}"))
                    .json(&serde_json::json!({
                        "properties": properties,
                    })),
                Some(page_id.to_string()),
            )
            .await?;
        Ok(api.into_page())
    }

    /// Get database by ID.
    pub async fn get_database(&self, database_id: &str) -> Result<NotionDatabase> {
        if !self.connected {
            return Err(IntegrationError::AuthFailed("Not connected".to_string()));
        }
        let api: NotionDatabaseApi = self
            .send_json(
                self.request(reqwest::Method::GET, &format!("/databases/{database_id}")),
                Some(database_id.to_string()),
            )
            .await?;
        Ok(api.into_database())
    }

    /// Query database.
    pub async fn query_database(
        &self,
        database_id: &str,
        filter: Option<serde_json::Value>,
        sorts: Option<Vec<Sort>>,
    ) -> Result<Vec<NotionPage>> {
        if !self.connected {
            return Err(IntegrationError::AuthFailed("Not connected".to_string()));
        }
        #[derive(Debug, Deserialize)]
        struct QueryResponse {
            #[serde(default)]
            results: Vec<NotionPageApi>,
        }

        let mut body = serde_json::Map::new();
        if let Some(filter) = filter {
            body.insert("filter".to_string(), filter);
        }
        if let Some(sorts) = sorts {
            let sorts_json: Vec<serde_json::Value> = sorts
                .into_iter()
                .map(|s| {
                    serde_json::json!({
                        "property": s.property,
                        "direction": match s.direction {
                            SortDirection::Ascending => "ascending",
                            SortDirection::Descending => "descending",
                        }
                    })
                })
                .collect();
            body.insert("sorts".to_string(), serde_json::Value::Array(sorts_json));
        }

        let resp: QueryResponse = self
            .send_json(
                self.request(
                    reqwest::Method::POST,
                    &format!("/databases/{database_id}/query"),
                )
                .json(&body),
                Some(database_id.to_string()),
            )
            .await?;

        Ok(resp.results.into_iter().map(|p| p.into_page()).collect())
    }

    /// Add row to database.
    pub async fn add_to_database(
        &self,
        database_id: &str,
        properties: HashMap<String, serde_json::Value>,
    ) -> Result<NotionPage> {
        if !self.connected {
            return Err(IntegrationError::AuthFailed("Not connected".to_string()));
        }
        let body = serde_json::json!({
            "parent": { "database_id": database_id },
            "properties": properties
        });

        let api: NotionPageApi = self
            .send_json(
                self.request(reqwest::Method::POST, "/pages").json(&body),
                Some(database_id.to_string()),
            )
            .await?;
        Ok(api.into_page())
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum NotionParentApi {
    Workspace {
        #[serde(default)]
        workspace: Option<bool>,
    },
    PageId {
        page_id: String,
    },
    DatabaseId {
        database_id: String,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Deserialize)]
struct NotionUserRef {
    id: String,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum NotionFileOrEmoji {
    Emoji { emoji: String },
    File { file: NotionFileUrl },
    External { external: NotionFileUrl },
}

#[derive(Debug, Deserialize)]
struct NotionFileUrl {
    url: String,
}

#[derive(Debug, Deserialize)]
struct NotionPageApi {
    id: String,
    parent: NotionParentApi,
    url: String,
    #[serde(default)]
    icon: Option<NotionFileOrEmoji>,
    #[serde(default)]
    cover: Option<NotionFileOrEmoji>,
    #[serde(default)]
    properties: HashMap<String, serde_json::Value>,
    #[serde(rename = "created_time")]
    created_time: DateTime<Utc>,
    #[serde(rename = "last_edited_time")]
    last_edited_time: DateTime<Utc>,
    created_by: NotionUserRef,
    #[serde(rename = "last_edited_by")]
    last_edited_by: NotionUserRef,
}

impl NotionPageApi {
    fn into_page(self) -> NotionPage {
        let (parent_type, parent_id) = match self.parent {
            NotionParentApi::Workspace { .. } => (ParentType::Workspace, None),
            NotionParentApi::PageId { page_id } => (ParentType::Page, Some(page_id)),
            NotionParentApi::DatabaseId { database_id } => {
                (ParentType::Database, Some(database_id))
            }
            NotionParentApi::Unknown => (ParentType::Workspace, None),
        };

        NotionPage {
            id: self.id,
            title: extract_notion_title(&self.properties),
            parent_id,
            parent_type,
            url: self.url,
            icon: self.icon.and_then(icon_to_string),
            cover: self.cover.and_then(icon_to_string),
            properties: self.properties,
            created_at: self.created_time,
            updated_at: self.last_edited_time,
            created_by: self.created_by.id,
            updated_by: self.last_edited_by.id,
        }
    }
}

#[derive(Debug, Deserialize)]
struct NotionRichText {
    #[serde(default)]
    plain_text: String,
}

#[derive(Debug, Deserialize)]
struct NotionDatabaseApi {
    id: String,
    #[serde(default)]
    title: Vec<NotionRichText>,
    #[serde(default)]
    description: Vec<NotionRichText>,
    url: String,
    #[serde(default)]
    properties: HashMap<String, NotionPropertySchemaApi>,
    #[serde(rename = "created_time")]
    created_time: DateTime<Utc>,
    #[serde(rename = "last_edited_time")]
    last_edited_time: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
struct NotionPropertySchemaApi {
    id: String,
    #[serde(rename = "type")]
    property_type: String,
}

impl NotionDatabaseApi {
    fn into_database(self) -> NotionDatabase {
        let title = self
            .title
            .iter()
            .map(|t| t.plain_text.as_str())
            .collect::<Vec<_>>()
            .join("");
        let description = self
            .description
            .iter()
            .map(|t| t.plain_text.as_str())
            .collect::<Vec<_>>()
            .join("");
        let description = if description.is_empty() {
            None
        } else {
            Some(description)
        };

        let properties = self
            .properties
            .into_iter()
            .map(|(name, p)| {
                let schema = PropertySchema {
                    id: p.id,
                    name: name.clone(),
                    property_type: parse_property_type(&p.property_type),
                };
                (name, schema)
            })
            .collect();

        NotionDatabase {
            id: self.id,
            title,
            description,
            url: self.url,
            properties,
            created_at: self.created_time,
            updated_at: self.last_edited_time,
        }
    }
}

fn icon_to_string(icon: NotionFileOrEmoji) -> Option<String> {
    match icon {
        NotionFileOrEmoji::Emoji { emoji } => Some(emoji),
        NotionFileOrEmoji::File { file } => Some(file.url),
        NotionFileOrEmoji::External { external } => Some(external.url),
    }
}

fn extract_notion_title(properties: &HashMap<String, serde_json::Value>) -> String {
    for prop in properties.values() {
        if prop.get("type").and_then(|v| v.as_str()) != Some("title") {
            continue;
        }
        let Some(title_arr) = prop.get("title").and_then(|v| v.as_array()) else {
            continue;
        };

        let mut out = String::new();
        for t in title_arr {
            if let Some(s) = t.get("plain_text").and_then(|v| v.as_str()) {
                out.push_str(s);
            }
        }
        if !out.is_empty() {
            return out;
        }
    }
    String::new()
}

fn parse_property_type(s: &str) -> PropertyType {
    match s {
        "title" => PropertyType::Title,
        "rich_text" => PropertyType::RichText,
        "number" => PropertyType::Number,
        "select" => PropertyType::Select,
        "multi_select" => PropertyType::MultiSelect,
        "date" => PropertyType::Date,
        "people" => PropertyType::People,
        "files" => PropertyType::Files,
        "checkbox" => PropertyType::Checkbox,
        "url" => PropertyType::Url,
        "email" => PropertyType::Email,
        "phone_number" => PropertyType::PhoneNumber,
        "formula" => PropertyType::Formula,
        "relation" => PropertyType::Relation,
        "rollup" => PropertyType::Rollup,
        "created_time" => PropertyType::CreatedTime,
        "created_by" => PropertyType::CreatedBy,
        "last_edited_time" => PropertyType::LastEditedTime,
        "last_edited_by" => PropertyType::LastEditedBy,
        "status" => PropertyType::Status,
        _ => PropertyType::RichText,
    }
}

/// Search result.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SearchResult {
    Page(NotionPage),
    Database(NotionDatabase),
}

/// Sort configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sort {
    /// Property name.
    pub property: String,
    /// Direction.
    pub direction: SortDirection,
}

/// Sort direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortDirection {
    Ascending,
    Descending,
}

#[async_trait]
impl IntegrationProvider for NotionClient {
    fn name(&self) -> &str {
        "notion"
    }

    async fn is_connected(&self) -> bool {
        self.connected
    }

    async fn connect(&mut self) -> Result<()> {
        // Validate API key
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
    async fn test_notion_client() {
        let mut client = NotionClient::new("test-key");
        assert!(!client.is_connected().await);

        client.connect().await.unwrap();
        assert!(client.is_connected().await);
    }
}
