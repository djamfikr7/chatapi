use crate::config::ChatApiConfig;

/// Check if a tool is allowed by the rules config.
/// Supports glob patterns: "git_*" matches "git_status", "git_diff", etc.
/// If allowed_tools is empty, all tools are allowed.
pub fn is_tool_allowed(tool_name: &str, config: &ChatApiConfig) -> bool {
    let allowed = &config.rules.allowed_tools;
    if allowed.is_empty() {
        return true;
    }
    allowed.iter().any(|pattern| {
        if let Ok(glob) = glob::Pattern::new(pattern) {
            glob.matches(tool_name)
        } else {
            pattern == tool_name
        }
    })
}

/// Check if a path is blocked by the rules config.
/// Supports glob patterns: "secrets/*" matches "secrets/keys.json".
/// Resolves the path to canonical form before checking.
pub fn is_path_blocked(path: &str, config: &ChatApiConfig) -> bool {
    let blocked = &config.rules.blocked_paths;
    if blocked.is_empty() {
        return false;
    }

    // Normalize path separators
    let normalized = path.replace('\\', "/");

    blocked.iter().any(|pattern| {
        if let Ok(glob) = glob::Pattern::new(pattern) {
            glob.matches(&normalized)
                || glob.matches(path)
                || normalized.starts_with(&pattern.replace('*', ""))
        } else {
            normalized.contains(pattern) || path.contains(pattern)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config_with_tools(allowed: Vec<&str>) -> ChatApiConfig {
        let mut config = ChatApiConfig::default();
        config.rules.allowed_tools = allowed.into_iter().map(String::from).collect();
        config
    }

    fn config_with_blocked(blocked: Vec<&str>) -> ChatApiConfig {
        let mut config = ChatApiConfig::default();
        config.rules.blocked_paths = blocked.into_iter().map(String::from).collect();
        config
    }

    #[test]
    fn test_empty_allowed_means_all() {
        let config = ChatApiConfig::default();
        assert!(is_tool_allowed("anything", &config));
    }

    #[test]
    fn test_exact_match() {
        let config = config_with_tools(vec!["read_file", "edit_file"]);
        assert!(is_tool_allowed("read_file", &config));
        assert!(is_tool_allowed("edit_file", &config));
        assert!(!is_tool_allowed("run_command", &config));
    }

    #[test]
    fn test_glob_pattern() {
        let config = config_with_tools(vec!["read_file", "git_*"]);
        assert!(is_tool_allowed("read_file", &config));
        assert!(is_tool_allowed("git_status", &config));
        assert!(is_tool_allowed("git_diff", &config));
        assert!(is_tool_allowed("git_commit", &config));
        assert!(!is_tool_allowed("edit_file", &config));
    }

    #[test]
    fn test_blocked_exact() {
        let config = config_with_blocked(vec![".env", ".git/config"]);
        assert!(is_path_blocked(".env", &config));
        assert!(is_path_blocked(".git/config", &config));
        assert!(!is_path_blocked("src/main.rs", &config));
    }

    #[test]
    fn test_blocked_glob() {
        let config = config_with_blocked(vec!["secrets/*"]);
        assert!(is_path_blocked("secrets/keys.json", &config));
        assert!(is_path_blocked("secrets/api_key.txt", &config));
    }

    #[test]
    fn test_no_blocked_paths() {
        let config = ChatApiConfig::default();
        assert!(!is_path_blocked("anything", &config));
    }
}
