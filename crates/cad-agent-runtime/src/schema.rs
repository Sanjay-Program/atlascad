//! A deliberately small JSON Schema checker for agent I/O.
//!
//! Agent contracts are declared inside this workspace, so the runtime does not need a full JSON
//! Schema implementation — it needs the keywords that actually constrain agent messages, checked
//! strictly enough that a contract claiming more than it enforces is impossible:
//!
//! * `type` — `object`, `array`, `string`, `number`, `integer`, `boolean`, `null`
//! * `required` — property names an object must carry
//! * `properties` / `items` — recursed into
//! * `minLength`, `minItems`, `minimum` — the scalar bounds used for budget-like inputs
//! * `maxLength`, `maxItems`, `maximum` — the matching upper bounds
//! * `enum` — a closed set of allowed scalars or strings
//!
//! The upper bounds exist because a lower bound alone does not bound anything: "at least three
//! profile points" still admits a million, and "a positive height" still admits 1e30 mm. An agent
//! contract is a resource-control document as much as a shape document, so the bounds family is
//! complete rather than half-open.
//!
//! Unknown keywords are a contract error rather than a silent pass: a schema the runtime cannot
//! honour must fail loudly at registration, not quietly at run time.

use serde_json::Value;

use cad_agent_api::AgentError;

/// Why an input or output did not satisfy its schema.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SchemaError {
    #[error("at {path}: expected type {expected}, found {found}")]
    Type {
        path: String,
        expected: String,
        found: String,
    },
    #[error("at {path}: missing required property {property:?}")]
    Missing { path: String, property: String },
    #[error("at {path}: {message}")]
    Bound { path: String, message: String },
    #[error("at {path}: unsupported schema keyword {keyword:?}")]
    Unsupported { path: String, keyword: String },
    #[error("at {path}: {message}")]
    Message { path: String, message: String },
}

impl From<SchemaError> for AgentError {
    fn from(error: SchemaError) -> Self {
        AgentError::InvalidInput(error.to_string())
    }
}

/// The keywords this runtime enforces — and therefore the only ones a contract may declare.
const SUPPORTED: &[&str] = &[
    "type",
    "enum",
    "required",
    "properties",
    "items",
    "minItems",
    "maxItems",
    "minLength",
    "maxLength",
    "minimum",
    "maximum",
    "description",
    "title",
];

/// JSON type name of a value, for diagnostics.
fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(n) if n.is_i64() || n.is_u64() => "integer",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// Does `value` satisfy `schema`? Returns the first violation with its path.
pub fn validate(schema: &Value, value: &Value, path: &str) -> Result<(), SchemaError> {
    let object = match schema {
        Value::Object(map) => map,
        // A non-object schema (e.g. `true`) constrains nothing and is accepted as-is.
        Value::Bool(true) => return Ok(()),
        Value::Bool(false) => {
            return Err(SchemaError::Message {
                path: path.to_string(),
                message: "schema is `false`: no value is valid".to_string(),
            });
        }
        _ => {
            return Err(SchemaError::Message {
                path: path.to_string(),
                message: format!(
                    "schema must be an object or boolean, found {}",
                    type_name(schema)
                ),
            });
        }
    };

    if let Some(expected) = object.get("type") {
        check_type(expected, value, path)?;
    }

    if let Some(values) = object.get("enum").and_then(Value::as_array)
        && !values.iter().any(|allowed| allowed == value)
    {
        return Err(SchemaError::Message {
            path: path.to_string(),
            message: format!("{value} is not one of {values:?}"),
        });
    }

    match value {
        Value::Object(map) => {
            if let Some(required) = object.get("required").and_then(Value::as_array) {
                for key in required.iter().filter_map(Value::as_str) {
                    if !map.contains_key(key) {
                        return Err(SchemaError::Missing {
                            path: path.to_string(),
                            property: key.to_string(),
                        });
                    }
                }
            }
            if let Some(properties) = object.get("properties").and_then(Value::as_object) {
                for (key, sub) in properties {
                    if let Some(child) = map.get(key) {
                        let child_path = if path.is_empty() {
                            key.clone()
                        } else {
                            format!("{path}.{key}")
                        };
                        validate(sub, child, &child_path)?;
                    }
                }
            }
        }
        Value::Array(items) => {
            if let Some(min_items) = object.get("minItems").and_then(Value::as_u64)
                && (items.len() as u64) < min_items
            {
                return Err(SchemaError::Bound {
                    path: path.to_string(),
                    message: format!("needs at least {min_items} items, found {}", items.len()),
                });
            }
            if let Some(max_items) = object.get("maxItems").and_then(Value::as_u64)
                && (items.len() as u64) > max_items
            {
                return Err(SchemaError::Bound {
                    path: path.to_string(),
                    message: format!("allows at most {max_items} items, found {}", items.len()),
                });
            }
            if let Some(items_schema) = object.get("items") {
                for (index, item) in items.iter().enumerate() {
                    validate(items_schema, item, &format!("{path}[{index}]"))?;
                }
            }
        }
        Value::String(text) => {
            let length = text.chars().count() as u64;
            if let Some(min_length) = object.get("minLength").and_then(Value::as_u64)
                && length < min_length
            {
                return Err(SchemaError::Bound {
                    path: path.to_string(),
                    message: format!("needs at least {min_length} characters, found {length}"),
                });
            }
            if let Some(max_length) = object.get("maxLength").and_then(Value::as_u64)
                && length > max_length
            {
                return Err(SchemaError::Bound {
                    path: path.to_string(),
                    message: format!("allows at most {max_length} characters, found {length}"),
                });
            }
        }
        Value::Number(number) => {
            let value = number.as_f64().unwrap_or(f64::NAN);
            if let Some(minimum) = object.get("minimum").and_then(Value::as_f64)
                && value < minimum
            {
                return Err(SchemaError::Bound {
                    path: path.to_string(),
                    message: format!("must be at least {minimum}, found {number}"),
                });
            }
            if let Some(maximum) = object.get("maximum").and_then(Value::as_f64)
                && value > maximum
            {
                return Err(SchemaError::Bound {
                    path: path.to_string(),
                    message: format!("must be at most {maximum}, found {number}"),
                });
            }
        }
        _ => {}
    }

    // Every other keyword is refused: an unenforceable constraint in a contract is a lie waiting
    // to happen.
    for key in object.keys() {
        if !SUPPORTED.contains(&key.as_str()) {
            return Err(SchemaError::Unsupported {
                path: path.to_string(),
                keyword: key.clone(),
            });
        }
    }
    Ok(())
}

/// `type` may be a single name or an array of names; anything else is an unsupported schema.
fn check_type(expected: &Value, value: &Value, path: &str) -> Result<(), SchemaError> {
    let found = type_name(value);
    let matches = |name: &str| {
        if name == "number" {
            matches!(value, Value::Number(_))
        } else if name == "integer" {
            matches!(value, Value::Number(n) if n.is_i64() || n.is_u64())
        } else {
            type_name(value) == name
        }
    };
    match expected {
        Value::String(name) if matches(name) => Ok(()),
        Value::String(name) => Err(SchemaError::Type {
            path: path.to_string(),
            expected: name.clone(),
            found: found.to_string(),
        }),
        Value::Array(names) if names.iter().any(|n| n.as_str().is_some_and(matches)) => Ok(()),
        Value::Array(names) => Err(SchemaError::Type {
            path: path.to_string(),
            expected: names
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join("|"),
            found: found.to_string(),
        }),
        other => Err(SchemaError::Message {
            path: path.to_string(),
            message: format!("\"type\" must be a string or array, found {other}"),
        }),
    }
}

/// Validate an agent's *declaration* schemas themselves: both must be object schemas, or the
/// contract would claim constraints the runtime can check but never does.
pub fn validate_schema_declaration(schema: &Value) -> Result<(), SchemaError> {
    match schema.get("type").and_then(Value::as_str) {
        Some("object") => Ok(()),
        Some(other) => Err(SchemaError::Message {
            path: "schema".to_string(),
            message: format!("agent I/O schemas must be object schemas, found type {other:?}"),
        }),
        None => Err(SchemaError::Message {
            path: "schema".to_string(),
            message: "agent I/O schemas need a \"type\"".to_string(),
        }),
    }
}

/// Recursively reject keywords the runtime does not enforce, anywhere in a declared schema.
///
/// [`validate`] refuses an unknown keyword when it *checks a value*; this walks the declaration
/// up front so the same keyword also fails at registration time, before any input arrives.
pub fn check_keywords(schema: &Value, path: &str) -> Result<(), SchemaError> {
    if matches!(schema, Value::Bool(true)) {
        return Ok(());
    }
    let object = match schema.as_object() {
        Some(object) => object,
        None => {
            return Err(SchemaError::Message {
                path: path.to_string(),
                message: format!("schema must be an object, found {}", type_name(schema)),
            });
        }
    };
    for key in object.keys() {
        if !SUPPORTED.contains(&key.as_str()) {
            return Err(SchemaError::Unsupported {
                path: path.to_string(),
                keyword: key.clone(),
            });
        }
    }
    if let Some(properties) = object.get("properties").and_then(Value::as_object) {
        for (key, sub) in properties {
            let child = if path.is_empty() {
                key.clone()
            } else {
                format!("{path}.{key}")
            };
            check_keywords(sub, &child)?;
        }
    }
    if let Some(items) = object.get("items") {
        check_keywords(items, &format!("{path}[]"))?;
    }
    Ok(())
}

/// Everything a contract's I/O schema must satisfy before it can be registered: an object schema
/// whose keywords are all enforceable.
pub fn validate_declaration(schema: &Value) -> Result<(), SchemaError> {
    validate_schema_declaration(schema)?;
    check_keywords(schema, "schema")
}

#[cfg(test)]
mod tests {
    use super::{SchemaError, validate, validate_schema_declaration};
    use serde_json::json;

    /// Assert a validation failed and return the error, without `expect_err` (the workspace
    /// denies `expect_used`, including in tests).
    fn want_schema_err(result: Result<(), SchemaError>, what: &str) -> SchemaError {
        match result {
            Ok(()) => SchemaError::Message {
                path: String::new(),
                message: format!("expected {what} to fail"),
            },
            Err(err) => err,
        }
    }

    #[test]
    fn object_schema_checks_type_and_required() {
        let schema = json!({
            "type": "object",
            "required": ["shape"],
            "properties": {
                "shape": {"type": "string", "minLength": 1},
                "count": {"type": "integer", "minimum": 1}
            }
        });
        assert!(validate(&schema, &json!({"shape": "box", "count": 3}), "").is_ok());
        let err = want_schema_err(
            validate(&schema, &json!({"count": 3}), ""),
            "missing required",
        );
        assert!(matches!(err, SchemaError::Missing { .. }));
        let err = want_schema_err(
            validate(&schema, &json!({"shape": "", "count": 3}), ""),
            "empty string",
        );
        assert!(matches!(err, SchemaError::Bound { .. }));
        let err = want_schema_err(
            validate(&schema, &json!({"shape": "box", "count": 0}), ""),
            "below minimum",
        );
        assert!(matches!(err, SchemaError::Bound { .. }));
        let err = want_schema_err(
            validate(&schema, &json!({"shape": 5, "count": 3}), ""),
            "wrong type",
        );
        assert!(matches!(err, SchemaError::Type { .. }));
    }

    #[test]
    fn paths_name_the_offending_property() {
        let schema = json!({
            "type": "object",
            "properties": {"inner": {"type": "object", "required": ["x"]}}
        });
        let err = want_schema_err(validate(&schema, &json!({"inner": {}}), ""), "missing x");
        assert!(err.to_string().contains("inner"), "{err}");
    }

    #[test]
    fn unsupported_keywords_fail_loudly() {
        let schema = json!({"type": "object", "patternProperties": {}});
        let err = want_schema_err(validate(&schema, &json!({}), ""), "unsupported keyword");
        assert!(matches!(err, SchemaError::Unsupported { .. }));
    }

    #[test]
    fn declarations_must_be_object_schemas() {
        assert!(validate_schema_declaration(&json!({"type": "object"})).is_ok());
        assert!(validate_schema_declaration(&json!({"type": "array"})).is_err());
        assert!(validate_schema_declaration(&json!({})).is_err());
    }

    /// The upper bounds of the bounds family: a contract that only says "at least three points"
    /// still admits a million.
    #[test]
    fn upper_bounds_are_enforced() {
        let schema = json!({
            "type": "object",
            "properties": {
                "points": {
                    "type": "array",
                    "minItems": 3,
                    "maxItems": 4,
                    "items": {"type": "array", "minItems": 2, "maxItems": 2, "items": {"type": "number"}}
                },
                "name": {"type": "string", "minLength": 1, "maxLength": 4},
                "height": {"type": "number", "minimum": 0.001, "maximum": 100.0}
            }
        });

        // Inside every bound.
        assert!(
            validate(
                &schema,
                &json!({"points": [[0, 0], [1, 0], [1, 1]], "name": "ab", "height": 10.0}),
                ""
            )
            .is_ok()
        );

        // Too many array items.
        let err = want_schema_err(
            validate(
                &schema,
                &json!({"points": [[0, 0], [1, 0], [1, 1], [2, 1], [2, 2]]}),
                "",
            ),
            "too many points",
        );
        assert!(matches!(err, SchemaError::Bound { .. }));

        // A point with three coordinates, where two were declared.
        let err = want_schema_err(
            validate(&schema, &json!({"points": [[0, 0, 0], [1, 0], [1, 1]]}), ""),
            "point with three coordinates",
        );
        assert!(matches!(err, SchemaError::Bound { .. }));

        // A string that is too long.
        let err = want_schema_err(
            validate(&schema, &json!({"name": "abcde"}), ""),
            "name too long",
        );
        assert!(matches!(err, SchemaError::Bound { .. }));

        // A number above the declared maximum.
        let err = want_schema_err(
            validate(&schema, &json!({"height": 1000.0}), ""),
            "height above maximum",
        );
        assert!(matches!(err, SchemaError::Bound { .. }));
    }

    /// The new keywords are enforceable, so a contract may declare them.
    #[test]
    fn the_upper_bound_keywords_are_supported() {
        let schema = json!({
            "type": "object",
            "properties": {"points": {"type": "array", "maxItems": 2}}
        });
        assert!(super::check_keywords(&schema, "schema").is_ok());
    }
}
