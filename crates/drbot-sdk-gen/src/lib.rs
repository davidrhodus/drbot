//! Auto-generate client SDKs from API definitions.
//!
//! This crate provides:
//! - OpenAPI parsing
//! - Multi-language SDK generation
//! - Type-safe client generation
//! - Documentation generation

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

/// SDK generation errors.
#[derive(Debug, Error)]
pub enum SdkGenError {
    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Generation error: {0}")]
    GenerationError(String),

    #[error("Unsupported language: {0}")]
    UnsupportedLanguage(String),

    #[error("Invalid schema: {0}")]
    InvalidSchema(String),
}

/// Result type for SDK generation.
pub type Result<T> = std::result::Result<T, SdkGenError>;

/// API definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiDefinition {
    /// API identifier.
    pub id: String,
    /// API name.
    pub name: String,
    /// Version.
    pub version: String,
    /// Base URL.
    pub base_url: String,
    /// Endpoints.
    pub endpoints: Vec<Endpoint>,
    /// Types.
    pub types: Vec<TypeDefinition>,
    /// Authentication.
    pub auth: Option<AuthDefinition>,
    /// Description.
    pub description: Option<String>,
}

/// An API endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Endpoint {
    /// Endpoint identifier.
    pub id: String,
    /// Path.
    pub path: String,
    /// HTTP method.
    pub method: HttpMethod,
    /// Operation name.
    pub operation: String,
    /// Description.
    pub description: Option<String>,
    /// Request body type.
    pub request_body: Option<TypeRef>,
    /// Response type.
    pub response: Option<TypeRef>,
    /// Path parameters.
    pub path_params: Vec<Parameter>,
    /// Query parameters.
    pub query_params: Vec<Parameter>,
    /// Headers.
    pub headers: Vec<Parameter>,
    /// Tags.
    pub tags: Vec<String>,
}

/// HTTP methods.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Head,
    Options,
}

/// A parameter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Parameter {
    /// Parameter name.
    pub name: String,
    /// Type reference.
    pub type_ref: TypeRef,
    /// Required.
    pub required: bool,
    /// Description.
    pub description: Option<String>,
    /// Default value.
    pub default: Option<String>,
}

/// Type reference.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeRef {
    /// Type name.
    pub name: String,
    /// Is array.
    pub is_array: bool,
    /// Is optional.
    pub is_optional: bool,
    /// Generic parameters.
    pub generics: Vec<TypeRef>,
}

/// Type definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeDefinition {
    /// Type name.
    pub name: String,
    /// Type kind.
    pub kind: TypeKind,
    /// Fields (for object types).
    pub fields: Vec<Field>,
    /// Enum variants (for enum types).
    pub variants: Vec<EnumVariant>,
    /// Description.
    pub description: Option<String>,
}

/// Type kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TypeKind {
    Object,
    Enum,
    Alias,
    Primitive,
}

/// A field in an object type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Field {
    /// Field name.
    pub name: String,
    /// Type reference.
    pub type_ref: TypeRef,
    /// Required.
    pub required: bool,
    /// Description.
    pub description: Option<String>,
}

/// An enum variant.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnumVariant {
    /// Variant name.
    pub name: String,
    /// Value (if string enum).
    pub value: Option<String>,
    /// Description.
    pub description: Option<String>,
}

/// Authentication definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthDefinition {
    /// Auth type.
    pub auth_type: AuthType,
    /// Header name (for API key).
    pub header: Option<String>,
    /// Token URL (for OAuth).
    pub token_url: Option<String>,
}

/// Authentication types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthType {
    None,
    ApiKey,
    Bearer,
    Basic,
    OAuth2,
}

/// Target language.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Language {
    TypeScript,
    Python,
    Rust,
    Go,
    Java,
    CSharp,
    Swift,
    Kotlin,
}

/// Generated SDK.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedSdk {
    /// SDK identifier.
    pub id: String,
    /// API name.
    pub api_name: String,
    /// Language.
    pub language: Language,
    /// Generated files.
    pub files: Vec<GeneratedFile>,
    /// Package name.
    pub package_name: String,
    /// Version.
    pub version: String,
    /// Generated at.
    pub generated_at: DateTime<Utc>,
}

/// A generated file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedFile {
    /// File path.
    pub path: String,
    /// File content.
    pub content: String,
    /// File type.
    pub file_type: FileType,
}

/// File types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileType {
    Source,
    Config,
    Documentation,
    Test,
}

/// Generation options.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationOptions {
    /// Package name.
    pub package_name: String,
    /// Version.
    pub version: String,
    /// Include tests.
    pub include_tests: bool,
    /// Include documentation.
    pub include_docs: bool,
    /// Generate async client.
    pub async_client: bool,
    /// Custom templates.
    pub templates: HashMap<String, String>,
}

impl Default for GenerationOptions {
    fn default() -> Self {
        Self {
            package_name: "generated-client".to_string(),
            version: "1.0.0".to_string(),
            include_tests: true,
            include_docs: true,
            async_client: true,
            templates: HashMap::new(),
        }
    }
}

/// SDK generator provider.
#[async_trait]
pub trait SdkGenerator: Send + Sync {
    /// Generate SDK for language.
    async fn generate(
        &self,
        api: &ApiDefinition,
        language: Language,
        options: &GenerationOptions,
    ) -> Result<GeneratedSdk>;

    /// Supported languages.
    fn supported_languages(&self) -> Vec<Language>;
}

/// The SDK generation engine.
pub struct SdkGenEngine {
    /// Generators by language.
    generators: HashMap<Language, Arc<dyn SdkGenerator>>,
    /// API definitions.
    apis: Arc<RwLock<HashMap<String, ApiDefinition>>>,
    /// Generated SDKs.
    generated: Arc<RwLock<Vec<GeneratedSdk>>>,
}

impl SdkGenEngine {
    /// Create a new SDK generation engine.
    pub fn new() -> Self {
        Self {
            generators: HashMap::new(),
            apis: Arc::new(RwLock::new(HashMap::new())),
            generated: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Register a generator.
    pub fn register_generator(&mut self, language: Language, generator: Arc<dyn SdkGenerator>) {
        self.generators.insert(language, generator);
    }

    /// Parse OpenAPI spec.
    pub async fn parse_openapi(&self, spec: &str) -> Result<ApiDefinition> {
        // Simplified parser - in real implementation would use openapi crate
        let parsed: serde_json::Value =
            serde_json::from_str(spec).map_err(|e| SdkGenError::ParseError(e.to_string()))?;

        let info = parsed
            .get("info")
            .ok_or_else(|| SdkGenError::ParseError("Missing info".to_string()))?;

        let api = ApiDefinition {
            id: Uuid::new_v4().to_string(),
            name: info
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("API")
                .to_string(),
            version: info
                .get("version")
                .and_then(|v| v.as_str())
                .unwrap_or("1.0.0")
                .to_string(),
            base_url: parsed
                .get("servers")
                .and_then(|s| s.as_array())
                .and_then(|a| a.first())
                .and_then(|s| s.get("url"))
                .and_then(|u| u.as_str())
                .unwrap_or("http://localhost")
                .to_string(),
            endpoints: Vec::new(),
            types: Vec::new(),
            auth: None,
            description: info
                .get("description")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
        };

        let mut apis = self.apis.write().await;
        apis.insert(api.id.clone(), api.clone());

        Ok(api)
    }

    /// Register API definition.
    pub async fn register_api(&self, api: ApiDefinition) -> String {
        let id = api.id.clone();
        let mut apis = self.apis.write().await;
        apis.insert(id.clone(), api);
        id
    }

    /// Generate SDK.
    pub async fn generate(
        &self,
        api_id: &str,
        language: Language,
        options: Option<GenerationOptions>,
    ) -> Result<GeneratedSdk> {
        let apis = self.apis.read().await;
        let api = apis
            .get(api_id)
            .ok_or_else(|| SdkGenError::ParseError(format!("API {} not found", api_id)))?
            .clone();

        let generator = self
            .generators
            .get(&language)
            .ok_or_else(|| SdkGenError::UnsupportedLanguage(format!("{:?}", language)))?;

        let opts = options.unwrap_or_default();
        let sdk = generator.generate(&api, language, &opts).await?;

        let mut generated = self.generated.write().await;
        generated.push(sdk.clone());

        Ok(sdk)
    }

    /// Generate for all supported languages.
    pub async fn generate_all(
        &self,
        api_id: &str,
        options: Option<GenerationOptions>,
    ) -> Result<Vec<GeneratedSdk>> {
        let mut sdks = Vec::new();
        let opts = options.unwrap_or_default();

        for language in self.generators.keys() {
            match self.generate(api_id, *language, Some(opts.clone())).await {
                Ok(sdk) => sdks.push(sdk),
                Err(e) => eprintln!("Failed to generate {:?} SDK: {}", language, e),
            }
        }

        Ok(sdks)
    }

    /// Get generated SDKs.
    pub async fn get_generated(&self, api_id: Option<&str>) -> Vec<GeneratedSdk> {
        let generated = self.generated.read().await;
        match api_id {
            Some(id) => generated.iter().filter(|s| s.id == id).cloned().collect(),
            None => generated.clone(),
        }
    }

    /// Get API definition.
    pub async fn get_api(&self, id: &str) -> Option<ApiDefinition> {
        let apis = self.apis.read().await;
        apis.get(id).cloned()
    }

    /// List APIs.
    pub async fn list_apis(&self) -> Vec<ApiDefinition> {
        let apis = self.apis.read().await;
        apis.values().cloned().collect()
    }

    /// Supported languages.
    pub fn supported_languages(&self) -> Vec<Language> {
        self.generators.keys().copied().collect()
    }
}

impl Default for SdkGenEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for API definitions.
pub struct ApiBuilder {
    api: ApiDefinition,
}

impl ApiBuilder {
    /// Create a new API builder.
    pub fn new(name: &str, version: &str) -> Self {
        Self {
            api: ApiDefinition {
                id: Uuid::new_v4().to_string(),
                name: name.to_string(),
                version: version.to_string(),
                base_url: "http://localhost".to_string(),
                endpoints: Vec::new(),
                types: Vec::new(),
                auth: None,
                description: None,
            },
        }
    }

    /// Set base URL.
    pub fn base_url(mut self, url: &str) -> Self {
        self.api.base_url = url.to_string();
        self
    }

    /// Set description.
    pub fn description(mut self, desc: &str) -> Self {
        self.api.description = Some(desc.to_string());
        self
    }

    /// Add endpoint.
    pub fn endpoint(mut self, method: HttpMethod, path: &str, operation: &str) -> Self {
        self.api.endpoints.push(Endpoint {
            id: Uuid::new_v4().to_string(),
            path: path.to_string(),
            method,
            operation: operation.to_string(),
            description: None,
            request_body: None,
            response: None,
            path_params: Vec::new(),
            query_params: Vec::new(),
            headers: Vec::new(),
            tags: Vec::new(),
        });
        self
    }

    /// Add type.
    pub fn type_def(mut self, name: &str, kind: TypeKind) -> Self {
        self.api.types.push(TypeDefinition {
            name: name.to_string(),
            kind,
            fields: Vec::new(),
            variants: Vec::new(),
            description: None,
        });
        self
    }

    /// Set authentication.
    pub fn auth(mut self, auth_type: AuthType) -> Self {
        self.api.auth = Some(AuthDefinition {
            auth_type,
            header: if auth_type == AuthType::ApiKey {
                Some("X-API-Key".to_string())
            } else {
                None
            },
            token_url: None,
        });
        self
    }

    /// Build the API definition.
    pub fn build(self) -> ApiDefinition {
        self.api
    }
}

/// Built-in TypeScript generator.
pub struct TypeScriptGenerator;

#[async_trait]
impl SdkGenerator for TypeScriptGenerator {
    async fn generate(
        &self,
        api: &ApiDefinition,
        _language: Language,
        options: &GenerationOptions,
    ) -> Result<GeneratedSdk> {
        let mut files = Vec::new();

        // Generate types file
        let mut types_content = String::from("// Generated types\n\n");
        for type_def in &api.types {
            match type_def.kind {
                TypeKind::Object => {
                    types_content.push_str(&format!("export interface {} {{\n", type_def.name));
                    for field in &type_def.fields {
                        let optional = if field.required { "" } else { "?" };
                        types_content.push_str(&format!(
                            "  {}{}: {};\n",
                            field.name,
                            optional,
                            self.ts_type(&field.type_ref)
                        ));
                    }
                    types_content.push_str("}\n\n");
                }
                TypeKind::Enum => {
                    types_content.push_str(&format!("export enum {} {{\n", type_def.name));
                    for variant in &type_def.variants {
                        if let Some(value) = &variant.value {
                            types_content
                                .push_str(&format!("  {} = \"{}\",\n", variant.name, value));
                        } else {
                            types_content.push_str(&format!("  {},\n", variant.name));
                        }
                    }
                    types_content.push_str("}\n\n");
                }
                _ => {}
            }
        }

        files.push(GeneratedFile {
            path: "types.ts".to_string(),
            content: types_content,
            file_type: FileType::Source,
        });

        // Generate client file
        let mut client_content = String::from("// Generated API client\n\n");
        client_content.push_str("import * as types from './types';\n\n");
        client_content.push_str(&format!("export class {}Client {{\n", api.name));
        client_content.push_str("  private baseUrl: string;\n");
        client_content.push_str("  private apiKey?: string;\n\n");
        client_content.push_str(&format!(
            "  constructor(baseUrl: string = '{}', apiKey?: string) {{\n",
            api.base_url
        ));
        client_content.push_str("    this.baseUrl = baseUrl;\n");
        client_content.push_str("    this.apiKey = apiKey;\n");
        client_content.push_str("  }\n\n");

        for endpoint in &api.endpoints {
            let method = format!("{:?}", endpoint.method).to_lowercase();
            let return_type = endpoint
                .response
                .as_ref()
                .map(|r| self.ts_type(r))
                .unwrap_or_else(|| "void".to_string());

            client_content.push_str(&format!(
                "  async {}(): Promise<{}> {{\n",
                endpoint.operation, return_type
            ));
            client_content.push_str(&format!(
                "    const response = await fetch(`${{this.baseUrl}}{}`, {{\n",
                endpoint.path
            ));
            client_content.push_str(&format!("      method: '{}',\n", method.to_uppercase()));
            client_content.push_str("      headers: {\n");
            client_content.push_str("        'Content-Type': 'application/json',\n");
            client_content.push_str("        ...(this.apiKey && { 'X-API-Key': this.apiKey }),\n");
            client_content.push_str("      },\n");
            client_content.push_str("    });\n");
            client_content.push_str("    return response.json();\n");
            client_content.push_str("  }\n\n");
        }

        client_content.push_str("}\n");

        files.push(GeneratedFile {
            path: "client.ts".to_string(),
            content: client_content,
            file_type: FileType::Source,
        });

        // Generate package.json
        let package_json = format!(
            r#"{{
  "name": "{}",
  "version": "{}",
  "main": "dist/client.js",
  "types": "dist/client.d.ts",
  "scripts": {{
    "build": "tsc",
    "test": "jest"
  }}
}}
"#,
            options.package_name, options.version
        );

        files.push(GeneratedFile {
            path: "package.json".to_string(),
            content: package_json,
            file_type: FileType::Config,
        });

        if options.include_docs {
            let readme = format!("# {} SDK\n\nGenerated TypeScript client for {}.\n\n## Usage\n\n```typescript\nimport {{ {}Client }} from '{}';\n\nconst client = new {}Client();\n```\n",
                api.name, api.name, api.name, options.package_name, api.name);

            files.push(GeneratedFile {
                path: "README.md".to_string(),
                content: readme,
                file_type: FileType::Documentation,
            });
        }

        Ok(GeneratedSdk {
            id: Uuid::new_v4().to_string(),
            api_name: api.name.clone(),
            language: Language::TypeScript,
            files,
            package_name: options.package_name.clone(),
            version: options.version.clone(),
            generated_at: Utc::now(),
        })
    }

    fn supported_languages(&self) -> Vec<Language> {
        vec![Language::TypeScript]
    }
}

impl TypeScriptGenerator {
    fn ts_type(&self, type_ref: &TypeRef) -> String {
        let base = match type_ref.name.as_str() {
            "string" => "string",
            "integer" | "number" => "number",
            "boolean" => "boolean",
            "object" => "Record<string, any>",
            other => other,
        };

        let result = if type_ref.is_array {
            format!("{}[]", base)
        } else {
            base.to_string()
        };

        if type_ref.is_optional {
            format!("{} | null", result)
        } else {
            result
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_api_builder() {
        let api = ApiBuilder::new("MyAPI", "1.0.0")
            .base_url("https://api.example.com")
            .description("Test API")
            .endpoint(HttpMethod::Get, "/users", "getUsers")
            .endpoint(HttpMethod::Post, "/users", "createUser")
            .auth(AuthType::ApiKey)
            .build();

        assert_eq!(api.name, "MyAPI");
        assert_eq!(api.endpoints.len(), 2);
        assert!(api.auth.is_some());
    }

    #[tokio::test]
    async fn test_typescript_generator() {
        let mut engine = SdkGenEngine::new();
        engine.register_generator(Language::TypeScript, Arc::new(TypeScriptGenerator));

        let api = ApiBuilder::new("TestAPI", "1.0.0")
            .endpoint(HttpMethod::Get, "/items", "getItems")
            .build();

        let api_id = engine.register_api(api).await;

        let sdk = engine
            .generate(&api_id, Language::TypeScript, None)
            .await
            .unwrap();

        assert_eq!(sdk.language, Language::TypeScript);
        assert!(!sdk.files.is_empty());

        // Check client file was generated
        let client_file = sdk.files.iter().find(|f| f.path == "client.ts").unwrap();
        assert!(client_file.content.contains("getItems"));
    }

    #[tokio::test]
    async fn test_parse_openapi() {
        let engine = SdkGenEngine::new();

        let spec = r#"{
            "openapi": "3.0.0",
            "info": {
                "title": "Pet Store",
                "version": "1.0.0"
            },
            "servers": [
                { "url": "https://petstore.example.com" }
            ]
        }"#;

        let api = engine.parse_openapi(spec).await.unwrap();
        assert_eq!(api.name, "Pet Store");
        assert_eq!(api.base_url, "https://petstore.example.com");
    }

    #[tokio::test]
    async fn test_supported_languages() {
        let mut engine = SdkGenEngine::new();
        engine.register_generator(Language::TypeScript, Arc::new(TypeScriptGenerator));

        let languages = engine.supported_languages();
        assert!(languages.contains(&Language::TypeScript));
    }
}
