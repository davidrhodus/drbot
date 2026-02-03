//! Canvas event handling.

use crate::{CanvasError, Result};
use drbot_canvas_protocol::{CanvasEvent, CanvasId, ComponentId};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};

/// Event handler function type.
pub type EventHandler = Box<dyn Fn(CanvasEvent) + Send + Sync>;

/// Event dispatcher for canvas events.
pub struct EventDispatcher {
    /// Broadcast sender for events.
    sender: broadcast::Sender<CanvasEvent>,
    /// Per-canvas handlers.
    handlers: Arc<RwLock<HashMap<CanvasId, Vec<EventHandler>>>>,
    /// Per-component handlers.
    component_handlers: Arc<RwLock<HashMap<(CanvasId, ComponentId), Vec<EventHandler>>>>,
}

impl EventDispatcher {
    /// Create a new event dispatcher.
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(1024);
        Self {
            sender,
            handlers: Arc::new(RwLock::new(HashMap::new())),
            component_handlers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Subscribe to all events.
    pub fn subscribe(&self) -> broadcast::Receiver<CanvasEvent> {
        self.sender.subscribe()
    }

    /// Dispatch an event.
    pub async fn dispatch(&self, event: CanvasEvent) {
        // Send to broadcast channel
        let _ = self.sender.send(event.clone());

        // Call canvas-level handlers
        if let Some(canvas_id) = Self::get_canvas_id(&event) {
            let handlers = self.handlers.read().await;
            if let Some(handlers) = handlers.get(&canvas_id) {
                for handler in handlers {
                    handler(event.clone());
                }
            }
        }

        // Call component-level handlers
        if let Some((canvas_id, component_id)) = Self::get_component_id(&event) {
            let handlers = self.component_handlers.read().await;
            let key = (canvas_id, component_id);
            if let Some(handlers) = handlers.get(&key) {
                for handler in handlers {
                    handler(event.clone());
                }
            }
        }
    }

    /// Register a handler for a specific canvas.
    pub async fn on_canvas(&self, canvas_id: CanvasId, handler: EventHandler) {
        let mut handlers = self.handlers.write().await;
        handlers.entry(canvas_id).or_default().push(handler);
    }

    /// Register a handler for a specific component.
    pub async fn on_component(
        &self,
        canvas_id: CanvasId,
        component_id: ComponentId,
        handler: EventHandler,
    ) {
        let mut handlers = self.component_handlers.write().await;
        handlers
            .entry((canvas_id, component_id))
            .or_default()
            .push(handler);
    }

    /// Remove all handlers for a canvas.
    pub async fn remove_canvas_handlers(&self, canvas_id: &CanvasId) {
        self.handlers.write().await.remove(canvas_id);

        // Also remove component handlers for this canvas
        let mut component_handlers = self.component_handlers.write().await;
        component_handlers.retain(|(id, _), _| id != canvas_id);
    }

    fn get_canvas_id(event: &CanvasEvent) -> Option<CanvasId> {
        match event {
            CanvasEvent::Created { canvas_id, .. } => Some(*canvas_id),
            CanvasEvent::Updated { canvas_id } => Some(*canvas_id),
            CanvasEvent::Destroyed { canvas_id } => Some(*canvas_id),
            CanvasEvent::ComponentEvent { canvas_id, .. } => Some(*canvas_id),
            CanvasEvent::FormSubmit { canvas_id, .. } => Some(*canvas_id),
            CanvasEvent::Error { canvas_id, .. } => *canvas_id,
        }
    }

    fn get_component_id(event: &CanvasEvent) -> Option<(CanvasId, ComponentId)> {
        match event {
            CanvasEvent::ComponentEvent {
                canvas_id,
                component_id,
                ..
            } => Some((*canvas_id, component_id.clone())),
            CanvasEvent::FormSubmit {
                canvas_id, form_id, ..
            } => Some((*canvas_id, form_id.clone())),
            _ => None,
        }
    }
}

impl Default for EventDispatcher {
    fn default() -> Self {
        Self::new()
    }
}

/// Component event data.
#[derive(Debug, Clone)]
pub struct ComponentEventData {
    /// Canvas ID.
    pub canvas_id: CanvasId,
    /// Component ID.
    pub component_id: ComponentId,
    /// Event type (e.g., "click", "change").
    pub event_type: String,
    /// Event data.
    pub data: Value,
}

impl ComponentEventData {
    /// Create new event data.
    pub fn new(
        canvas_id: CanvasId,
        component_id: ComponentId,
        event_type: impl Into<String>,
        data: Value,
    ) -> Self {
        Self {
            canvas_id,
            component_id,
            event_type: event_type.into(),
            data,
        }
    }

    /// Convert to CanvasEvent.
    pub fn to_canvas_event(&self) -> CanvasEvent {
        CanvasEvent::ComponentEvent {
            canvas_id: self.canvas_id,
            component_id: self.component_id.clone(),
            event_type: self.event_type.clone(),
            data: self.data.clone(),
        }
    }
}

/// Event queue for buffering events.
pub struct EventQueue {
    events: Arc<RwLock<Vec<CanvasEvent>>>,
    max_size: usize,
}

impl EventQueue {
    /// Create a new event queue.
    pub fn new(max_size: usize) -> Self {
        Self {
            events: Arc::new(RwLock::new(Vec::new())),
            max_size,
        }
    }

    /// Push an event to the queue.
    pub async fn push(&self, event: CanvasEvent) -> Result<()> {
        let mut events = self.events.write().await;
        if events.len() >= self.max_size {
            return Err(CanvasError::InvalidOperation(
                "Event queue is full".to_string(),
            ));
        }
        events.push(event);
        Ok(())
    }

    /// Pop an event from the queue.
    pub async fn pop(&self) -> Option<CanvasEvent> {
        let mut events = self.events.write().await;
        if events.is_empty() {
            None
        } else {
            Some(events.remove(0))
        }
    }

    /// Drain all events.
    pub async fn drain(&self) -> Vec<CanvasEvent> {
        let mut events = self.events.write().await;
        std::mem::take(&mut *events)
    }

    /// Get queue length.
    pub async fn len(&self) -> usize {
        self.events.read().await.len()
    }

    /// Check if queue is empty.
    pub async fn is_empty(&self) -> bool {
        self.events.read().await.is_empty()
    }
}

impl Default for EventQueue {
    fn default() -> Self {
        Self::new(1000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_event_dispatcher() {
        let dispatcher = EventDispatcher::new();
        let mut receiver = dispatcher.subscribe();

        let event = CanvasEvent::Created {
            canvas_id: CanvasId::new(),
            name: "Test".to_string(),
        };

        dispatcher.dispatch(event.clone()).await;

        let received = receiver.try_recv();
        assert!(received.is_ok());
    }

    #[tokio::test]
    async fn test_event_queue() {
        let queue = EventQueue::new(10);

        let event = CanvasEvent::Created {
            canvas_id: CanvasId::new(),
            name: "Test".to_string(),
        };

        queue.push(event).await.unwrap();
        assert_eq!(queue.len().await, 1);

        let popped = queue.pop().await;
        assert!(popped.is_some());
        assert!(queue.is_empty().await);
    }
}
