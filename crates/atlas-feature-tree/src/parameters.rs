//! Parametric dimensions and expression variables.

use serde::{Deserialize, Serialize};

/// Parametric model variable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Parameter {
    pub id: String,
    pub name: String,
    pub value: f64,
    pub unit: String,
    pub expression: Option<String>,
}

impl Parameter {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        value: f64,
        unit: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            value,
            unit: unit.into(),
            expression: None,
        }
    }

    pub fn with_expression(mut self, expr: impl Into<String>) -> Self {
        self.expression = Some(expr.into());
        self
    }
}
