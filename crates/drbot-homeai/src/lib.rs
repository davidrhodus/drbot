//! Smart home integration for drbot.
//!
//! AI-powered home automation.
//!
//! # Features
//!
//! - HomeKit/Matter support
//! - Voice commands
//! - Automation routines
//! - Scene management
//! - Energy optimization

use async_trait::async_trait;
use chrono::{DateTime, NaiveTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

/// Home AI result type.
pub type Result<T> = std::result::Result<T, HomeError>;

/// Home AI errors.
#[derive(Debug, thiserror::Error)]
pub enum HomeError {
    #[error("Device not found: {0}")]
    DeviceNotFound(String),
    #[error("Command failed: {0}")]
    CommandFailed(String),
    #[error("Scene not found: {0}")]
    SceneNotFound(String),
    #[error("Automation failed: {0}")]
    AutomationFailed(String),
}

/// Device type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceType {
    Light,
    Switch,
    Thermostat,
    Lock,
    Camera,
    Sensor,
    Speaker,
    Tv,
    Fan,
    Blind,
    Garage,
    Doorbell,
    Vacuum,
    Other,
}

/// Device state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceState {
    /// Is on/active.
    pub on: Option<bool>,
    /// Brightness (0-100).
    pub brightness: Option<u8>,
    /// Color (hex).
    pub color: Option<String>,
    /// Temperature (Celsius).
    pub temperature: Option<f32>,
    /// Target temperature.
    pub target_temp: Option<f32>,
    /// Humidity.
    pub humidity: Option<f32>,
    /// Battery level.
    pub battery: Option<u8>,
    /// Is locked.
    pub locked: Option<bool>,
    /// Volume (0-100).
    pub volume: Option<u8>,
    /// Custom properties.
    pub custom: HashMap<String, serde_json::Value>,
}

impl Default for DeviceState {
    fn default() -> Self {
        Self {
            on: None,
            brightness: None,
            color: None,
            temperature: None,
            target_temp: None,
            humidity: None,
            battery: None,
            locked: None,
            volume: None,
            custom: HashMap::new(),
        }
    }
}

/// Smart home device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    /// Device ID.
    pub id: Uuid,
    /// Device name.
    pub name: String,
    /// Device type.
    pub device_type: DeviceType,
    /// Room.
    pub room: Option<String>,
    /// Manufacturer.
    pub manufacturer: Option<String>,
    /// Model.
    pub model: Option<String>,
    /// Current state.
    pub state: DeviceState,
    /// Is reachable.
    pub reachable: bool,
    /// Last updated.
    pub last_updated: DateTime<Utc>,
}

impl Device {
    /// Create a new device.
    pub fn new(name: &str, device_type: DeviceType) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.to_string(),
            device_type,
            room: None,
            manufacturer: None,
            model: None,
            state: DeviceState::default(),
            reachable: true,
            last_updated: Utc::now(),
        }
    }

    /// Set room.
    pub fn in_room(mut self, room: &str) -> Self {
        self.room = Some(room.to_string());
        self
    }
}

/// Device command.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCommand {
    /// Target device.
    pub device_id: Uuid,
    /// Command type.
    pub command: CommandType,
    /// Value.
    pub value: Option<serde_json::Value>,
}

/// Command types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandType {
    TurnOn,
    TurnOff,
    Toggle,
    SetBrightness,
    SetColor,
    SetTemperature,
    Lock,
    Unlock,
    SetVolume,
    Custom,
}

/// Scene.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scene {
    /// Scene ID.
    pub id: Uuid,
    /// Scene name.
    pub name: String,
    /// Icon.
    pub icon: Option<String>,
    /// Actions.
    pub actions: Vec<DeviceCommand>,
    /// Created at.
    pub created_at: DateTime<Utc>,
}

impl Scene {
    /// Create a new scene.
    pub fn new(name: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.to_string(),
            icon: None,
            actions: Vec::new(),
            created_at: Utc::now(),
        }
    }

    /// Add action.
    pub fn with_action(mut self, command: DeviceCommand) -> Self {
        self.actions.push(command);
        self
    }
}

/// Automation rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Automation {
    /// Automation ID.
    pub id: Uuid,
    /// Name.
    pub name: String,
    /// Triggers.
    pub triggers: Vec<Trigger>,
    /// Conditions.
    pub conditions: Vec<Condition>,
    /// Actions.
    pub actions: Vec<DeviceCommand>,
    /// Is enabled.
    pub enabled: bool,
    /// Last triggered.
    pub last_triggered: Option<DateTime<Utc>>,
}

/// Trigger types.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Trigger {
    /// Time trigger.
    Time { time: NaiveTime, days: Vec<u8> },
    /// Device state trigger.
    DeviceState {
        device_id: Uuid,
        property: String,
        value: serde_json::Value,
    },
    /// Sunrise/sunset.
    Sun {
        event: SunEvent,
        offset_minutes: i32,
    },
    /// Voice command.
    Voice { phrase: String },
}

/// Sun events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SunEvent {
    Sunrise,
    Sunset,
}

/// Condition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Condition {
    /// Time range.
    TimeRange { start: NaiveTime, end: NaiveTime },
    /// Device state.
    DeviceState {
        device_id: Uuid,
        property: String,
        operator: String,
        value: serde_json::Value,
    },
    /// Day of week.
    DayOfWeek { days: Vec<u8> },
}

/// Home event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HomeEvent {
    /// Device state changed.
    DeviceChanged {
        device_id: Uuid,
        old_state: DeviceState,
        new_state: DeviceState,
    },
    /// Scene activated.
    SceneActivated { scene_id: Uuid },
    /// Automation triggered.
    AutomationTriggered { automation_id: Uuid },
    /// Device added.
    DeviceAdded(Device),
    /// Device removed.
    DeviceRemoved(Uuid),
}

/// Home AI configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HomeConfig {
    /// Enable voice commands.
    pub voice_enabled: bool,
    /// Enable energy optimization.
    pub energy_optimization: bool,
    /// Home latitude.
    pub latitude: Option<f64>,
    /// Home longitude.
    pub longitude: Option<f64>,
}

impl Default for HomeConfig {
    fn default() -> Self {
        Self {
            voice_enabled: true,
            energy_optimization: true,
            latitude: None,
            longitude: None,
        }
    }
}

/// Trait for device controllers.
#[async_trait]
pub trait DeviceController: Send + Sync {
    /// Execute command.
    async fn execute(&self, device: &Device, command: &DeviceCommand) -> Result<DeviceState>;
    /// Get device state.
    async fn get_state(&self, device: &Device) -> Result<DeviceState>;
}

/// Trait for voice interpreters.
#[async_trait]
pub trait VoiceInterpreter: Send + Sync {
    /// Parse voice command.
    async fn parse(&self, text: &str, devices: &[Device]) -> Result<Vec<DeviceCommand>>;
}

/// Home AI engine.
pub struct HomeAIEngine<C: DeviceController, V: VoiceInterpreter> {
    config: HomeConfig,
    controller: C,
    voice: V,
    devices: Arc<RwLock<HashMap<Uuid, Device>>>,
    scenes: Arc<RwLock<HashMap<Uuid, Scene>>>,
    automations: Arc<RwLock<HashMap<Uuid, Automation>>>,
    event_tx: broadcast::Sender<HomeEvent>,
}

impl<C: DeviceController, V: VoiceInterpreter> HomeAIEngine<C, V> {
    /// Create a new home AI engine.
    pub fn new(config: HomeConfig, controller: C, voice: V) -> Self {
        let (event_tx, _) = broadcast::channel(100);

        Self {
            config,
            controller,
            voice,
            devices: Arc::new(RwLock::new(HashMap::new())),
            scenes: Arc::new(RwLock::new(HashMap::new())),
            automations: Arc::new(RwLock::new(HashMap::new())),
            event_tx,
        }
    }

    /// Add device.
    pub async fn add_device(&self, device: Device) -> Uuid {
        let id = device.id;
        self.devices.write().await.insert(id, device.clone());
        let _ = self.event_tx.send(HomeEvent::DeviceAdded(device));
        id
    }

    /// Get device.
    pub async fn get_device(&self, id: Uuid) -> Option<Device> {
        self.devices.read().await.get(&id).cloned()
    }

    /// List devices.
    pub async fn list_devices(&self) -> Vec<Device> {
        self.devices.read().await.values().cloned().collect()
    }

    /// List devices by room.
    pub async fn devices_in_room(&self, room: &str) -> Vec<Device> {
        self.devices
            .read()
            .await
            .values()
            .filter(|d| d.room.as_deref() == Some(room))
            .cloned()
            .collect()
    }

    /// Execute command.
    pub async fn execute(&self, command: DeviceCommand) -> Result<DeviceState> {
        let mut devices = self.devices.write().await;
        let device = devices
            .get_mut(&command.device_id)
            .ok_or(HomeError::DeviceNotFound(command.device_id.to_string()))?;

        let old_state = device.state.clone();
        let new_state = self.controller.execute(device, &command).await?;

        device.state = new_state.clone();
        device.last_updated = Utc::now();

        let _ = self.event_tx.send(HomeEvent::DeviceChanged {
            device_id: command.device_id,
            old_state,
            new_state: new_state.clone(),
        });

        Ok(new_state)
    }

    /// Process voice command.
    pub async fn voice_command(&self, text: &str) -> Result<Vec<DeviceState>> {
        if !self.config.voice_enabled {
            return Err(HomeError::CommandFailed("Voice disabled".to_string()));
        }

        let devices: Vec<_> = self.devices.read().await.values().cloned().collect();
        let commands = self.voice.parse(text, &devices).await?;

        let mut results = Vec::new();
        for command in commands {
            results.push(self.execute(command).await?);
        }

        Ok(results)
    }

    /// Create scene.
    pub async fn create_scene(&self, scene: Scene) -> Uuid {
        let id = scene.id;
        self.scenes.write().await.insert(id, scene);
        id
    }

    /// Activate scene.
    pub async fn activate_scene(&self, scene_id: Uuid) -> Result<()> {
        let scene = self
            .scenes
            .read()
            .await
            .get(&scene_id)
            .cloned()
            .ok_or(HomeError::SceneNotFound(scene_id.to_string()))?;

        for action in scene.actions {
            self.execute(action).await?;
        }

        let _ = self.event_tx.send(HomeEvent::SceneActivated { scene_id });
        Ok(())
    }

    /// List scenes.
    pub async fn list_scenes(&self) -> Vec<Scene> {
        self.scenes.read().await.values().cloned().collect()
    }

    /// Create automation.
    pub async fn create_automation(&self, automation: Automation) -> Uuid {
        let id = automation.id;
        self.automations.write().await.insert(id, automation);
        id
    }

    /// Subscribe to events.
    pub fn subscribe(&self) -> broadcast::Receiver<HomeEvent> {
        self.event_tx.subscribe()
    }

    /// Get rooms.
    pub async fn rooms(&self) -> Vec<String> {
        let mut rooms: Vec<_> = self
            .devices
            .read()
            .await
            .values()
            .filter_map(|d| d.room.clone())
            .collect();
        rooms.sort();
        rooms.dedup();
        rooms
    }

    /// Get statistics.
    pub async fn stats(&self) -> HomeStats {
        let devices = self.devices.read().await;
        let scenes = self.scenes.read().await;
        let automations = self.automations.read().await;

        let on_count = devices
            .values()
            .filter(|d| d.state.on == Some(true))
            .count();

        let mut by_type: HashMap<DeviceType, usize> = HashMap::new();
        for device in devices.values() {
            *by_type.entry(device.device_type).or_insert(0) += 1;
        }

        HomeStats {
            total_devices: devices.len(),
            devices_on: on_count,
            total_scenes: scenes.len(),
            total_automations: automations.len(),
            by_type,
        }
    }
}

/// Home statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HomeStats {
    pub total_devices: usize,
    pub devices_on: usize,
    pub total_scenes: usize,
    pub total_automations: usize,
    pub by_type: HashMap<DeviceType, usize>,
}

/// Simple device controller for testing.
pub struct SimpleController;

#[async_trait]
impl DeviceController for SimpleController {
    async fn execute(&self, device: &Device, command: &DeviceCommand) -> Result<DeviceState> {
        let mut state = device.state.clone();

        match command.command {
            CommandType::TurnOn => state.on = Some(true),
            CommandType::TurnOff => state.on = Some(false),
            CommandType::Toggle => state.on = Some(!state.on.unwrap_or(false)),
            CommandType::SetBrightness => {
                if let Some(v) = &command.value {
                    state.brightness = v.as_u64().map(|n| n as u8);
                }
            }
            CommandType::SetTemperature => {
                if let Some(v) = &command.value {
                    state.target_temp = v.as_f64().map(|n| n as f32);
                }
            }
            CommandType::Lock => state.locked = Some(true),
            CommandType::Unlock => state.locked = Some(false),
            _ => {}
        }

        Ok(state)
    }

    async fn get_state(&self, device: &Device) -> Result<DeviceState> {
        Ok(device.state.clone())
    }
}

/// Simple voice interpreter for testing.
pub struct SimpleVoice;

#[async_trait]
impl VoiceInterpreter for SimpleVoice {
    async fn parse(&self, text: &str, devices: &[Device]) -> Result<Vec<DeviceCommand>> {
        let text_lower = text.to_lowercase();
        let mut commands = Vec::new();

        for device in devices {
            if text_lower.contains(&device.name.to_lowercase()) {
                let command_type =
                    if text_lower.contains("turn on") || text_lower.contains("switch on") {
                        Some(CommandType::TurnOn)
                    } else if text_lower.contains("turn off") || text_lower.contains("switch off") {
                        Some(CommandType::TurnOff)
                    } else {
                        None
                    };

                if let Some(ct) = command_type {
                    commands.push(DeviceCommand {
                        device_id: device.id,
                        command: ct,
                        value: None,
                    });
                }
            }
        }

        Ok(commands)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_add_device() {
        let engine = HomeAIEngine::new(HomeConfig::default(), SimpleController, SimpleVoice);
        let device = Device::new("Living Room Light", DeviceType::Light).in_room("Living Room");

        let id = engine.add_device(device).await;
        let retrieved = engine.get_device(id).await.unwrap();
        assert_eq!(retrieved.name, "Living Room Light");
    }

    #[tokio::test]
    async fn test_execute_command() {
        let engine = HomeAIEngine::new(HomeConfig::default(), SimpleController, SimpleVoice);
        let device = Device::new("Lamp", DeviceType::Light);
        let id = engine.add_device(device).await;

        let state = engine
            .execute(DeviceCommand {
                device_id: id,
                command: CommandType::TurnOn,
                value: None,
            })
            .await
            .unwrap();

        assert_eq!(state.on, Some(true));
    }

    #[tokio::test]
    async fn test_voice_command() {
        let engine = HomeAIEngine::new(HomeConfig::default(), SimpleController, SimpleVoice);
        let device = Device::new("Kitchen Light", DeviceType::Light);
        engine.add_device(device).await;

        let results = engine.voice_command("turn on kitchen light").await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].on, Some(true));
    }

    #[tokio::test]
    async fn test_scene() {
        let engine = HomeAIEngine::new(HomeConfig::default(), SimpleController, SimpleVoice);

        let light1 = engine
            .add_device(Device::new("Light 1", DeviceType::Light))
            .await;
        let light2 = engine
            .add_device(Device::new("Light 2", DeviceType::Light))
            .await;

        let scene = Scene::new("Movie Time")
            .with_action(DeviceCommand {
                device_id: light1,
                command: CommandType::TurnOff,
                value: None,
            })
            .with_action(DeviceCommand {
                device_id: light2,
                command: CommandType::TurnOff,
                value: None,
            });

        let scene_id = engine.create_scene(scene).await;
        engine.activate_scene(scene_id).await.unwrap();

        let d1 = engine.get_device(light1).await.unwrap();
        let d2 = engine.get_device(light2).await.unwrap();
        assert_eq!(d1.state.on, Some(false));
        assert_eq!(d2.state.on, Some(false));
    }
}
