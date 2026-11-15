//! Command registry and discovery engine for AtlasCAD.

use crate::error::CommandError;
use crate::schema::{
    ArgumentType, CommandCategory, CommandPermission, CommandSchema, ValidationRule,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecutedCommandRecord {
    pub execution_id: String,
    pub command_id: String,
    pub caller_identity: String,
    pub arguments: serde_json::Value,
    pub status: String,
    pub execution_time_ms: u64,
    pub timestamp_epoch_ms: u64,
    pub audit_hash: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CommandFilter {
    pub category: Option<CommandCategory>,
    pub required_permission: Option<CommandPermission>,
    pub query: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CommandRegistry {
    commands: BTreeMap<String, CommandSchema>,
    history: Vec<ExecutedCommandRecord>,
}

impl CommandRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self {
            commands: BTreeMap::new(),
            history: Vec::new(),
        }
    }

    pub fn register(&mut self, schema: CommandSchema) -> Result<(), CommandError> {
        let id = schema.id.clone();
        if self.commands.contains_key(&id) {
            return Err(CommandError::Internal(format!(
                "Command '{id}' already registered"
            )));
        }
        self.commands.insert(id, schema);
        Ok(())
    }

    #[must_use]
    pub fn get(&self, id: &str) -> Option<&CommandSchema> {
        self.commands.get(id)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.commands.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    #[must_use]
    pub fn discover(&self, filter: Option<&CommandFilter>) -> Vec<CommandSchema> {
        self.commands
            .values()
            .filter(|cmd| {
                if let Some(f) = filter {
                    if f.category.is_some_and(|cat| cmd.category != cat) {
                        return false;
                    }
                    if f.required_permission
                        .is_some_and(|perm| !cmd.required_permissions.contains(&perm))
                    {
                        return false;
                    }
                    if let Some(ref q) = f.query {
                        let query = q.to_lowercase();
                        if !cmd.id.to_lowercase().contains(&query)
                            && !cmd.name.to_lowercase().contains(&query)
                            && !cmd.description.to_lowercase().contains(&query)
                        {
                            return false;
                        }
                    }
                }
                true
            })
            .cloned()
            .collect()
    }

    pub fn validate_call(
        &self,
        command_id: &str,
        args: &serde_json::Value,
        caller_permissions: &[CommandPermission],
    ) -> Result<&CommandSchema, CommandError> {
        let schema = self
            .commands
            .get(command_id)
            .ok_or_else(|| CommandError::CommandNotFound(command_id.to_string()))?;

        // Check permissions
        for req_perm in &schema.required_permissions {
            if !caller_permissions.contains(req_perm)
                && !caller_permissions.contains(&CommandPermission::Admin)
            {
                return Err(CommandError::PermissionDenied {
                    required: req_perm.as_str().to_string(),
                    held: format!("{caller_permissions:?}"),
                });
            }
        }

        // Validate arguments
        let args_obj = args.as_object().ok_or_else(|| {
            CommandError::ValidationFailed("Command arguments must be a JSON object".to_string())
        })?;

        for arg_def in &schema.arguments {
            match args_obj.get(&arg_def.name) {
                Some(val) if !val.is_null() => {
                    Self::validate_arg_type(arg_def, val)?;
                    Self::validate_arg_rules(arg_def, val)?;
                }
                _ => {
                    if arg_def.required && arg_def.default_value.is_none() {
                        return Err(CommandError::MissingRequiredArgument(arg_def.name.clone()));
                    }
                }
            }
        }

        Ok(schema)
    }

    fn validate_arg_type(
        def: &crate::schema::CommandArgument,
        val: &serde_json::Value,
    ) -> Result<(), CommandError> {
        let valid = match def.arg_type {
            ArgumentType::String => val.is_string(),
            ArgumentType::Number => val.is_number(),
            ArgumentType::Integer => {
                val.is_i64() || (val.is_f64() && val.as_f64().is_some_and(|f| f.fract() == 0.0))
            }
            ArgumentType::Boolean => val.is_boolean(),
            ArgumentType::Array => val.is_array(),
            ArgumentType::Object => val.is_object(),
        };

        if !valid {
            return Err(CommandError::InvalidArgument {
                name: def.name.clone(),
                reason: format!("Expected {:?}, received {:?}", def.arg_type, val),
            });
        }
        Ok(())
    }

    fn validate_arg_rules(
        def: &crate::schema::CommandArgument,
        val: &serde_json::Value,
    ) -> Result<(), CommandError> {
        for rule in &def.validation_rules {
            match rule {
                ValidationRule::Min(min) => {
                    if let Some(num) = val.as_f64().filter(|&n| n < *min) {
                        return Err(CommandError::InvalidArgument {
                            name: def.name.clone(),
                            reason: format!("Value {num} is below minimum {min}"),
                        });
                    }
                }
                ValidationRule::Max(max) => {
                    if let Some(num) = val.as_f64().filter(|&n| n > *max) {
                        return Err(CommandError::InvalidArgument {
                            name: def.name.clone(),
                            reason: format!("Value {num} exceeds maximum {max}"),
                        });
                    }
                }
                ValidationRule::NonEmpty => {
                    if let Some(s) = val.as_str() {
                        if s.trim().is_empty() {
                            return Err(CommandError::InvalidArgument {
                                name: def.name.clone(),
                                reason: "String must not be empty".to_string(),
                            });
                        }
                    } else if let Some(_arr) = val.as_array().filter(|a| a.is_empty()) {
                        return Err(CommandError::InvalidArgument {
                            name: def.name.clone(),
                            reason: "Array must not be empty".to_string(),
                        });
                    }
                }
                ValidationRule::AllowedValues(allowed) => {
                    if let Some(s) = val.as_str().filter(|s| !allowed.iter().any(|a| a == *s)) {
                        return Err(CommandError::InvalidArgument {
                            name: def.name.clone(),
                            reason: format!("Value '{s}' is not one of allowed: {allowed:?}"),
                        });
                    }
                }
            }
        }
        Ok(())
    }

    pub fn record_execution(&mut self, record: ExecutedCommandRecord) {
        self.history.push(record);
    }

    #[must_use]
    pub fn history(&self, limit: usize) -> &[ExecutedCommandRecord] {
        let total = self.history.len();
        if total <= limit {
            &self.history[..]
        } else {
            &self.history[total - limit..]
        }
    }
}
