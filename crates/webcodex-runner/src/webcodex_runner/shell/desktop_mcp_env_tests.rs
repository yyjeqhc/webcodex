use super::*;

#[test]
fn desktop_mcp_sources_are_not_ambient_job_environment_but_remain_explicitly_mappable() {
    for key in [
        "WEBPI_DESKTOP_MCP_PROVIDER_0",
        "WebCodex_Desktop_Mcp_Provider_1",
    ] {
        assert!(!should_inherit_env_key(key));
        // The MCP gateway resolves explicit source names directly from its
        // parent environment; these are not transport/account credentials.
        assert!(!is_sensitive_env_key(key));
    }
    assert!(should_inherit_env_key("PATH"));
    assert!(should_inherit_env_key("HOME"));
    assert!(!should_inherit_env_key("WEBPI_TOKEN"));
    assert!(is_sensitive_env_key("WEBPI_TOKEN"));
}
