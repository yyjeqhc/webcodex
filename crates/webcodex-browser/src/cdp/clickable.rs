//! Conservative click-only admission from browser event evidence and DOM/layout facts.
//! No selector, script or listener body crosses the model boundary.
use super::*;

const MAX_CLICK_SCAN_NODES: usize = crate::MAX_SNAPSHOT_OFFSET + MAX_SNAPSHOT_NODES;
const MAX_CLICK_CARDS: usize = 32;

/// Preflight the already depth-bounded DOM response before asking Chromium for
/// layout/event evidence. Refuse incomplete trees and unknown frame contents;
/// the response cap and deadline still guard changes racing this observation.
pub(super) fn capture_source_is_bounded(root: &Value) -> bool {
    fn visit(node: &Value, remaining: &mut usize, depth: usize) -> bool {
        if *remaining == 0 || depth > 64 {
            return false;
        }
        *remaining -= 1;
        let children = node.get("children").and_then(Value::as_array);
        if matches!(
            node.get("nodeType").and_then(Value::as_i64),
            Some(1 | 9 | 11)
        ) && node.get("childNodeCount").and_then(Value::as_u64)
            != Some(children.map_or(0, Vec::len) as u64)
        {
            return false;
        }
        if matches!(
            node.get("localName").and_then(Value::as_str),
            Some("iframe" | "frame")
        ) && node.get("contentDocument").is_none()
        {
            return false;
        }
        for key in ["children", "shadowRoots", "pseudoElements"] {
            if let Some(children) = node.get(key).and_then(Value::as_array) {
                if children.len() > *remaining
                    || !children
                        .iter()
                        .all(|child| visit(child, remaining, depth + 1))
                {
                    return false;
                }
            }
        }
        node.get("contentDocument")
            .is_none_or(|document| visit(document, remaining, depth + 1))
    }
    let mut remaining = MAX_CLICK_SCAN_NODES;
    visit(root, &mut remaining, 0)
}

pub(super) fn admit_clickable_cards(nodes: &mut [BackendNode], root: &Value, capture: &Value) {
    let Some(strings) = capture.get("strings").and_then(Value::as_array) else {
        return;
    };
    let Some(document) = capture
        .get("documents")
        .and_then(Value::as_array)
        .and_then(|v| v.first())
    else {
        return;
    };
    let Some(ids) = document
        .pointer("/nodes/backendNodeId")
        .and_then(Value::as_array)
    else {
        return;
    };
    // No prefix admission from an incomplete topology: a nested control may be in the tail.
    let Some(root_id) = root.get("backendNodeId").and_then(Value::as_i64) else {
        return;
    };
    if ids.len() > MAX_CLICK_SCAN_NODES || ids.first().and_then(Value::as_i64) != Some(root_id) {
        return;
    }
    let Some(clicks) = document
        .pointer("/nodes/isClickable/index")
        .and_then(Value::as_array)
    else {
        return;
    };
    if clicks.len() > ids.len() {
        return;
    }
    let Some(clickable) = clicks
        .iter()
        .map(|i| {
            i.as_u64()
                .and_then(|i| usize::try_from(i).ok())
                .and_then(|i| ids.get(i))
                .and_then(Value::as_i64)
        })
        .collect::<Option<HashSet<i64>>>()
    else {
        return;
    };
    let Some(layout) = document.get("layout") else {
        return;
    };
    let Some(indices) = layout.get("nodeIndex").and_then(Value::as_array) else {
        return;
    };
    if indices.len() > MAX_CLICK_SCAN_NODES {
        return;
    }
    let string = |value: Option<&Value>| {
        value
            .and_then(Value::as_u64)
            .and_then(|i| strings.get(i as usize))
            .and_then(Value::as_str)
    };
    let mut visible = HashSet::new();
    let mut rendered = HashSet::new();
    let mut invisible = HashSet::new();
    for (i, index) in indices.iter().enumerate() {
        let Some(id) = index
            .as_u64()
            .and_then(|i| ids.get(i as usize))
            .and_then(Value::as_i64)
        else {
            continue;
        };
        let Some(styles) = layout
            .get("styles")
            .and_then(|v| v.get(i))
            .and_then(Value::as_array)
        else {
            invisible.insert(id);
            continue;
        };
        let Some(bounds) = layout
            .get("bounds")
            .and_then(|v| v.get(i))
            .and_then(Value::as_array)
        else {
            invisible.insert(id);
            continue;
        };
        let rendered_here = string(styles.get(1)) == Some("visible")
            && string(styles.get(2)).is_some_and(|v| v != "none")
            && string(styles.get(4))
                .and_then(|v| v.parse::<f64>().ok())
                .is_some_and(|v| v > 0.0)
            && bounds
                .get(2)
                .and_then(Value::as_f64)
                .is_some_and(|v| v > 0.0)
            && bounds
                .get(3)
                .and_then(Value::as_f64)
                .is_some_and(|v| v > 0.0);
        if rendered_here {
            rendered.insert(id);
        } else {
            invisible.insert(id);
        }
        if clickable.contains(&id)
            && rendered_here
            && string(styles.first()) == Some("pointer")
            && string(styles.get(3)) == Some("auto")
        {
            visible.insert(id);
        }
    }
    let mut candidates = HashMap::new();
    let mut remaining = MAX_CLICK_SCAN_NODES;
    collect_candidates(
        root,
        false,
        &visible,
        &rendered,
        &invisible,
        &clickable,
        &mut candidates,
        &mut remaining,
    );
    if remaining == 0 {
        return;
    }
    let mut admitted = 0;
    for node in nodes.iter_mut().take(MAX_CLICK_SCAN_NODES) {
        if admitted >= MAX_CLICK_CARDS {
            break;
        }
        if node.capability.admits_any()
            || node.frame_fence.is_some()
            || !matches!(
                node.role.as_str(),
                "generic" | "group" | "listitem" | "article"
            )
            || node.disabled == Some(true)
        {
            continue;
        }
        let Some(label) = node.backend_node_id.and_then(|id| candidates.get(&id)) else {
            continue;
        };
        node.name = Some(label.clone());
        node.capability = ControlCapability::click();
        admitted += 1;
    }
}

fn collect_candidates(
    node: &Value,
    blocked: bool,
    visible: &HashSet<i64>,
    rendered: &HashSet<i64>,
    invisible: &HashSet<i64>,
    clickable: &HashSet<i64>,
    candidates: &mut HashMap<i64, String>,
    remaining: &mut usize,
) {
    if *remaining == 0 {
        return;
    }
    *remaining -= 1;
    let blocked = blocked
        || node.get("shadowRootType").is_some()
        || node
            .get("backendNodeId")
            .and_then(Value::as_i64)
            .is_some_and(|id| invisible.contains(&id))
        || dom_attribute(node, "contenteditable").is_some()
        || dom_attribute(node, "inert").is_some()
        || dom_attribute(node, "hidden").is_some()
        || dom_attribute(node, "disabled").is_some()
        || dom_attribute(node, "aria-disabled").as_deref() == Some("true")
        || dom_attribute(node, "aria-hidden").as_deref() == Some("true");
    if blocked {
        return;
    }
    let tag = node.get("localName").and_then(Value::as_str).unwrap_or("");
    if matches!(tag, "div" | "li" | "article") {
        if let Some(id) = node
            .get("backendNodeId")
            .and_then(Value::as_i64)
            .filter(|id| visible.contains(id))
        {
            // Both actual click response and pointer affordance are required. A container
            // with another target, unknown shadow contents or truncated children is ambiguous.
            let mut budget = 64;
            let mut text_budget = MAX_NODE_TEXT_BYTES - 1;
            if safe_content(
                node,
                rendered,
                clickable,
                &mut budget,
                &mut text_budget,
                true,
            ) {
                if let Some(label) = dom_text_content(node)
                    .filter(|label| !label.trim().is_empty() && label.len() < MAX_NODE_TEXT_BYTES)
                {
                    candidates.insert(id, label);
                }
            }
        }
    }
    if let Some(children) = node.get("children").and_then(Value::as_array) {
        let ancestor_clickable = node
            .get("backendNodeId")
            .and_then(Value::as_i64)
            .is_some_and(|id| clickable.contains(&id));
        for child in children {
            if *remaining == 0 {
                break;
            }
            collect_candidates(
                child,
                ancestor_clickable,
                visible,
                rendered,
                invisible,
                clickable,
                candidates,
                remaining,
            );
        }
    }
    // Do not enter shadow roots or frame documents; their authority has separate owners.
}

fn safe_content(
    node: &Value,
    rendered: &HashSet<i64>,
    clickable: &HashSet<i64>,
    budget: &mut usize,
    text_budget: &mut usize,
    owner: bool,
) -> bool {
    if *budget == 0 {
        return false;
    }
    *budget -= 1;
    if node.get("nodeType").and_then(Value::as_i64) == Some(3) {
        let bytes = node
            .get("nodeValue")
            .and_then(Value::as_str)
            .map_or(0, str::len);
        if bytes > *text_budget {
            return false;
        }
        *text_budget -= bytes;
    }
    if dom_attribute(node, "hidden").is_some()
        || dom_attribute(node, "aria-hidden").as_deref() == Some("true")
    {
        return false;
    }
    if node.get("shadowRoots").is_some() || node.get("contentDocument").is_some() {
        return false;
    }
    if !owner && node.get("nodeType").and_then(Value::as_i64) == Some(1) {
        let tag = node.get("localName").and_then(Value::as_str).unwrap_or("");
        if !matches!(tag, "script" | "style" | "noscript")
            && !node
                .get("backendNodeId")
                .and_then(Value::as_i64)
                .is_some_and(|id| rendered.contains(&id))
        {
            return false;
        }
    }
    if !owner {
        let tag = node.get("localName").and_then(Value::as_str).unwrap_or("");
        if matches!(
            tag,
            "a" | "button" | "input" | "select" | "textarea" | "summary" | "iframe"
        ) || dom_attribute(node, "role").is_some()
            || dom_attribute(node, "tabindex").is_some()
            || dom_attribute(node, "contenteditable").is_some()
            || node
                .get("backendNodeId")
                .and_then(Value::as_i64)
                .is_some_and(|id| clickable.contains(&id))
        {
            return false;
        }
    }
    let children = node.get("children").and_then(Value::as_array);
    if node
        .get("childNodeCount")
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize
        > children.map_or(0, Vec::len)
    {
        return false;
    }
    children.is_none_or(|children| {
        children
            .iter()
            .all(|child| safe_content(child, rendered, clickable, budget, text_budget, false))
    })
}

#[cfg(test)]
#[path = "tests/clickable.rs"]
mod tests;
