//! Numeric values in a document are either literals or expressions over the
//! document parameters, e.g. `"w / 2 + 3"`. Units are implicit (mm, degrees).

use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum Expr {
    Num(f64),
    Text(String),
}

impl Default for Expr {
    fn default() -> Self {
        Expr::Num(0.0)
    }
}

impl Expr {
    /// True for the literal 0 (serde skips such optional fields).
    pub fn is_zero(&self) -> bool {
        matches!(self, Expr::Num(v) if *v == 0.0)
    }
}

impl From<f64> for Expr {
    fn from(v: f64) -> Self {
        Expr::Num(v)
    }
}

/// Resolved parameter table. Parameters may reference earlier parameters.
pub struct Scope {
    ctx: evalexpr::HashMapContext<evalexpr::DefaultNumericTypes>,
}

impl Scope {
    pub fn new(params: &BTreeMap<String, Expr>, overrides: &BTreeMap<String, String>) -> Result<Self> {
        let mut scope = Scope { ctx: evalexpr::HashMapContext::new() };
        for (name, expr) in params {
            let e = match overrides.get(name) {
                Some(s) => Expr::Text(s.clone()),
                None => expr.clone(),
            };
            let v = scope.eval(&e)?;
            scope.set(name, v)?;
        }
        for (name, s) in overrides {
            if !params.contains_key(name) {
                let v = scope.eval(&Expr::Text(s.clone()))?;
                scope.set(name, v)?;
            }
        }
        Ok(scope)
    }

    fn set(&mut self, name: &str, v: f64) -> Result<()> {
        use evalexpr::ContextWithMutableVariables;
        self.ctx
            .set_value(name.to_string(), evalexpr::Value::Float(v))
            .map_err(|e| Error::Expr(name.to_string(), e.to_string()))
    }

    /// Current value of a parameter, if defined.
    pub fn get(&self, name: &str) -> Option<f64> {
        use evalexpr::Context;
        self.ctx.get_value(name).and_then(|v| v.as_float().ok())
    }

    pub fn eval(&self, e: &Expr) -> Result<f64> {
        match e {
            Expr::Num(v) => Ok(*v),
            Expr::Text(s) => evalexpr::eval_number_with_context(s, &self.ctx)
                .map_err(|err| Error::Expr(s.clone(), err.to_string())),
        }
    }

    pub fn eval3(&self, v: &[Expr; 3]) -> Result<cadrum::DVec3> {
        Ok(cadrum::DVec3::new(self.eval(&v[0])?, self.eval(&v[1])?, self.eval(&v[2])?))
    }
}
