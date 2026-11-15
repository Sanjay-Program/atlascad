//! AtlasCAD Universal Command Platform
//!
//! Provides a typed, machine-readable command contracts and registry with
//! discovery, validation, permission checks, execution tracking, and audit logging.

#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

pub mod builtin;
pub mod error;
pub mod registry;
pub mod schema;

pub use builtin::register_builtin_commands;
pub use error::CommandError;
pub use registry::{CommandFilter, CommandRegistry, ExecutedCommandRecord};
pub use schema::{
    ArgumentType, CommandArgument, CommandCategory, CommandPermission, CommandSchema,
    ResourceRequirements, ValidationRule,
};

#[must_use]
pub fn create_default_registry() -> CommandRegistry {
    let mut reg = CommandRegistry::new();
    let _ = register_builtin_commands(&mut reg);
    reg
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_registration_and_discovery() -> Result<(), CommandError> {
        let reg = create_default_registry();
        assert!(!reg.is_empty(), "Builtin commands should be registered");
        assert!(reg.len() >= 12, "At least 12 standard commands registered");

        // Test discovery by category
        let cad_cmds = reg.discover(Some(&CommandFilter {
            category: Some(CommandCategory::Features),
            required_permission: None,
            query: None,
        }));
        assert!(!cad_cmds.is_empty());

        // Test discovery by permission
        let sim_cmds = reg.discover(Some(&CommandFilter {
            category: None,
            required_permission: Some(CommandPermission::SimulationRun),
            query: None,
        }));
        assert!(!sim_cmds.is_empty());

        // Test text search
        let aero_cmds = reg.discover(Some(&CommandFilter {
            category: None,
            required_permission: None,
            query: Some("aerodynamics".into()),
        }));
        assert_eq!(aero_cmds.len(), 1);
        assert_eq!(aero_cmds[0].id, "sim.aerodynamics.solve");

        Ok(())
    }

    #[test]
    fn test_argument_validation() -> Result<(), CommandError> {
        let reg = create_default_registry();

        // Valid arguments
        let valid_args = serde_json::json!({
            "airspeed_m_s": 75.0,
            "angle_of_attack_deg": 3.0
        });
        let res = reg.validate_call(
            "sim.aerodynamics.solve",
            &valid_args,
            &[CommandPermission::SimulationRun],
        );
        assert!(res.is_ok());

        // Permission denied
        let perm_denied = reg.validate_call(
            "sim.aerodynamics.solve",
            &valid_args,
            &[CommandPermission::ReadOnly],
        );
        assert!(matches!(
            perm_denied,
            Err(CommandError::PermissionDenied { .. })
        ));

        // Invalid argument rule (airspeed below min)
        let invalid_args = serde_json::json!({
            "airspeed_m_s": -10.0,
        });
        let val_failed = reg.validate_call(
            "sim.aerodynamics.solve",
            &invalid_args,
            &[CommandPermission::SimulationRun],
        );
        assert!(matches!(
            val_failed,
            Err(CommandError::InvalidArgument { .. })
        ));

        Ok(())
    }
}
