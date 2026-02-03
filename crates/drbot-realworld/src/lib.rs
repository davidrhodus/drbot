//! Real-world integration for drbot
//!
//! Connects to IoT devices, location services, health data, and vehicles.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;

#[derive(Error, Debug)]
pub enum RealworldError {
    #[error("Device not found: {0}")]
    DeviceNotFound(String),
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),
    #[error("Permission denied: {0}")]
    PermissionDenied(String),
    #[error("Sensor error: {0}")]
    SensorError(String),
    #[error("Service unavailable: {0}")]
    ServiceUnavailable(String),
    #[error("Provider error: {0}")]
    ProviderError(String),
}

pub type Result<T> = std::result::Result<T, RealworldError>;

// ============================================================================
// IoT Integration
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IoTDevice {
    pub id: String,
    pub name: String,
    pub device_type: DeviceType,
    pub location: Option<String>,
    pub state: DeviceState,
    pub capabilities: Vec<DeviceCapability>,
    pub last_seen: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DeviceType {
    Light,
    Thermostat,
    Lock,
    Camera,
    Sensor,
    Switch,
    Speaker,
    Television,
    Appliance,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceState {
    pub online: bool,
    pub power: Option<bool>,
    pub brightness: Option<u8>,
    pub temperature: Option<f32>,
    pub humidity: Option<f32>,
    pub locked: Option<bool>,
    pub custom: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DeviceCapability {
    OnOff,
    Brightness,
    ColorTemperature,
    Color,
    Temperature,
    Lock,
    OpenClose,
    Volume,
    MediaControl,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCommand {
    pub device_id: String,
    pub action: DeviceAction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DeviceAction {
    TurnOn,
    TurnOff,
    SetBrightness(u8),
    SetTemperature(f32),
    Lock,
    Unlock,
    SetVolume(u8),
    Play,
    Pause,
    Custom {
        name: String,
        params: HashMap<String, serde_json::Value>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scene {
    pub id: String,
    pub name: String,
    pub commands: Vec<DeviceCommand>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Automation {
    pub id: String,
    pub name: String,
    pub trigger: AutomationTrigger,
    pub conditions: Vec<AutomationCondition>,
    pub actions: Vec<DeviceCommand>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AutomationTrigger {
    Time {
        cron: String,
    },
    DeviceState {
        device_id: String,
        state_key: String,
        value: serde_json::Value,
    },
    Location {
        event: LocationEvent,
    },
    Sunrise {
        offset_minutes: i32,
    },
    Sunset {
        offset_minutes: i32,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AutomationCondition {
    TimeRange {
        start: String,
        end: String,
    },
    DeviceState {
        device_id: String,
        state_key: String,
        value: serde_json::Value,
    },
    DayOfWeek {
        days: Vec<u8>,
    },
}

// ============================================================================
// Location Services
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Location {
    pub latitude: f64,
    pub longitude: f64,
    pub altitude: Option<f64>,
    pub accuracy: Option<f32>,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LocationEvent {
    Enter,
    Exit,
    Dwell,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Geofence {
    pub id: String,
    pub name: String,
    pub center: Location,
    pub radius_meters: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Place {
    pub id: String,
    pub name: String,
    pub address: Option<String>,
    pub location: Location,
    pub category: Option<String>,
    pub rating: Option<f32>,
    pub open_now: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Route {
    pub origin: Location,
    pub destination: Location,
    pub waypoints: Vec<Location>,
    pub distance_meters: u32,
    pub duration_seconds: u32,
    pub steps: Vec<RouteStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteStep {
    pub instruction: String,
    pub distance_meters: u32,
    pub duration_seconds: u32,
    pub start_location: Location,
    pub end_location: Location,
}

// ============================================================================
// Health Integration
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthData {
    pub user_id: String,
    pub metrics: Vec<HealthMetric>,
    pub period_start: u64,
    pub period_end: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthMetric {
    pub metric_type: HealthMetricType,
    pub value: f64,
    pub unit: String,
    pub timestamp: u64,
    pub source: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum HealthMetricType {
    Steps,
    HeartRate,
    HeartRateVariability,
    BloodOxygen,
    BloodPressureSystolic,
    BloodPressureDiastolic,
    SleepDuration,
    SleepQuality,
    ActiveCalories,
    RestingCalories,
    Distance,
    FlightsClimbed,
    Weight,
    BodyFat,
    WaterIntake,
    CaffeineIntake,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SleepAnalysis {
    pub total_duration_minutes: u32,
    pub time_asleep_minutes: u32,
    pub time_awake_minutes: u32,
    pub deep_sleep_minutes: u32,
    pub rem_sleep_minutes: u32,
    pub light_sleep_minutes: u32,
    pub efficiency: f32,
    pub start_time: u64,
    pub end_time: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workout {
    pub id: String,
    pub activity_type: String,
    pub start_time: u64,
    pub end_time: u64,
    pub duration_seconds: u32,
    pub calories: Option<u32>,
    pub distance_meters: Option<u32>,
    pub avg_heart_rate: Option<u32>,
    pub max_heart_rate: Option<u32>,
}

// ============================================================================
// Vehicle Integration
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vehicle {
    pub id: String,
    pub name: String,
    pub make: String,
    pub model: String,
    pub year: u16,
    pub vin: Option<String>,
    pub vehicle_type: VehicleType,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum VehicleType {
    Car,
    Truck,
    SUV,
    Motorcycle,
    ElectricVehicle,
    Hybrid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VehicleState {
    pub vehicle_id: String,
    pub location: Option<Location>,
    pub odometer_km: Option<u32>,
    pub fuel_level_percent: Option<u8>,
    pub battery_level_percent: Option<u8>,
    pub range_km: Option<u32>,
    pub locked: Option<bool>,
    pub doors_open: Vec<String>,
    pub windows_open: Vec<String>,
    pub climate_on: Option<bool>,
    pub interior_temp_celsius: Option<f32>,
    pub exterior_temp_celsius: Option<f32>,
    pub charging: Option<bool>,
    pub charging_complete_time: Option<u64>,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VehicleCommand {
    Lock,
    Unlock,
    StartClimate {
        target_temp: f32,
    },
    StopClimate,
    StartCharging,
    StopCharging,
    Honk,
    Flash,
    OpenTrunk,
    CloseTrunk,
    RemoteStart,
    Custom {
        name: String,
        params: HashMap<String, serde_json::Value>,
    },
}

// ============================================================================
// Provider Trait
// ============================================================================

#[async_trait]
pub trait RealworldProvider: Send + Sync {
    // IoT
    async fn list_devices(&self) -> Result<Vec<IoTDevice>>;
    async fn get_device(&self, device_id: &str) -> Result<IoTDevice>;
    async fn execute_command(&self, command: DeviceCommand) -> Result<()>;
    async fn execute_scene(&self, scene_id: &str) -> Result<()>;

    // Location
    async fn get_current_location(&self) -> Result<Location>;
    async fn search_places(&self, query: &str, near: &Location) -> Result<Vec<Place>>;
    async fn get_route(&self, from: &Location, to: &Location) -> Result<Route>;

    // Health
    async fn get_health_data(
        &self,
        metric_types: &[HealthMetricType],
        days: u32,
    ) -> Result<HealthData>;
    async fn get_sleep_analysis(&self, date: &str) -> Result<SleepAnalysis>;
    async fn get_workouts(&self, days: u32) -> Result<Vec<Workout>>;

    // Vehicle
    async fn list_vehicles(&self) -> Result<Vec<Vehicle>>;
    async fn get_vehicle_state(&self, vehicle_id: &str) -> Result<VehicleState>;
    async fn send_vehicle_command(&self, vehicle_id: &str, command: VehicleCommand) -> Result<()>;
}

// ============================================================================
// Realworld Engine
// ============================================================================

pub struct RealworldEngine {
    provider: Arc<dyn RealworldProvider>,
    devices: Arc<RwLock<HashMap<String, IoTDevice>>>,
    geofences: Arc<RwLock<HashMap<String, Geofence>>>,
    automations: Arc<RwLock<HashMap<String, Automation>>>,
    scenes: Arc<RwLock<HashMap<String, Scene>>>,
}

impl RealworldEngine {
    pub fn new(provider: Arc<dyn RealworldProvider>) -> Self {
        Self {
            provider,
            devices: Arc::new(RwLock::new(HashMap::new())),
            geofences: Arc::new(RwLock::new(HashMap::new())),
            automations: Arc::new(RwLock::new(HashMap::new())),
            scenes: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn refresh_devices(&self) -> Result<()> {
        let devices = self.provider.list_devices().await?;
        let mut cache = self.devices.write().await;
        cache.clear();
        for device in devices {
            cache.insert(device.id.clone(), device);
        }
        Ok(())
    }

    pub async fn get_device(&self, device_id: &str) -> Result<IoTDevice> {
        let cache = self.devices.read().await;
        if let Some(device) = cache.get(device_id) {
            return Ok(device.clone());
        }
        drop(cache);

        let device = self.provider.get_device(device_id).await?;
        let mut cache = self.devices.write().await;
        cache.insert(device_id.to_string(), device.clone());
        Ok(device)
    }

    pub async fn control_device(&self, device_id: &str, action: DeviceAction) -> Result<()> {
        let command = DeviceCommand {
            device_id: device_id.to_string(),
            action,
        };
        self.provider.execute_command(command).await
    }

    pub async fn add_scene(&self, scene: Scene) -> Result<()> {
        let mut scenes = self.scenes.write().await;
        scenes.insert(scene.id.clone(), scene);
        Ok(())
    }

    pub async fn execute_scene(&self, scene_id: &str) -> Result<()> {
        let scenes = self.scenes.read().await;
        let scene = scenes
            .get(scene_id)
            .ok_or_else(|| RealworldError::DeviceNotFound(format!("Scene {}", scene_id)))?;

        for command in &scene.commands {
            self.provider.execute_command(command.clone()).await?;
        }
        Ok(())
    }

    pub async fn add_geofence(&self, geofence: Geofence) -> Result<()> {
        let mut geofences = self.geofences.write().await;
        geofences.insert(geofence.id.clone(), geofence);
        Ok(())
    }

    pub async fn check_geofence(&self, geofence_id: &str, location: &Location) -> Result<bool> {
        let geofences = self.geofences.read().await;
        let geofence = geofences
            .get(geofence_id)
            .ok_or_else(|| RealworldError::DeviceNotFound(format!("Geofence {}", geofence_id)))?;

        let distance = haversine_distance(
            geofence.center.latitude,
            geofence.center.longitude,
            location.latitude,
            location.longitude,
        );

        Ok(distance <= geofence.radius_meters as f64)
    }

    pub async fn add_automation(&self, automation: Automation) -> Result<()> {
        let mut automations = self.automations.write().await;
        automations.insert(automation.id.clone(), automation);
        Ok(())
    }

    pub async fn get_health_summary(&self, days: u32) -> Result<HealthSummary> {
        let metrics = vec![
            HealthMetricType::Steps,
            HealthMetricType::HeartRate,
            HealthMetricType::SleepDuration,
            HealthMetricType::ActiveCalories,
        ];

        let data = self.provider.get_health_data(&metrics, days).await?;

        let mut total_steps = 0u64;
        let mut heart_rates = Vec::new();
        let mut total_sleep_minutes = 0u64;
        let mut total_calories = 0u64;

        for metric in &data.metrics {
            match metric.metric_type {
                HealthMetricType::Steps => total_steps += metric.value as u64,
                HealthMetricType::HeartRate => heart_rates.push(metric.value),
                HealthMetricType::SleepDuration => total_sleep_minutes += metric.value as u64,
                HealthMetricType::ActiveCalories => total_calories += metric.value as u64,
                _ => {}
            }
        }

        let avg_heart_rate = if heart_rates.is_empty() {
            None
        } else {
            Some(heart_rates.iter().sum::<f64>() / heart_rates.len() as f64)
        };

        Ok(HealthSummary {
            total_steps,
            avg_daily_steps: total_steps / days as u64,
            avg_heart_rate,
            total_sleep_minutes,
            avg_sleep_hours: total_sleep_minutes as f32 / days as f32 / 60.0,
            total_active_calories: total_calories,
            days,
        })
    }

    pub async fn get_vehicle_status(&self, vehicle_id: &str) -> Result<VehicleState> {
        self.provider.get_vehicle_state(vehicle_id).await
    }

    pub async fn control_vehicle(&self, vehicle_id: &str, command: VehicleCommand) -> Result<()> {
        self.provider
            .send_vehicle_command(vehicle_id, command)
            .await
    }

    pub async fn find_nearby(&self, query: &str) -> Result<Vec<Place>> {
        let location = self.provider.get_current_location().await?;
        self.provider.search_places(query, &location).await
    }

    pub async fn get_directions(&self, destination: &str) -> Result<Route> {
        let current = self.provider.get_current_location().await?;
        let places = self.provider.search_places(destination, &current).await?;

        if places.is_empty() {
            return Err(RealworldError::DeviceNotFound(format!(
                "Place: {}",
                destination
            )));
        }

        self.provider.get_route(&current, &places[0].location).await
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthSummary {
    pub total_steps: u64,
    pub avg_daily_steps: u64,
    pub avg_heart_rate: Option<f64>,
    pub total_sleep_minutes: u64,
    pub avg_sleep_hours: f32,
    pub total_active_calories: u64,
    pub days: u32,
}

// Helper function for distance calculation
fn haversine_distance(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    const EARTH_RADIUS_METERS: f64 = 6_371_000.0;

    let lat1_rad = lat1.to_radians();
    let lat2_rad = lat2.to_radians();
    let delta_lat = (lat2 - lat1).to_radians();
    let delta_lon = (lon2 - lon1).to_radians();

    let a = (delta_lat / 2.0).sin().powi(2)
        + lat1_rad.cos() * lat2_rad.cos() * (delta_lon / 2.0).sin().powi(2);
    let c = 2.0 * a.sqrt().asin();

    EARTH_RADIUS_METERS * c
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    struct MockProvider {
        devices: Vec<IoTDevice>,
    }

    impl MockProvider {
        fn new() -> Self {
            Self {
                devices: vec![IoTDevice {
                    id: "light-1".to_string(),
                    name: "Living Room Light".to_string(),
                    device_type: DeviceType::Light,
                    location: Some("Living Room".to_string()),
                    state: DeviceState {
                        online: true,
                        power: Some(true),
                        brightness: Some(80),
                        temperature: None,
                        humidity: None,
                        locked: None,
                        custom: HashMap::new(),
                    },
                    capabilities: vec![DeviceCapability::OnOff, DeviceCapability::Brightness],
                    last_seen: 1700000000,
                }],
            }
        }
    }

    #[async_trait]
    impl RealworldProvider for MockProvider {
        async fn list_devices(&self) -> Result<Vec<IoTDevice>> {
            Ok(self.devices.clone())
        }

        async fn get_device(&self, device_id: &str) -> Result<IoTDevice> {
            self.devices
                .iter()
                .find(|d| d.id == device_id)
                .cloned()
                .ok_or_else(|| RealworldError::DeviceNotFound(device_id.to_string()))
        }

        async fn execute_command(&self, _command: DeviceCommand) -> Result<()> {
            Ok(())
        }

        async fn execute_scene(&self, _scene_id: &str) -> Result<()> {
            Ok(())
        }

        async fn get_current_location(&self) -> Result<Location> {
            Ok(Location {
                latitude: 37.7749,
                longitude: -122.4194,
                altitude: Some(10.0),
                accuracy: Some(5.0),
                timestamp: 1700000000,
            })
        }

        async fn search_places(&self, query: &str, _near: &Location) -> Result<Vec<Place>> {
            Ok(vec![Place {
                id: "place-1".to_string(),
                name: query.to_string(),
                address: Some("123 Test St".to_string()),
                location: Location {
                    latitude: 37.7849,
                    longitude: -122.4094,
                    altitude: None,
                    accuracy: None,
                    timestamp: 1700000000,
                },
                category: Some("Restaurant".to_string()),
                rating: Some(4.5),
                open_now: Some(true),
            }])
        }

        async fn get_route(&self, from: &Location, to: &Location) -> Result<Route> {
            Ok(Route {
                origin: from.clone(),
                destination: to.clone(),
                waypoints: vec![],
                distance_meters: 1500,
                duration_seconds: 600,
                steps: vec![],
            })
        }

        async fn get_health_data(
            &self,
            _metric_types: &[HealthMetricType],
            _days: u32,
        ) -> Result<HealthData> {
            Ok(HealthData {
                user_id: "user-1".to_string(),
                metrics: vec![HealthMetric {
                    metric_type: HealthMetricType::Steps,
                    value: 10000.0,
                    unit: "steps".to_string(),
                    timestamp: 1700000000,
                    source: Some("Watch".to_string()),
                }],
                period_start: 1699900000,
                period_end: 1700000000,
            })
        }

        async fn get_sleep_analysis(&self, _date: &str) -> Result<SleepAnalysis> {
            Ok(SleepAnalysis {
                total_duration_minutes: 480,
                time_asleep_minutes: 450,
                time_awake_minutes: 30,
                deep_sleep_minutes: 90,
                rem_sleep_minutes: 120,
                light_sleep_minutes: 240,
                efficiency: 0.94,
                start_time: 1699920000,
                end_time: 1699948800,
            })
        }

        async fn get_workouts(&self, _days: u32) -> Result<Vec<Workout>> {
            Ok(vec![Workout {
                id: "workout-1".to_string(),
                activity_type: "Running".to_string(),
                start_time: 1699990000,
                end_time: 1699993600,
                duration_seconds: 3600,
                calories: Some(500),
                distance_meters: Some(8000),
                avg_heart_rate: Some(145),
                max_heart_rate: Some(175),
            }])
        }

        async fn list_vehicles(&self) -> Result<Vec<Vehicle>> {
            Ok(vec![Vehicle {
                id: "car-1".to_string(),
                name: "My Car".to_string(),
                make: "Tesla".to_string(),
                model: "Model 3".to_string(),
                year: 2023,
                vin: Some("5YJ3E1EA1PF000001".to_string()),
                vehicle_type: VehicleType::ElectricVehicle,
            }])
        }

        async fn get_vehicle_state(&self, _vehicle_id: &str) -> Result<VehicleState> {
            Ok(VehicleState {
                vehicle_id: "car-1".to_string(),
                location: Some(Location {
                    latitude: 37.7749,
                    longitude: -122.4194,
                    altitude: None,
                    accuracy: None,
                    timestamp: 1700000000,
                }),
                odometer_km: Some(15000),
                fuel_level_percent: None,
                battery_level_percent: Some(80),
                range_km: Some(320),
                locked: Some(true),
                doors_open: vec![],
                windows_open: vec![],
                climate_on: Some(false),
                interior_temp_celsius: Some(22.0),
                exterior_temp_celsius: Some(18.0),
                charging: Some(false),
                charging_complete_time: None,
                timestamp: 1700000000,
            })
        }

        async fn send_vehicle_command(
            &self,
            _vehicle_id: &str,
            _command: VehicleCommand,
        ) -> Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_device_operations() {
        let provider = Arc::new(MockProvider::new());
        let engine = RealworldEngine::new(provider);

        engine.refresh_devices().await.unwrap();
        let device = engine.get_device("light-1").await.unwrap();
        assert_eq!(device.name, "Living Room Light");
        assert_eq!(device.device_type, DeviceType::Light);
    }

    #[tokio::test]
    async fn test_geofence() {
        let provider = Arc::new(MockProvider::new());
        let engine = RealworldEngine::new(provider);

        let geofence = Geofence {
            id: "home".to_string(),
            name: "Home".to_string(),
            center: Location {
                latitude: 37.7749,
                longitude: -122.4194,
                altitude: None,
                accuracy: None,
                timestamp: 0,
            },
            radius_meters: 100.0,
        };

        engine.add_geofence(geofence).await.unwrap();

        let inside = Location {
            latitude: 37.7749,
            longitude: -122.4194,
            altitude: None,
            accuracy: None,
            timestamp: 0,
        };
        assert!(engine.check_geofence("home", &inside).await.unwrap());

        let outside = Location {
            latitude: 37.7849,
            longitude: -122.4294,
            altitude: None,
            accuracy: None,
            timestamp: 0,
        };
        assert!(!engine.check_geofence("home", &outside).await.unwrap());
    }

    #[tokio::test]
    async fn test_scene_execution() {
        let provider = Arc::new(MockProvider::new());
        let engine = RealworldEngine::new(provider);

        let scene = Scene {
            id: "movie".to_string(),
            name: "Movie Time".to_string(),
            commands: vec![DeviceCommand {
                device_id: "light-1".to_string(),
                action: DeviceAction::SetBrightness(20),
            }],
        };

        engine.add_scene(scene).await.unwrap();
        engine.execute_scene("movie").await.unwrap();
    }

    #[tokio::test]
    async fn test_haversine_distance() {
        // San Francisco to Oakland ~13km
        let distance = haversine_distance(37.7749, -122.4194, 37.8044, -122.2712);
        assert!(distance > 12000.0 && distance < 15000.0);
    }

    #[tokio::test]
    async fn test_vehicle_control() {
        let provider = Arc::new(MockProvider::new());
        let engine = RealworldEngine::new(provider);

        let state = engine.get_vehicle_status("car-1").await.unwrap();
        assert_eq!(state.battery_level_percent, Some(80));
        assert_eq!(state.locked, Some(true));

        engine
            .control_vehicle("car-1", VehicleCommand::Unlock)
            .await
            .unwrap();
    }
}
