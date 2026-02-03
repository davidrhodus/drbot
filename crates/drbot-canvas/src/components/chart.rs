//! Chart component.

use super::{BaseProps, Component};
use crate::Result;
use async_trait::async_trait;
use drbot_canvas_protocol::{ComponentId, ComponentSpec, ComponentType, StyleSpec};
use serde_json::Value;
use std::collections::HashMap;

/// Chart types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChartType {
    Line,
    Bar,
    Pie,
    Area,
    Scatter,
    Donut,
}

impl ChartType {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Line => "line",
            Self::Bar => "bar",
            Self::Pie => "pie",
            Self::Area => "area",
            Self::Scatter => "scatter",
            Self::Donut => "donut",
        }
    }

    fn from_str(s: &str) -> Option<Self> {
        match s {
            "line" => Some(Self::Line),
            "bar" => Some(Self::Bar),
            "pie" => Some(Self::Pie),
            "area" => Some(Self::Area),
            "scatter" => Some(Self::Scatter),
            "donut" => Some(Self::Donut),
            _ => None,
        }
    }
}

/// Chart data series.
#[derive(Debug, Clone)]
pub struct ChartSeries {
    /// Series name.
    pub name: String,
    /// Data points.
    pub data: Vec<ChartDataPoint>,
    /// Series color.
    pub color: Option<String>,
}

impl ChartSeries {
    /// Create a new series.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            data: Vec::new(),
            color: None,
        }
    }

    /// Add a data point.
    pub fn point(mut self, x: impl Into<Value>, y: f64) -> Self {
        self.data.push(ChartDataPoint { x: x.into(), y });
        self
    }

    /// Set color.
    pub fn color(mut self, color: impl Into<String>) -> Self {
        self.color = Some(color.into());
        self
    }
}

/// Chart data point.
#[derive(Debug, Clone)]
pub struct ChartDataPoint {
    /// X value.
    pub x: Value,
    /// Y value.
    pub y: f64,
}

/// Chart component.
pub struct ChartComponent {
    base: BaseProps,
    style: Option<StyleSpec>,
    chart_type: ChartType,
    series: Vec<ChartSeries>,
    title: Option<String>,
    x_axis_label: Option<String>,
    y_axis_label: Option<String>,
}

impl ChartComponent {
    /// Create a new chart.
    pub fn new(id: ComponentId, chart_type: ChartType) -> Self {
        Self {
            base: BaseProps::new(id),
            style: None,
            chart_type,
            series: Vec::new(),
            title: None,
            x_axis_label: None,
            y_axis_label: None,
        }
    }

    /// Create from specification.
    pub fn from_spec(spec: &ComponentSpec) -> Result<Self> {
        let chart_type = spec
            .props
            .get("chartType")
            .and_then(|v| v.as_str())
            .and_then(ChartType::from_str)
            .unwrap_or(ChartType::Line);

        let title = spec
            .props
            .get("title")
            .and_then(|v| v.as_str())
            .map(String::from);

        let x_axis_label = spec
            .props
            .get("xAxisLabel")
            .and_then(|v| v.as_str())
            .map(String::from);

        let y_axis_label = spec
            .props
            .get("yAxisLabel")
            .and_then(|v| v.as_str())
            .map(String::from);

        let series = spec
            .props
            .get("series")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|s| {
                        let name = s.get("name")?.as_str()?.to_string();
                        let color = s.get("color").and_then(|v| v.as_str()).map(String::from);
                        let data = s
                            .get("data")?
                            .as_array()?
                            .iter()
                            .filter_map(|d| {
                                let x = d.get("x")?.clone();
                                let y = d.get("y")?.as_f64()?;
                                Some(ChartDataPoint { x, y })
                            })
                            .collect();
                        Some(ChartSeries { name, data, color })
                    })
                    .collect()
            })
            .unwrap_or_default();

        let mut chart = Self::new(spec.id.clone(), chart_type);
        chart.style = spec.style.clone();
        chart.title = title;
        chart.x_axis_label = x_axis_label;
        chart.y_axis_label = y_axis_label;
        chart.series = series;

        Ok(chart)
    }

    /// Set chart title.
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Set X axis label.
    pub fn x_axis_label(mut self, label: impl Into<String>) -> Self {
        self.x_axis_label = Some(label.into());
        self
    }

    /// Set Y axis label.
    pub fn y_axis_label(mut self, label: impl Into<String>) -> Self {
        self.y_axis_label = Some(label.into());
        self
    }

    /// Add a data series.
    pub fn series(mut self, series: ChartSeries) -> Self {
        self.series.push(series);
        self
    }

    /// Set style.
    pub fn style(mut self, style: StyleSpec) -> Self {
        self.style = Some(style);
        self
    }
}

#[async_trait]
impl Component for ChartComponent {
    fn id(&self) -> &ComponentId {
        &self.base.id
    }

    fn component_type(&self) -> ComponentType {
        ComponentType::Chart
    }

    fn render(&self) -> ComponentSpec {
        let mut props = self.base.props.clone();

        props.insert(
            "chartType".to_string(),
            serde_json::json!(self.chart_type.as_str()),
        );

        if let Some(ref title) = self.title {
            props.insert("title".to_string(), serde_json::json!(title));
        }
        if let Some(ref label) = self.x_axis_label {
            props.insert("xAxisLabel".to_string(), serde_json::json!(label));
        }
        if let Some(ref label) = self.y_axis_label {
            props.insert("yAxisLabel".to_string(), serde_json::json!(label));
        }

        let series_json: Vec<Value> = self
            .series
            .iter()
            .map(|s| {
                let data: Vec<Value> = s
                    .data
                    .iter()
                    .map(|d| serde_json::json!({"x": d.x, "y": d.y}))
                    .collect();

                let mut obj = serde_json::json!({
                    "name": s.name,
                    "data": data
                });
                if let Some(ref color) = s.color {
                    obj["color"] = serde_json::json!(color);
                }
                obj
            })
            .collect();

        props.insert("series".to_string(), serde_json::json!(series_json));

        ComponentSpec {
            id: self.base.id.clone(),
            component_type: ComponentType::Chart,
            props,
            style: self.style.clone(),
            layout: None,
            children: vec![],
            events: vec!["click".to_string(), "hover".to_string()],
        }
    }

    async fn handle_event(&mut self, event_type: &str, data: Value) -> Result<Option<Value>> {
        match event_type {
            "click" | "hover" => Ok(Some(serde_json::json!({
                "chart_id": self.base.id.0,
                "event": event_type,
                "data": data
            }))),
            _ => Ok(None),
        }
    }

    fn props(&self) -> &HashMap<String, Value> {
        &self.base.props
    }

    fn set_prop(&mut self, key: &str, value: Value) {
        match key {
            "chartType" => {
                if let Some(s) = value.as_str() {
                    if let Some(ct) = ChartType::from_str(s) {
                        self.chart_type = ct;
                    }
                }
            }
            "title" => {
                self.title = value.as_str().map(String::from);
            }
            _ => {
                self.base.set(key, value);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chart_component() {
        let chart = ChartComponent::new(ComponentId::from_str("chart1"), ChartType::Line)
            .title("Sales Data")
            .series(
                ChartSeries::new("Revenue")
                    .point("Jan", 100.0)
                    .point("Feb", 150.0)
                    .point("Mar", 200.0)
                    .color("#3498db"),
            );

        let spec = chart.render();
        assert_eq!(spec.component_type, ComponentType::Chart);
        assert_eq!(spec.props.get("chartType").unwrap(), "line");
        assert_eq!(spec.props.get("title").unwrap(), "Sales Data");
    }
}
