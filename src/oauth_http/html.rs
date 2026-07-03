/// Minimal HTML-escaping for interpolating untrusted text into an HTML page.
fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

/// Render the minimal authorize login page. `return_to` is the original
/// `/oauth/authorize?...` path+query the user navigated to. It is rendered
/// into a hidden field and revalidated on POST.
pub(super) fn authorize_login_html(return_to: &str, error: Option<&str>) -> String {
    let error_html = match error {
        Some(msg) => format!(r#"<p class="error">{}</p>"#, html_escape(msg)),
        None => String::new(),
    };
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>WebCodex Authorize</title>
</head>
<body>
<h1>WebCodex Authorization</h1>
<p>Sign in with a WebCodex PAT to continue.</p>
{error_html}
<form method="post" action="/oauth/authorize/login">
  <input type="hidden" name="return_to" value="{return_to}">
  <label>WebCodex token<br>
    <input type="password" name="token" autocomplete="current-password" required>
  </label>
  <button type="submit">Continue</button>
</form>
</body>
</html>"#,
        return_to = html_escape(return_to),
        error_html = error_html,
    )
}

/// Render the minimal authorize consent page. The original authorize query
/// parameters are carried as hidden fields and revalidated on POST.
pub(super) fn authorize_consent_html(
    client_name: &str,
    client_id: &str,
    redirect_uri: &str,
    scopes: &[String],
    resource: Option<&str>,
    original_query: &str,
) -> String {
    let scope_items = scopes
        .iter()
        .map(|s| format!("<li>{}</li>", html_escape(s)))
        .collect::<Vec<_>>()
        .join("\n");
    let resource_html = resource
        .map(|r| format!("<p>Resource: <code>{}</code></p>", html_escape(r)))
        .unwrap_or_default();
    // Re-render the original authorize query as hidden form fields so the
    // consent POST can revalidate every parameter from scratch.
    let hidden_fields: String = url::form_urlencoded::parse(original_query.as_bytes())
        .map(|(k, v)| {
            format!(
                r#"  <input type="hidden" name="{}" value="{}">"#,
                html_escape(k.as_ref()),
                html_escape(v.as_ref())
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Authorize WebCodex client</title>
</head>
<body>
<h1>Authorize WebCodex client</h1>
<p>Client: <strong>{client_name}</strong> ({client_id})</p>
<p>Redirect URI: <code>{redirect_uri}</code></p>
{resource_html}
<p>The application is requesting the following scopes:</p>
<ul>
{scope_items}
</ul>
<form method="post" action="/oauth/authorize/consent">
{hidden_fields}
  <button name="decision" value="allow">Allow</button>
  <button name="decision" value="deny">Deny</button>
</form>
</body>
</html>"#,
        client_name = html_escape(client_name),
        client_id = html_escape(client_id),
        redirect_uri = html_escape(redirect_uri),
        resource_html = resource_html,
        scope_items = scope_items,
        hidden_fields = hidden_fields,
    )
}

pub(super) fn authorize_bridge_html(
    client_name: &str,
    client_id: &str,
    redirect_uri: &str,
    scopes: &[String],
    resource: Option<&str>,
    original_query: &str,
    error: Option<&str>,
) -> String {
    let scope_items = scopes
        .iter()
        .map(|s| format!("<li>{}</li>", html_escape(s)))
        .collect::<Vec<_>>()
        .join("\n");
    let resource_html = resource
        .map(|r| format!("<p>Resource: <code>{}</code></p>", html_escape(r)))
        .unwrap_or_default();
    let error_html = match error {
        Some(msg) => format!(r#"<p class="error">{}</p>"#, html_escape(msg)),
        None => String::new(),
    };
    let hidden_fields: String = url::form_urlencoded::parse(original_query.as_bytes())
        .map(|(k, v)| {
            format!(
                r#"  <input type="hidden" name="{}" value="{}">"#,
                html_escape(k.as_ref()),
                html_escape(v.as_ref())
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Authorize WebCodex shared key</title>
</head>
<body>
<h1>Authorize WebCodex shared key</h1>
<p>Client: <strong>{client_name}</strong> ({client_id})</p>
<p>Redirect URI: <code>{redirect_uri}</code></p>
{resource_html}
<p>The application is requesting the following scopes:</p>
<ul>
{scope_items}
</ul>
{error_html}
<form method="post" action="/oauth/authorize/bridge">
{hidden_fields}
  <label>Shared key<br>
    <input type="password" name="shared_key" autocomplete="current-password" required>
  </label>
  <button type="submit">Continue</button>
</form>
</body>
</html>"#,
        client_name = html_escape(client_name),
        client_id = html_escape(client_id),
        redirect_uri = html_escape(redirect_uri),
        resource_html = resource_html,
        scope_items = scope_items,
        error_html = error_html,
        hidden_fields = hidden_fields,
    )
}
