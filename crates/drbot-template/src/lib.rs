//! Simple template engine for drbot.
//!
//! This crate provides:
//! - Variable substitution
//! - Conditional blocks
//! - Loop blocks
//! - Filters

use regex::Regex;
use std::collections::HashMap;
use thiserror::Error;

/// Template error types.
#[derive(Error, Debug)]
pub enum TemplateError {
    #[error("Syntax error: {0}")]
    SyntaxError(String),

    #[error("Missing variable: {0}")]
    MissingVariable(String),

    #[error("Invalid filter: {0}")]
    InvalidFilter(String),

    #[error("Render error: {0}")]
    RenderError(String),
}

/// Result type for template operations.
pub type Result<T> = std::result::Result<T, TemplateError>;

/// Template value.
#[derive(Debug, Clone)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    Array(Vec<Value>),
    Object(HashMap<String, Value>),
}

impl Value {
    /// Check if value is truthy.
    pub fn is_truthy(&self) -> bool {
        match self {
            Value::Null => false,
            Value::Bool(b) => *b,
            Value::Int(i) => *i != 0,
            Value::Float(f) => *f != 0.0,
            Value::String(s) => !s.is_empty(),
            Value::Array(a) => !a.is_empty(),
            Value::Object(o) => !o.is_empty(),
        }
    }

    /// Convert to string.
    pub fn to_string(&self) -> String {
        match self {
            Value::Null => String::new(),
            Value::Bool(b) => b.to_string(),
            Value::Int(i) => i.to_string(),
            Value::Float(f) => f.to_string(),
            Value::String(s) => s.clone(),
            Value::Array(a) => {
                let items: Vec<String> = a.iter().map(|v| v.to_string()).collect();
                items.join(", ")
            }
            Value::Object(_) => "[object]".to_string(),
        }
    }

    /// Get nested value by path.
    pub fn get(&self, path: &str) -> Option<&Value> {
        let parts: Vec<&str> = path.split('.').collect();
        let mut current = self;

        for part in parts {
            match current {
                Value::Object(obj) => {
                    current = obj.get(part)?;
                }
                Value::Array(arr) => {
                    let idx: usize = part.parse().ok()?;
                    current = arr.get(idx)?;
                }
                _ => return None,
            }
        }

        Some(current)
    }
}

impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Value::String(s.to_string())
    }
}

impl From<String> for Value {
    fn from(s: String) -> Self {
        Value::String(s)
    }
}

impl From<i64> for Value {
    fn from(i: i64) -> Self {
        Value::Int(i)
    }
}

impl From<i32> for Value {
    fn from(i: i32) -> Self {
        Value::Int(i as i64)
    }
}

impl From<f64> for Value {
    fn from(f: f64) -> Self {
        Value::Float(f)
    }
}

impl From<bool> for Value {
    fn from(b: bool) -> Self {
        Value::Bool(b)
    }
}

impl<T: Into<Value>> From<Vec<T>> for Value {
    fn from(v: Vec<T>) -> Self {
        Value::Array(v.into_iter().map(|x| x.into()).collect())
    }
}

/// Template context.
#[derive(Debug, Clone, Default)]
pub struct Context {
    values: HashMap<String, Value>,
}

impl Context {
    /// Create new empty context.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set value.
    pub fn set<V: Into<Value>>(&mut self, key: &str, value: V) {
        self.values.insert(key.to_string(), value.into());
    }

    /// Get value.
    pub fn get(&self, key: &str) -> Option<&Value> {
        // Handle nested paths
        if key.contains('.') {
            let parts: Vec<&str> = key.splitn(2, '.').collect();
            if let Some(value) = self.values.get(parts[0]) {
                return value.get(parts[1]);
            }
            return None;
        }
        self.values.get(key)
    }

    /// Check if key exists.
    pub fn has(&self, key: &str) -> bool {
        self.get(key).is_some()
    }

    /// Remove value.
    pub fn remove(&mut self, key: &str) -> Option<Value> {
        self.values.remove(key)
    }

    /// Extend with another context.
    pub fn extend(&mut self, other: &Context) {
        self.values.extend(other.values.clone());
    }
}

/// Simple template engine.
pub struct Template {
    source: String,
}

impl Template {
    /// Create new template.
    pub fn new(source: &str) -> Self {
        Self {
            source: source.to_string(),
        }
    }

    /// Render template with context.
    pub fn render(&self, ctx: &Context) -> Result<String> {
        let mut output = self.source.clone();

        // Process conditionals first
        output = self.process_conditionals(&output, ctx)?;

        // Process loops
        output = self.process_loops(&output, ctx)?;

        // Process variables
        output = self.process_variables(&output, ctx)?;

        Ok(output)
    }

    fn process_variables(&self, input: &str, ctx: &Context) -> Result<String> {
        let re = Regex::new(r"\{\{\s*([a-zA-Z_][a-zA-Z0-9_\.]*)\s*(\|[^}]+)?\}\}").unwrap();

        let mut output = input.to_string();
        for cap in re.captures_iter(input) {
            let full_match = cap.get(0).unwrap().as_str();
            let var_name = cap.get(1).unwrap().as_str();
            let filters = cap.get(2).map(|m| m.as_str());

            let value = ctx.get(var_name).cloned().unwrap_or(Value::Null);
            let mut result = value.to_string();

            // Apply filters
            if let Some(filter_str) = filters {
                for filter in filter_str.trim_start_matches('|').split('|') {
                    result = self.apply_filter(&result, filter.trim())?;
                }
            }

            output = output.replace(full_match, &result);
        }

        Ok(output)
    }

    fn process_conditionals(&self, input: &str, ctx: &Context) -> Result<String> {
        let re =
            Regex::new(r"\{%\s*if\s+([a-zA-Z_][a-zA-Z0-9_\.]*)\s*%\}([\s\S]*?)\{%\s*endif\s*%\}")
                .unwrap();

        let mut output = input.to_string();
        for cap in re.captures_iter(input) {
            let full_match = cap.get(0).unwrap().as_str();
            let var_name = cap.get(1).unwrap().as_str();
            let content = cap.get(2).unwrap().as_str();

            let value = ctx.get(var_name);
            let is_truthy = value.map(|v| v.is_truthy()).unwrap_or(false);

            // Check for else
            let (if_content, else_content) = if content.contains("{% else %}") {
                let parts: Vec<&str> = content.splitn(2, "{% else %}").collect();
                (parts[0], parts.get(1).copied().unwrap_or(""))
            } else {
                (content, "")
            };

            let result = if is_truthy { if_content } else { else_content };
            output = output.replace(full_match, result);
        }

        Ok(output)
    }

    fn process_loops(&self, input: &str, ctx: &Context) -> Result<String> {
        let re = Regex::new(r"\{%\s*for\s+([a-zA-Z_][a-zA-Z0-9_]*)\s+in\s+([a-zA-Z_][a-zA-Z0-9_\.]*)\s*%\}([\s\S]*?)\{%\s*endfor\s*%\}").unwrap();

        let mut output = input.to_string();
        for cap in re.captures_iter(input) {
            let full_match = cap.get(0).unwrap().as_str();
            let item_name = cap.get(1).unwrap().as_str();
            let array_name = cap.get(2).unwrap().as_str();
            let content = cap.get(3).unwrap().as_str();

            let value = ctx.get(array_name);
            let mut result = String::new();

            if let Some(Value::Array(items)) = value {
                for (index, item) in items.iter().enumerate() {
                    let mut loop_ctx = ctx.clone();
                    loop_ctx.values.insert(item_name.to_string(), item.clone());

                    let mut loop_obj = HashMap::new();
                    loop_obj.insert("index".to_string(), Value::Int((index + 1) as i64));
                    loop_obj.insert("index0".to_string(), Value::Int(index as i64));
                    loop_obj.insert("first".to_string(), Value::Bool(index == 0));
                    loop_obj.insert("last".to_string(), Value::Bool(index + 1 == items.len()));
                    loop_obj.insert("length".to_string(), Value::Int(items.len() as i64));
                    loop_obj.insert(
                        "revindex".to_string(),
                        Value::Int((items.len() - index) as i64),
                    );
                    loop_obj.insert(
                        "revindex0".to_string(),
                        Value::Int((items.len() - index - 1) as i64),
                    );
                    loop_ctx.values.insert("loop".to_string(), Value::Object(loop_obj));
                    let rendered = self.process_variables(content, &loop_ctx)?;
                    result.push_str(&rendered);
                }
            }

            output = output.replace(full_match, &result);
        }

        Ok(output)
    }

    fn apply_filter(&self, value: &str, filter: &str) -> Result<String> {
        let (filter_name, arg) = if let Some(pos) = filter.find(':') {
            (&filter[..pos], Some(filter[pos + 1..].trim()))
        } else {
            (filter, None)
        };

        match filter_name {
            "upper" => Ok(value.to_uppercase()),
            "lower" => Ok(value.to_lowercase()),
            "capitalize" => {
                let mut chars = value.chars();
                match chars.next() {
                    None => Ok(String::new()),
                    Some(first) => Ok(first.to_uppercase().collect::<String>() + chars.as_str()),
                }
            }
            "title" => Ok(value
                .split_whitespace()
                .map(|word| {
                    let mut chars = word.chars();
                    match chars.next() {
                        None => String::new(),
                        Some(first) => {
                            first.to_uppercase().collect::<String>()
                                + &chars.as_str().to_lowercase()
                        }
                    }
                })
                .collect::<Vec<_>>()
                .join(" ")),
            "trim" => Ok(value.trim().to_string()),
            "length" => Ok(value.len().to_string()),
            "reverse" => Ok(value.chars().rev().collect()),
            "default" => {
                if value.is_empty() {
                    Ok(arg.unwrap_or("").to_string())
                } else {
                    Ok(value.to_string())
                }
            }
            "truncate" => {
                let len: usize = arg
                    .ok_or_else(|| {
                        TemplateError::InvalidFilter("truncate requires length".to_string())
                    })?
                    .parse()
                    .map_err(|_| TemplateError::InvalidFilter("invalid length".to_string()))?;
                if value.len() > len {
                    Ok(format!("{}...", &value[..len]))
                } else {
                    Ok(value.to_string())
                }
            }
            "replace" => {
                let parts: Vec<&str> = arg
                    .ok_or_else(|| {
                        TemplateError::InvalidFilter("replace requires arguments".to_string())
                    })?
                    .splitn(2, ',')
                    .collect();
                if parts.len() != 2 {
                    return Err(TemplateError::InvalidFilter(
                        "replace requires from,to".to_string(),
                    ));
                }
                Ok(value.replace(parts[0].trim(), parts[1].trim()))
            }
            "escape" => Ok(value
                .replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;")
                .replace('"', "&quot;")
                .replace('\'', "&#39;")),
            _ => Err(TemplateError::InvalidFilter(filter_name.to_string())),
        }
    }
}

/// Quick template rendering.
pub fn render(template: &str, ctx: &Context) -> Result<String> {
    Template::new(template).render(ctx)
}

/// Simple variable substitution (no conditionals or loops).
pub fn substitute(template: &str, vars: &HashMap<String, String>) -> String {
    let mut output = template.to_string();
    for (key, value) in vars {
        output = output.replace(&format!("{{{{{}}}}}", key), value);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_variable() {
        let mut ctx = Context::new();
        ctx.set("name", "World");

        let result = render("Hello, {{ name }}!", &ctx).unwrap();
        assert_eq!(result, "Hello, World!");
    }

    #[test]
    fn test_multiple_variables() {
        let mut ctx = Context::new();
        ctx.set("first", "Hello");
        ctx.set("second", "World");

        let result = render("{{ first }}, {{ second }}!", &ctx).unwrap();
        assert_eq!(result, "Hello, World!");
    }

    #[test]
    fn test_filter_upper() {
        let mut ctx = Context::new();
        ctx.set("name", "hello");

        let result = render("{{ name | upper }}", &ctx).unwrap();
        assert_eq!(result, "HELLO");
    }

    #[test]
    fn test_filter_chain() {
        let mut ctx = Context::new();
        ctx.set("name", "  hello  ");

        let result = render("{{ name | trim | upper }}", &ctx).unwrap();
        assert_eq!(result, "HELLO");
    }

    #[test]
    fn test_conditional_true() {
        let mut ctx = Context::new();
        ctx.set("show", true);

        let result = render("{% if show %}visible{% endif %}", &ctx).unwrap();
        assert_eq!(result, "visible");
    }

    #[test]
    fn test_conditional_false() {
        let mut ctx = Context::new();
        ctx.set("show", false);

        let result = render("{% if show %}visible{% endif %}", &ctx).unwrap();
        assert_eq!(result, "");
    }

    #[test]
    fn test_conditional_else() {
        let mut ctx = Context::new();
        ctx.set("show", false);

        let result = render("{% if show %}yes{% else %}no{% endif %}", &ctx).unwrap();
        assert_eq!(result, "no");
    }

    #[test]
    fn test_loop() {
        let mut ctx = Context::new();
        ctx.set("items", vec!["a", "b", "c"]);

        let result = render("{% for item in items %}{{ item }}{% endfor %}", &ctx).unwrap();
        assert_eq!(result, "abc");
    }

    #[test]
    fn test_loop_with_index() {
        let mut ctx = Context::new();
        ctx.set("items", vec!["a", "b"]);

        let result = render(
            "{% for item in items %}{{ loop.index }}:{{ item }} {% endfor %}",
            &ctx,
        )
        .unwrap();
        assert_eq!(result, "1:a 2:b ");
    }

    #[test]
    fn test_filter_truncate() {
        let mut ctx = Context::new();
        ctx.set("text", "hello world");

        let result = render("{{ text | truncate: 5 }}", &ctx).unwrap();
        assert_eq!(result, "hello...");
    }

    #[test]
    fn test_filter_default() {
        let ctx = Context::new();

        let result = render("{{ missing | default: fallback }}", &ctx).unwrap();
        assert_eq!(result, "fallback");
    }

    #[test]
    fn test_substitute() {
        let mut vars = HashMap::new();
        vars.insert("name".to_string(), "World".to_string());

        let result = substitute("Hello, {{name}}!", &vars);
        assert_eq!(result, "Hello, World!");
    }

    #[test]
    fn test_escape_filter() {
        let mut ctx = Context::new();
        ctx.set("html", "<script>alert('xss')</script>");

        let result = render("{{ html | escape }}", &ctx).unwrap();
        assert!(result.contains("&lt;script&gt;"));
    }
}
