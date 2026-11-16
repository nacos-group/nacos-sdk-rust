use crate::api::error::{Error, Result};

pub(crate) fn ensure_mcp_registry(supported: Option<bool>) -> Result<()> {
    if supported == Some(false) {
        return Err(Error::UnsupportedServerVersion(
            "server does not support the mcp registry feature; nacos-server >= 3.0.3 is required"
                .to_string(),
        ));
    }
    Ok(())
}

pub(crate) fn ensure_mcp_draft_release(supported: Option<bool>) -> Result<()> {
    if supported != Some(true) {
        return Err(Error::UnsupportedServerVersion(
            "server does not support MCP lifecycle draft release; a newer nacos-server is required"
                .to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_registry_is_lenient_when_ability_missing() {
        assert!(ensure_mcp_registry(None).is_ok());
        assert!(ensure_mcp_registry(Some(true)).is_ok());
    }

    #[test]
    fn test_mcp_registry_fails_when_explicitly_disabled() {
        let result = ensure_mcp_registry(Some(false));
        assert!(matches!(result, Err(Error::UnsupportedServerVersion(_))));
    }

    #[test]
    fn test_mcp_draft_release_requires_supported() {
        assert!(ensure_mcp_draft_release(None).is_err());
        assert!(ensure_mcp_draft_release(Some(false)).is_err());
        assert!(ensure_mcp_draft_release(Some(true)).is_ok());
    }
}
