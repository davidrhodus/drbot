//! Screenshot analysis, UI understanding, visual context.
//!
//! This crate provides:
//! - Screenshot capture and analysis
//! - UI element detection
//! - Visual context extraction
//! - Image understanding

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Vision errors.
#[derive(Debug, Error)]
pub enum VisionError {
    #[error("Capture failed: {0}")]
    CaptureFailed(String),

    #[error("Analysis failed: {0}")]
    AnalysisFailed(String),

    #[error("Invalid image: {0}")]
    InvalidImage(String),

    #[error("Provider error: {0}")]
    ProviderError(String),
}

/// Result type for vision operations.
pub type Result<T> = std::result::Result<T, VisionError>;

/// An image for analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Image {
    /// Image identifier.
    pub id: String,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Image format.
    pub format: ImageFormat,
    /// Base64 encoded data.
    pub data: String,
    /// Capture timestamp.
    pub captured_at: DateTime<Utc>,
    /// Source description.
    pub source: String,
}

/// Image formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImageFormat {
    Png,
    Jpeg,
    WebP,
    Gif,
}

/// A detected UI element.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UIElement {
    /// Element identifier.
    pub id: String,
    /// Element type.
    pub element_type: UIElementType,
    /// Bounding box.
    pub bounds: BoundingBox,
    /// Detected text.
    pub text: Option<String>,
    /// Confidence.
    pub confidence: f64,
    /// Child elements.
    pub children: Vec<UIElement>,
    /// Attributes.
    pub attributes: HashMap<String, String>,
}

/// Types of UI elements.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UIElementType {
    Button,
    TextField,
    Label,
    Image,
    Icon,
    Menu,
    MenuItem,
    Tab,
    Window,
    Dialog,
    List,
    ListItem,
    Table,
    Link,
    Checkbox,
    RadioButton,
    Slider,
    ScrollBar,
    Container,
    Unknown,
}

/// Bounding box for an element.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct BoundingBox {
    /// X coordinate.
    pub x: i32,
    /// Y coordinate.
    pub y: i32,
    /// Width.
    pub width: u32,
    /// Height.
    pub height: u32,
}

impl BoundingBox {
    /// Check if a point is inside the box.
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x
            && x < self.x + self.width as i32
            && y >= self.y
            && y < self.y + self.height as i32
    }

    /// Get center point.
    pub fn center(&self) -> (i32, i32) {
        (
            self.x + self.width as i32 / 2,
            self.y + self.height as i32 / 2,
        )
    }
}

/// Visual context extracted from an image.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisualContext {
    /// Context identifier.
    pub id: String,
    /// Source image.
    pub image_id: String,
    /// Detected application.
    pub application: Option<String>,
    /// Screen region.
    pub region: ScreenRegion,
    /// Detected UI elements.
    pub elements: Vec<UIElement>,
    /// Extracted text (OCR).
    pub text_content: String,
    /// Detected colors.
    pub dominant_colors: Vec<String>,
    /// Scene description.
    pub description: String,
    /// Actionable items.
    pub actionable_items: Vec<ActionableItem>,
}

/// Screen regions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScreenRegion {
    FullScreen,
    Window,
    Selection,
    Menu,
    Dialog,
}

/// An actionable item detected in the UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionableItem {
    /// Item identifier.
    pub id: String,
    /// Associated UI element.
    pub element_id: String,
    /// Action type.
    pub action_type: ActionType,
    /// Action description.
    pub description: String,
    /// Confidence.
    pub confidence: f64,
}

/// Types of actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActionType {
    Click,
    Type,
    Select,
    Scroll,
    Drag,
    RightClick,
    DoubleClick,
}

/// Analysis request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisRequest {
    /// Image to analyze.
    pub image: Image,
    /// Analysis options.
    pub options: AnalysisOptions,
}

/// Analysis options.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AnalysisOptions {
    /// Detect UI elements.
    pub detect_ui: bool,
    /// Perform OCR.
    pub ocr: bool,
    /// Generate description.
    pub describe: bool,
    /// Find actionable items.
    pub find_actions: bool,
    /// Language for OCR.
    pub language: Option<String>,
}

/// Provider for vision analysis.
#[async_trait]
pub trait VisionProvider: Send + Sync {
    /// Analyze an image.
    async fn analyze(&self, request: &AnalysisRequest) -> Result<VisualContext>;

    /// Detect UI elements.
    async fn detect_ui(&self, image: &Image) -> Result<Vec<UIElement>>;

    /// Perform OCR.
    async fn ocr(&self, image: &Image) -> Result<String>;

    /// Describe an image.
    async fn describe(&self, image: &Image) -> Result<String>;

    /// Find element at position.
    async fn element_at(&self, image: &Image, x: i32, y: i32) -> Result<Option<UIElement>>;
}

/// The vision engine.
pub struct VisionEngine {
    /// Vision provider.
    provider: Arc<dyn VisionProvider>,
    /// Analysis cache.
    cache: Arc<RwLock<HashMap<String, VisualContext>>>,
    /// History.
    history: Arc<RwLock<Vec<VisualContext>>>,
}

impl VisionEngine {
    /// Create a new vision engine.
    pub fn new(provider: Arc<dyn VisionProvider>) -> Self {
        Self {
            provider,
            cache: Arc::new(RwLock::new(HashMap::new())),
            history: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Analyze an image.
    pub async fn analyze(&self, image: Image, options: AnalysisOptions) -> Result<VisualContext> {
        // Check cache
        {
            let cache = self.cache.read().await;
            if let Some(ctx) = cache.get(&image.id) {
                return Ok(ctx.clone());
            }
        }

        let request = AnalysisRequest {
            image: image.clone(),
            options,
        };
        let context = self.provider.analyze(&request).await?;

        // Cache result
        let mut cache = self.cache.write().await;
        cache.insert(image.id, context.clone());

        // Add to history
        let mut history = self.history.write().await;
        history.push(context.clone());
        if history.len() > 100 {
            history.drain(0..10);
        }

        Ok(context)
    }

    /// Quick OCR on an image.
    pub async fn ocr(&self, image: Image) -> Result<String> {
        self.provider.ocr(&image).await
    }

    /// Describe an image.
    pub async fn describe(&self, image: Image) -> Result<String> {
        self.provider.describe(&image).await
    }

    /// Find UI element at position.
    pub async fn element_at(&self, image: Image, x: i32, y: i32) -> Result<Option<UIElement>> {
        self.provider.element_at(&image, x, y).await
    }

    /// Get clickable elements.
    pub async fn get_clickable(&self, image: Image) -> Result<Vec<UIElement>> {
        let elements = self.provider.detect_ui(&image).await?;
        Ok(elements
            .into_iter()
            .filter(|e| {
                matches!(
                    e.element_type,
                    UIElementType::Button
                        | UIElementType::Link
                        | UIElementType::MenuItem
                        | UIElementType::Tab
                        | UIElementType::Checkbox
                        | UIElementType::RadioButton
                )
            })
            .collect())
    }

    /// Get text input fields.
    pub async fn get_text_fields(&self, image: Image) -> Result<Vec<UIElement>> {
        let elements = self.provider.detect_ui(&image).await?;
        Ok(elements
            .into_iter()
            .filter(|e| e.element_type == UIElementType::TextField)
            .collect())
    }

    /// Get analysis history.
    pub async fn get_history(&self, limit: usize) -> Vec<VisualContext> {
        let history = self.history.read().await;
        history.iter().rev().take(limit).cloned().collect()
    }

    /// Clear cache.
    pub async fn clear_cache(&self) {
        let mut cache = self.cache.write().await;
        cache.clear();
    }
}

/// Builder for creating images.
pub struct ImageBuilder {
    image: Image,
}

impl ImageBuilder {
    /// Create a new image builder.
    pub fn new(data: String, format: ImageFormat) -> Self {
        Self {
            image: Image {
                id: Uuid::new_v4().to_string(),
                width: 0,
                height: 0,
                format,
                data,
                captured_at: Utc::now(),
                source: String::new(),
            },
        }
    }

    /// Set dimensions.
    pub fn dimensions(mut self, width: u32, height: u32) -> Self {
        self.image.width = width;
        self.image.height = height;
        self
    }

    /// Set source.
    pub fn source(mut self, source: &str) -> Self {
        self.image.source = source.to_string();
        self
    }

    /// Build the image.
    pub fn build(self) -> Image {
        self.image
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockProvider;

    #[async_trait]
    impl VisionProvider for MockProvider {
        async fn analyze(&self, request: &AnalysisRequest) -> Result<VisualContext> {
            Ok(VisualContext {
                id: Uuid::new_v4().to_string(),
                image_id: request.image.id.clone(),
                application: Some("TestApp".to_string()),
                region: ScreenRegion::Window,
                elements: vec![UIElement {
                    id: "btn1".to_string(),
                    element_type: UIElementType::Button,
                    bounds: BoundingBox {
                        x: 10,
                        y: 10,
                        width: 100,
                        height: 30,
                    },
                    text: Some("Click Me".to_string()),
                    confidence: 0.95,
                    children: vec![],
                    attributes: HashMap::new(),
                }],
                text_content: "Sample text content".to_string(),
                dominant_colors: vec!["#FFFFFF".to_string(), "#000000".to_string()],
                description: "A window with a button".to_string(),
                actionable_items: vec![ActionableItem {
                    id: "action1".to_string(),
                    element_id: "btn1".to_string(),
                    action_type: ActionType::Click,
                    description: "Click the button".to_string(),
                    confidence: 0.9,
                }],
            })
        }

        async fn detect_ui(&self, _image: &Image) -> Result<Vec<UIElement>> {
            Ok(vec![
                UIElement {
                    id: "btn1".to_string(),
                    element_type: UIElementType::Button,
                    bounds: BoundingBox {
                        x: 10,
                        y: 10,
                        width: 100,
                        height: 30,
                    },
                    text: Some("Button".to_string()),
                    confidence: 0.95,
                    children: vec![],
                    attributes: HashMap::new(),
                },
                UIElement {
                    id: "txt1".to_string(),
                    element_type: UIElementType::TextField,
                    bounds: BoundingBox {
                        x: 10,
                        y: 50,
                        width: 200,
                        height: 25,
                    },
                    text: None,
                    confidence: 0.9,
                    children: vec![],
                    attributes: HashMap::new(),
                },
            ])
        }

        async fn ocr(&self, _image: &Image) -> Result<String> {
            Ok("Extracted text from image".to_string())
        }

        async fn describe(&self, _image: &Image) -> Result<String> {
            Ok("A screenshot showing a user interface".to_string())
        }

        async fn element_at(&self, _image: &Image, x: i32, y: i32) -> Result<Option<UIElement>> {
            if x >= 10 && x < 110 && y >= 10 && y < 40 {
                Ok(Some(UIElement {
                    id: "btn1".to_string(),
                    element_type: UIElementType::Button,
                    bounds: BoundingBox {
                        x: 10,
                        y: 10,
                        width: 100,
                        height: 30,
                    },
                    text: Some("Button".to_string()),
                    confidence: 0.95,
                    children: vec![],
                    attributes: HashMap::new(),
                }))
            } else {
                Ok(None)
            }
        }
    }

    fn create_test_image() -> Image {
        ImageBuilder::new("base64data".to_string(), ImageFormat::Png)
            .dimensions(800, 600)
            .source("test")
            .build()
    }

    #[tokio::test]
    async fn test_analyze() {
        let provider = Arc::new(MockProvider);
        let engine = VisionEngine::new(provider);

        let image = create_test_image();
        let context = engine
            .analyze(image, AnalysisOptions::default())
            .await
            .unwrap();

        assert!(!context.elements.is_empty());
        assert!(context.application.is_some());
    }

    #[tokio::test]
    async fn test_ocr() {
        let provider = Arc::new(MockProvider);
        let engine = VisionEngine::new(provider);

        let image = create_test_image();
        let text = engine.ocr(image).await.unwrap();

        assert!(!text.is_empty());
    }

    #[tokio::test]
    async fn test_get_clickable() {
        let provider = Arc::new(MockProvider);
        let engine = VisionEngine::new(provider);

        let image = create_test_image();
        let clickable = engine.get_clickable(image).await.unwrap();

        assert_eq!(clickable.len(), 1);
        assert_eq!(clickable[0].element_type, UIElementType::Button);
    }

    #[tokio::test]
    async fn test_get_text_fields() {
        let provider = Arc::new(MockProvider);
        let engine = VisionEngine::new(provider);

        let image = create_test_image();
        let fields = engine.get_text_fields(image).await.unwrap();

        assert_eq!(fields.len(), 1);
    }

    #[tokio::test]
    async fn test_bounding_box() {
        let bbox = BoundingBox {
            x: 10,
            y: 10,
            width: 100,
            height: 50,
        };

        assert!(bbox.contains(50, 30));
        assert!(!bbox.contains(0, 0));

        let (cx, cy) = bbox.center();
        assert_eq!(cx, 60);
        assert_eq!(cy, 35);
    }
}
