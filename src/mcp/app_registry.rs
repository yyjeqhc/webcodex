//! Bundled MCP App composition, after the existing capability/authority gates.
//! No runtime registration, external JavaScript, execution or credential access.

mod builtin;
pub(super) use builtin::*;

use serde_json::{json, Value};

pub(super) const MCP_UI_EXTENSION: &str = "io.modelcontextprotocol/ui";
pub(super) const MCP_UI_RESOURCE_MIME_TYPE: &str = "text/html;profile=mcp-app";

struct AppListing {
    name: &'static str,
    description: &'static str,
}

pub(super) struct BundledMcpApp {
    pub uri: &'static str,
    html: &'static str,
    listing: Option<AppListing>,
    tools: &'static [&'static str],
    pub cache_ttl_ms: Option<u64>,
    tool_title: Option<&'static str>,
    tool_entrypoints: &'static [&'static str],
    read_display_modes: &'static [&'static str],
    resource_domains: &'static [&'static str],
}

impl BundledMcpApp {
    const fn template(uri: &'static str, html: &'static str) -> Self {
        Self {
            uri,
            html,
            listing: None,
            tools: &[],
            cache_ttl_ms: None,
            tool_title: None,
            tool_entrypoints: &[],
            read_display_modes: &[],
            resource_domains: &[],
        }
    }

    fn resource_metadata(&self, domain: Option<&str>) -> Value {
        let mut meta = resource_meta(domain);
        if !self.resource_domains.is_empty() {
            meta["ui"]["csp"]["resourceDomains"] = json!(self.resource_domains);
        }
        meta
    }

    pub fn read(&self, domain: Option<&str>) -> Value {
        let mut meta = self.resource_metadata(domain);
        // Display modes were historically resource-read metadata only. Keep
        // resources/list and descriptor metadata separate from this projection.
        if let Some(preferred) = self.read_display_modes.first() {
            meta["openai/ui"] = json!({
                "availableDisplayModes": self.read_display_modes,
                "preferredDisplayMode": preferred,
            });
        }
        json!({"contents": [{
            "uri": self.uri, "mimeType": MCP_UI_RESOURCE_MIME_TYPE,
            "text": self.html, "_meta": meta,
        }]})
    }

    /// Optional presentation extras only. The caller already attached the App
    /// resource to an admitted descriptor; neither lookup nor this method admits it.
    pub fn add_tool_metadata(&self, value: &mut Value) {
        if let Some(title) = self.tool_title {
            value["title"] = json!(title);
        }
        if !self.tool_entrypoints.is_empty() {
            if let Some(meta) = value.get_mut("_meta").and_then(Value::as_object_mut) {
                meta.insert("openai/ui".into(), json!({
                    "entrypoints": self.tool_entrypoints.iter().map(|kind| json!({"type": kind})).collect::<Vec<_>>()
                }));
            }
        }
    }
}

pub(super) fn resource_meta(domain: Option<&str>) -> Value {
    let mut ui = json!({
        "prefersBorder": true,
        "csp": { "connectDomains": [], "resourceDomains": [] },
    });
    if let Some(domain) = domain {
        ui["domain"] = Value::String(domain.to_string());
    }
    json!({ "ui": ui })
}

pub(super) fn for_uri(uri: &str) -> Option<&'static BundledMcpApp> {
    BUILTIN_MCP_APPS.iter().find(|app| app.uri == uri)
}

pub(super) fn for_tool(tool: &str) -> Option<&'static BundledMcpApp> {
    BUILTIN_MCP_APPS
        .iter()
        .find(|app| app.tools.contains(&tool))
}

pub(super) fn resources_list(domain: Option<&str>) -> Value {
    let resources: Vec<_> = BUILTIN_MCP_APPS
        .iter()
        .filter_map(|app| {
            let listing = app.listing.as_ref()?;
            Some(json!({
                "uri": app.uri, "name": listing.name, "description": listing.description,
                "mimeType": MCP_UI_RESOURCE_MIME_TYPE, "_meta": app.resource_metadata(domain),
            }))
        })
        .collect();
    json!({ "resources": resources })
}

#[cfg(test)]
mod tests;
