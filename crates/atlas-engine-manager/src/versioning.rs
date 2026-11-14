//! Engine version extraction and compatibility.

pub struct VersionDetector;

impl VersionDetector {
    /// Extracts semantic version string from common solver output strings.
    pub fn extract_semver(raw: &str) -> Option<String> {
        let parts: Vec<&str> = raw.split_whitespace().collect();
        for p in parts {
            let clean = p.trim_matches(|c: char| !c.is_numeric() && c != '.');
            if !clean.is_empty() && clean.chars().next().is_some_and(|c| c.is_numeric()) {
                return Some(clean.to_string());
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_extraction() {
        assert_eq!(
            VersionDetector::extract_semver("CalculiX Version 2.21, build 2023"),
            Some("2.21".to_string())
        );
        assert_eq!(
            VersionDetector::extract_semver("OpenFOAM-v11 patch release"),
            Some("11".to_string())
        );
    }
}
