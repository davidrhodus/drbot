//! Table component.

use super::{BaseProps, Component};
use crate::Result;
use async_trait::async_trait;
use drbot_canvas_protocol::{ComponentId, ComponentSpec, ComponentType, StyleSpec};
use serde_json::Value;
use std::collections::HashMap;

/// Table column definition.
#[derive(Debug, Clone)]
pub struct TableColumn {
    /// Column key (field name in data).
    pub key: String,
    /// Column header label.
    pub label: String,
    /// Column width.
    pub width: Option<String>,
    /// Whether sortable.
    pub sortable: bool,
    /// Alignment.
    pub align: Option<String>,
}

impl TableColumn {
    /// Create a new column.
    pub fn new(key: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            width: None,
            sortable: false,
            align: None,
        }
    }

    /// Set width.
    pub fn width(mut self, width: impl Into<String>) -> Self {
        self.width = Some(width.into());
        self
    }

    /// Make sortable.
    pub fn sortable(mut self) -> Self {
        self.sortable = true;
        self
    }

    /// Set alignment.
    pub fn align(mut self, align: impl Into<String>) -> Self {
        self.align = Some(align.into());
        self
    }
}

/// Table component.
pub struct TableComponent {
    base: BaseProps,
    style: Option<StyleSpec>,
    columns: Vec<TableColumn>,
    data: Vec<HashMap<String, Value>>,
    sort_column: Option<String>,
    sort_direction: SortDirection,
    selected_rows: Vec<usize>,
    selectable: bool,
}

/// Sort direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDirection {
    Ascending,
    Descending,
}

impl Default for SortDirection {
    fn default() -> Self {
        Self::Ascending
    }
}

impl TableComponent {
    /// Create a new table.
    pub fn new(id: ComponentId) -> Self {
        Self {
            base: BaseProps::new(id),
            style: None,
            columns: Vec::new(),
            data: Vec::new(),
            sort_column: None,
            sort_direction: SortDirection::default(),
            selected_rows: Vec::new(),
            selectable: false,
        }
    }

    /// Create from specification.
    pub fn from_spec(spec: &ComponentSpec) -> Result<Self> {
        let columns = spec
            .props
            .get("columns")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|col| {
                        let key = col.get("key")?.as_str()?.to_string();
                        let label = col
                            .get("label")
                            .and_then(|v| v.as_str())
                            .unwrap_or(&key)
                            .to_string();
                        let width = col.get("width").and_then(|v| v.as_str()).map(String::from);
                        let sortable = col
                            .get("sortable")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);
                        let align = col.get("align").and_then(|v| v.as_str()).map(String::from);
                        Some(TableColumn {
                            key,
                            label,
                            width,
                            sortable,
                            align,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        let data = spec
            .props
            .get("data")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|row| row.as_object())
                    .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                    .collect()
            })
            .unwrap_or_default();

        let selectable = spec
            .props
            .get("selectable")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let mut table = Self::new(spec.id.clone());
        table.style = spec.style.clone();
        table.columns = columns;
        table.data = data;
        table.selectable = selectable;

        Ok(table)
    }

    /// Add a column.
    pub fn column(mut self, column: TableColumn) -> Self {
        self.columns.push(column);
        self
    }

    /// Set columns.
    pub fn columns(mut self, columns: Vec<TableColumn>) -> Self {
        self.columns = columns;
        self
    }

    /// Set data.
    pub fn data(mut self, data: Vec<HashMap<String, Value>>) -> Self {
        self.data = data;
        self
    }

    /// Enable row selection.
    pub fn selectable(mut self, selectable: bool) -> Self {
        self.selectable = selectable;
        self
    }

    /// Set style.
    pub fn style(mut self, style: StyleSpec) -> Self {
        self.style = Some(style);
        self
    }
}

#[async_trait]
impl Component for TableComponent {
    fn id(&self) -> &ComponentId {
        &self.base.id
    }

    fn component_type(&self) -> ComponentType {
        ComponentType::Table
    }

    fn render(&self) -> ComponentSpec {
        let mut props = self.base.props.clone();

        let columns_json: Vec<Value> = self
            .columns
            .iter()
            .map(|col| {
                let mut obj = serde_json::json!({
                    "key": col.key,
                    "label": col.label,
                    "sortable": col.sortable
                });
                if let Some(ref width) = col.width {
                    obj["width"] = serde_json::json!(width);
                }
                if let Some(ref align) = col.align {
                    obj["align"] = serde_json::json!(align);
                }
                obj
            })
            .collect();

        props.insert("columns".to_string(), serde_json::json!(columns_json));
        props.insert("data".to_string(), serde_json::json!(self.data));
        props.insert("selectable".to_string(), serde_json::json!(self.selectable));
        props.insert(
            "selectedRows".to_string(),
            serde_json::json!(self.selected_rows),
        );

        if let Some(ref sort_col) = self.sort_column {
            props.insert("sortColumn".to_string(), serde_json::json!(sort_col));
            props.insert(
                "sortDirection".to_string(),
                serde_json::json!(match self.sort_direction {
                    SortDirection::Ascending => "asc",
                    SortDirection::Descending => "desc",
                }),
            );
        }

        ComponentSpec {
            id: self.base.id.clone(),
            component_type: ComponentType::Table,
            props,
            style: self.style.clone(),
            layout: None,
            children: vec![],
            events: vec![
                "sort".to_string(),
                "select".to_string(),
                "rowClick".to_string(),
            ],
        }
    }

    async fn handle_event(&mut self, event_type: &str, data: Value) -> Result<Option<Value>> {
        match event_type {
            "sort" => {
                if let Some(column) = data.get("column").and_then(|v| v.as_str()) {
                    if self.sort_column.as_deref() == Some(column) {
                        // Toggle direction
                        self.sort_direction = match self.sort_direction {
                            SortDirection::Ascending => SortDirection::Descending,
                            SortDirection::Descending => SortDirection::Ascending,
                        };
                    } else {
                        self.sort_column = Some(column.to_string());
                        self.sort_direction = SortDirection::Ascending;
                    }
                    Ok(Some(serde_json::json!({
                        "table_id": self.base.id.0,
                        "sortColumn": self.sort_column,
                        "sortDirection": match self.sort_direction {
                            SortDirection::Ascending => "asc",
                            SortDirection::Descending => "desc",
                        }
                    })))
                } else {
                    Ok(None)
                }
            }
            "select" => {
                if let Some(row_index) = data.get("rowIndex").and_then(|v| v.as_u64()) {
                    let idx = row_index as usize;
                    if let Some(pos) = self.selected_rows.iter().position(|&r| r == idx) {
                        self.selected_rows.remove(pos);
                    } else {
                        self.selected_rows.push(idx);
                    }
                    Ok(Some(serde_json::json!({
                        "table_id": self.base.id.0,
                        "selectedRows": self.selected_rows
                    })))
                } else {
                    Ok(None)
                }
            }
            "rowClick" => {
                if let Some(row_index) = data.get("rowIndex").and_then(|v| v.as_u64()) {
                    let idx = row_index as usize;
                    let row_data = self.data.get(idx).cloned();
                    Ok(Some(serde_json::json!({
                        "table_id": self.base.id.0,
                        "rowIndex": idx,
                        "rowData": row_data
                    })))
                } else {
                    Ok(None)
                }
            }
            _ => Ok(None),
        }
    }

    fn props(&self) -> &HashMap<String, Value> {
        &self.base.props
    }

    fn set_prop(&mut self, key: &str, value: Value) {
        match key {
            "data" => {
                if let Some(arr) = value.as_array() {
                    self.data = arr
                        .iter()
                        .filter_map(|v| v.as_object())
                        .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                        .collect();
                }
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
    fn test_table_component() {
        let data = vec![
            [("name".to_string(), serde_json::json!("Alice"))]
                .into_iter()
                .collect(),
            [("name".to_string(), serde_json::json!("Bob"))]
                .into_iter()
                .collect(),
        ];

        let table = TableComponent::new(ComponentId::from_str("table1"))
            .column(TableColumn::new("name", "Name").sortable())
            .data(data)
            .selectable(true);

        let spec = table.render();
        assert_eq!(spec.component_type, ComponentType::Table);
        assert!(spec.events.contains(&"sort".to_string()));
    }
}
