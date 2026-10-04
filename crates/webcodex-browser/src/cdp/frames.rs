//! Same-origin frame discovery. Native identifiers never leave the backend.
use super::*;
use sha2::{Digest, Sha256};

fn origin(frame: &Value) -> Option<url::Origin> {
    let url = Url::parse(frame.get("securityOrigin")?.as_str()?).ok()?;
    matches!(url.scheme(), "http" | "https").then(|| url.origin())
}

fn documents<'a>(node: &'a Value, output: &mut HashMap<String, &'a Value>) {
    if let (Some(id), Some(document)) = (
        node.get("frameId").and_then(Value::as_str),
        node.get("contentDocument"),
    ) {
        // Opaque sandbox origins must never acquire authority from their URL.
        let sandboxed = dom_attribute(node, "sandbox")
            .is_some_and(|v| !v.split_ascii_whitespace().any(|v| v == "allow-same-origin"));
        if !sandboxed
            && document
                .get("backendNodeId")
                .and_then(Value::as_i64)
                .is_some()
        {
            output.insert(id.to_owned(), document);
            documents(document, output);
        }
    }
    for key in ["children", "shadowRoots"] {
        if let Some(children) = node.get(key).and_then(Value::as_array) {
            for child in children {
                documents(child, output);
            }
        }
    }
}

pub(super) fn frame_documents<'a>(tree: &Value, root: &'a Value) -> Vec<(String, &'a Value)> {
    fn visit<'a>(
        tree: &Value,
        top_origin: &url::Origin,
        dom: &HashMap<String, &'a Value>,
        out: &mut Vec<(String, &'a Value)>,
    ) {
        for child in tree
            .get("childFrames")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let frame = &child["frame"];
            if origin(frame).as_ref() != Some(top_origin) {
                continue;
            }
            let Some(id) = frame.get("id").and_then(Value::as_str) else {
                continue;
            };
            if frame
                .get("loaderId")
                .and_then(Value::as_str)
                .is_none_or(str::is_empty)
            {
                continue;
            }
            let Some(document) = dom.get(id) else {
                continue;
            };
            out.push((id.to_owned(), *document));
            visit(child, top_origin, dom, out);
        }
    }
    let mut out = Vec::new();
    let tree = &tree["frameTree"];
    let Some(top_origin) = origin(&tree["frame"]) else {
        return out;
    };
    if tree["frame"]["id"].as_str().is_none()
        || tree["frame"]["loaderId"].as_str().is_none_or(str::is_empty)
        || root["backendNodeId"].as_i64().is_none()
    {
        return out;
    }
    let mut dom = HashMap::new();
    documents(root, &mut dom);
    visit(tree, &top_origin, &dom, &mut out);
    out
}

/// Conservatively fence the complete frame topology and document incarnations.
/// This also catches document.open/replacement without a loader change. URL/text
/// changes are deliberately absent: ordinary edits do not revoke sibling IDs.
pub(super) fn frame_fence(tree: &Value, root: &Value) -> String {
    fn identities(tree: &Value, out: &mut Vec<Value>) {
        let frame = &tree["frame"];
        out.push(json!([
            frame["id"],
            frame["parentId"],
            frame["loaderId"],
            frame["securityOrigin"]
        ]));
        for child in tree
            .get("childFrames")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            identities(child, out);
        }
    }
    let mut ids = Vec::new();
    identities(&tree["frameTree"], &mut ids);
    let mut dom = HashMap::new();
    documents(root, &mut dom);
    let mut docs = dom
        .into_iter()
        .map(|(id, doc)| (id, doc["backendNodeId"].clone()))
        .collect::<Vec<_>>();
    docs.sort_by(|a, b| a.0.cmp(&b.0));
    let encoded =
        serde_json::to_vec(&(ids, &root["backendNodeId"], docs)).expect("JSON identities");
    format!("{:x}", Sha256::digest(encoded))
}

#[cfg(test)]
#[path = "tests/frames.rs"]
mod tests;
