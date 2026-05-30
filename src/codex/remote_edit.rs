use super::edit::edit_error;
use super::shell::shell_escape;
use super::ssh::{build_ssh_command, build_ssh_target};
use super::truncate_string;
use super::types::{EditRequest, EditResponse};
use super::{CHECK_TIMEOUT_SECS, MAX_OUTPUT_LEN};
use crate::projects::{ProjectConfig, SshConfig};
use std::time::Instant;

/// Embedded Python3 script for remote edit operations.
/// Receives project root via argv[1] and edit JSON via stdin.
/// Returns JSON result on stdout.
pub(super) const REMOTE_EDIT_SCRIPT: &str = r#####"
import sys, json, os, difflib, base64, urllib.request, urllib.parse, socket, ipaddress

SENSITIVE = ('.git', '.env', '.pem', '.key', 'id_rsa', 'id_ed25519',
             'target', 'node_modules')
MAX_FILE = 2 * 1024 * 1024
MAX_TEXT = 200 * 1024
MAX_BINARY = 5 * 1024 * 1024
URL_TIMEOUT = 10

def err(msg):
    return {'success': False, 'changed_files': [], 'diff': '', 'warnings': [], 'error': msg}

def is_sensitive(p):
    parts = p.replace('\\\\', '/').split('/')
    for seg in parts:
        if seg in SENSITIVE:
            return True
        for suf in ('.pem', '.key'):
            if seg.endswith(suf):
                return True
    return False

def validate_path(rel):
    if not rel:
        return 'path cannot be empty'
    if rel.startswith('/'):
        return 'Absolute paths are not allowed'
    if '..' in rel:
        return 'Path traversal (..) is not allowed'
    if is_sensitive(rel):
        return 'Cannot modify sensitive path: ' + rel
    return None

def resolve(root, rel, must_exist):
    e = validate_path(rel)
    if e:
        return None, e
    full = os.path.normpath(os.path.join(root, rel))
    canon_root = os.path.realpath(root)
    if not os.path.realpath(full).startswith(canon_root + os.sep) and os.path.realpath(full) != canon_root:
        return None, 'Path is outside project directory'
    if must_exist:
        if not os.path.isfile(full):
            return None, 'File does not exist: ' + rel
    else:
        parent = os.path.dirname(full)
        ancestor = parent
        while ancestor and not os.path.isdir(ancestor):
            next_ancestor = os.path.dirname(ancestor)
            if next_ancestor == ancestor:
                return None, 'path has no existing parent directory'
            ancestor = next_ancestor
        if not os.path.realpath(ancestor).startswith(canon_root + os.sep) and os.path.realpath(ancestor) != canon_root:
            return None, 'Path is outside project directory'
    return full, None

def read_file(path):
    try:
        sz = os.path.getsize(path)
    except OSError as e:
        return None, 'Failed to stat file: ' + str(e)
    if sz > MAX_FILE:
        return None, 'File too large: %d bytes' % sz
    try:
        with open(path, 'r', encoding='utf-8') as f:
            return f.read(), None
    except Exception as e:
        return None, 'Failed to read UTF-8 file: ' + str(e)

def replace_nth(content, old, new, occ):
    if not old:
        return None, 'old_text cannot be empty'
    idxs = []
    start = 0
    while True:
        i = content.find(old, start)
        if i < 0:
            break
        idxs.append(i)
        start = i + len(old)
    if not idxs:
        return None, 'old_text was not found'
    if occ is not None:
        if occ < 1:
            return None, 'occurrence is 1-based and must be >= 1'
        if occ > len(idxs):
            return None, 'occurrence %d exceeds match count %d' % (occ, len(idxs))
        sel = idxs[occ - 1]
    else:
        if len(idxs) > 1:
            return None, 'old_text matched %d times; specify occurrence' % len(idxs)
        sel = idxs[0]
    return content[:sel] + new + content[sel + len(old):], None

def replace_range(content, sl, el, new):
    if sl < 1 or el < 1 or sl > el:
        return None, 'start_line and end_line must be 1-based and start_line <= end_line'
    had_nl = content.endswith('\n')
    lines = content.split('\n')
    # If content ends with \n, split gives an extra empty string at the end
    if had_nl and lines and lines[-1] == '':
        lines = lines[:-1]
    if el > len(lines):
        return None, 'line range %d-%d exceeds file line count %d' % (sl, el, len(lines))
    repl = [] if not new else new.rstrip('\n').split('\n')
    lines2 = lines[:sl-1] + repl + lines[el:]
    out = '\n'.join(lines2)
    if had_nl or new.endswith('\n'):
        out += '\n'
    return out, None

def simple_diff(path, old_content, new_content):
    old_lines = (old_content or '').splitlines(True)
    new_lines = new_content.splitlines(True)
    diff = difflib.unified_diff(old_lines, new_lines, fromfile='a/' + path, tofile='b/' + path)
    return ''.join(diff)

def simple_binary_diff(path, old_len, new_len):
    if old_len is None:
        return 'diff --git a/{0} b/{0}\nnew file mode 100644\nBinary file b/{0} added\n# new size: {1} bytes\n'.format(path, new_len)
    return 'diff --git a/{0} b/{0}\nBinary files a/{0} and b/{0} differ\n# old size: {1} bytes\n# new size: {2} bytes\n'.format(path, old_len, new_len)

def check_binary_size(data, label):
    if len(data) > MAX_BINARY:
        return None, 'binary content for %s exceeds %d bytes' % (label, MAX_BINARY)
    return data, None

def decode_binary(payload, rel):
    if len(payload) > MAX_BINARY * 2:
        return None, 'base64 content for %s is too large; maximum decoded size is %d bytes' % (rel, MAX_BINARY)
    try:
        data = base64.b64decode(payload, validate=True)
    except Exception as e:
        return None, 'Invalid base64 content for %s: %s' % (rel, e)
    return check_binary_size(data, rel)

def read_upload(root, source_file, rel):
    if not source_file:
        return None, 'source_file cannot be empty'
    if '..' in source_file:
        return None, 'source_file path traversal is not allowed'
    if is_sensitive(source_file):
        return None, 'source_file cannot reference a sensitive path'
    full = source_file if os.path.isabs(source_file) else os.path.join(root, source_file)
    try:
        canon = os.path.realpath(full)
    except Exception as e:
        return None, 'Failed to access source_file: %s' % e
    allowed_roots = [root, '/tmp', '/var/tmp', '/mnt/data']
    drop_data = os.environ.get('DROP_DATA')
    if drop_data:
        allowed_roots.append(os.path.join(drop_data, 'uploads'))
    allowed = False
    for allowed_root in allowed_roots:
        if os.path.isdir(allowed_root):
            ar = os.path.realpath(allowed_root)
            if canon == ar or canon.startswith(ar + os.sep):
                allowed = True
                break
    if not allowed:
        return None, 'source_file is outside allowed upload/temp directories'
    if not os.path.isfile(canon):
        return None, 'source_file must be a regular file'
    if os.path.getsize(canon) > MAX_BINARY:
        return None, 'source_file for %s exceeds %d bytes' % (rel, MAX_BINARY)
    try:
        with open(canon, 'rb') as f:
            return check_binary_size(f.read(), rel)
    except Exception as e:
        return None, 'Failed to read source_file: %s' % e

def blocked_ip(ip):
    try:
        addr = ipaddress.ip_address(ip)
    except ValueError:
        return True
    return addr.is_private or addr.is_loopback or addr.is_link_local or addr.is_multicast or addr.is_unspecified or addr.is_reserved

def is_allowed_chatgpt_estuary_url(parsed):
    if parsed.scheme != 'https' or parsed.hostname != 'chatgpt.com':
        return False
    if parsed.path != '/backend-api/estuary/content':
        return False
    qs = urllib.parse.parse_qs(parsed.query)
    ids = qs.get('id') or []
    sigs = qs.get('sig') or []
    return any(v.startswith('file_') for v in ids) and any(bool(v) for v in sigs)

def validate_url(source_url):
    try:
        parsed = urllib.parse.urlparse(source_url)
    except Exception as e:
        return None, 'Invalid source_url: %s' % e
    if parsed.scheme not in ('http', 'https'):
        return None, 'source_url must use http or https'
    if parsed.username or parsed.password:
        return None, 'source_url must not contain credentials'
    host = parsed.hostname
    if not host:
        return None, 'source_url must include a host'
    if host.lower() == 'localhost' or host.lower().endswith('.localhost'):
        return None, 'source_url host is not allowed'
    if is_allowed_chatgpt_estuary_url(parsed):
        return source_url, None
    port = parsed.port or (443 if parsed.scheme == 'https' else 80)
    try:
        infos = socket.getaddrinfo(host, port, type=socket.SOCK_STREAM)
    except Exception as e:
        return None, 'Failed to resolve source_url host: %s' % e
    if not infos:
        return None, 'source_url host resolved to no addresses'
    for info in infos:
        if blocked_ip(info[4][0]):
            return None, 'source_url resolves to a blocked private/local address'
    return source_url, None

def read_url(source_url, rel):
    url, e = validate_url(source_url)
    if e:
        return None, e
    req = urllib.request.Request(url, headers={'User-Agent': 'private-drop-artifact-import/1'})
    opener = urllib.request.build_opener(urllib.request.HTTPRedirectHandler)
    try:
        with urllib.request.urlopen(req, timeout=URL_TIMEOUT) as resp:
            final_url = resp.geturl()
            if final_url != url:
                return None, 'source_url redirects are not allowed'
            length = resp.headers.get('Content-Length')
            if length and int(length) > MAX_BINARY:
                return None, 'source_url content for %s exceeds %d bytes' % (rel, MAX_BINARY)
            data = resp.read(MAX_BINARY + 1)
    except Exception as e:
        return None, 'Failed to fetch source_url: %s' % e
    return check_binary_size(data, rel)

def main():
    if len(sys.argv) < 2:
        print(json.dumps(err('Missing project root argument')))
        return
    root = sys.argv[1]
    if not os.path.isdir(root):
        print(json.dumps(err('Project root does not exist: ' + root)))
        return
    try:
        body = json.load(sys.stdin)
    except Exception as e:
        print(json.dumps(err('Invalid JSON: ' + str(e))))
        return
    dry_run = body.get('dry_run', False)
    edits = body.get('edits', [])
    if not edits:
        print(json.dumps(err('edits cannot be empty')))
        return
    originals = {}
    current = {}
    paths_map = {}
    binary_originals = {}
    binary_current = {}
    changed = set()
    for ed in edits:
        etype = ed.get('type', '')
        rel = ed.get('path', '')
        e = validate_path(rel)
        if e:
            print(json.dumps(err(e)))
            return
        text_key = None
        if etype == 'replace_text':
            text_key = 'new_text'
        elif etype == 'replace_range':
            text_key = 'new_text'
        elif etype == 'append_file':
            text_key = 'text'
        elif etype in ('create_file', 'write_file'):
            text_key = 'content'
        if text_key:
            txt = ed.get(text_key, '')
            if len(txt.encode('utf-8')) > MAX_TEXT:
                print(json.dumps(err('edit text for %s exceeds %d bytes' % (rel, MAX_TEXT))))
                return
        if etype == 'replace_text':
            if rel not in current:
                full, e = resolve(root, rel, True)
                if e:
                    print(json.dumps(err(e)))
                    return
                paths_map[rel] = full
                c, e = read_file(full)
                if e:
                    print(json.dumps(err(e)))
                    return
                originals.setdefault(rel, c)
                current[rel] = c
            after, e = replace_nth(current[rel], ed.get('old_text', ''), ed.get('new_text', ''), ed.get('occurrence'))
            if e:
                print(json.dumps(err(e)))
                return
            current[rel] = after
        elif etype == 'replace_range':
            if rel not in current:
                full, e = resolve(root, rel, True)
                if e:
                    print(json.dumps(err(e)))
                    return
                paths_map[rel] = full
                c, e = read_file(full)
                if e:
                    print(json.dumps(err(e)))
                    return
                originals.setdefault(rel, c)
                current[rel] = c
            after, e = replace_range(current[rel], ed['start_line'], ed['end_line'], ed.get('new_text', ''))
            if e:
                print(json.dumps(err(e)))
                return
            current[rel] = after
        elif etype == 'append_file':
            if rel not in current:
                full, e = resolve(root, rel, True)
                if e:
                    print(json.dumps(err(e)))
                    return
                paths_map[rel] = full
                c, e = read_file(full)
                if e:
                    print(json.dumps(err(e)))
                    return
                originals.setdefault(rel, c)
                current[rel] = c
            current[rel] = current[rel] + ed.get('text', '')
        elif etype == 'create_file':
            full, e = resolve(root, rel, False)
            if e:
                print(json.dumps(err(e)))
                return
            if os.path.exists(full) or rel in current:
                print(json.dumps(err('File already exists: ' + rel)))
                return
            paths_map[rel] = full
            originals.setdefault(rel, None)
            current[rel] = ed.get('content', '')
        elif etype == 'write_file':
            full, e = resolve(root, rel, False)
            if e:
                print(json.dumps(err(e)))
                return
            allow = ed.get('allow_overwrite', False)
            if os.path.exists(full) and not allow:
                print(json.dumps(err('File exists and allow_overwrite is false: ' + rel)))
                return
            if os.path.exists(full) and rel not in originals:
                old_c, e2 = read_file(full)
                if e2:
                    print(json.dumps(err(e2)))
                    return
                originals[rel] = old_c
            elif rel not in originals:
                originals.setdefault(rel, None)
            paths_map[rel] = full
            current[rel] = ed.get('content', '')
        elif etype in ('create_binary_file', 'create_binary_artifact'):
            full, e = resolve(root, rel, False)
            if e:
                print(json.dumps(err(e)))
                return
            if os.path.exists(full) or rel in binary_current:
                print(json.dumps(err('File already exists: ' + rel)))
                return
            data, e = decode_binary(ed.get('base64_content', ''), rel)
            if e:
                print(json.dumps(err(e)))
                return
            paths_map[rel] = full
            binary_originals.setdefault(rel, None)
            binary_current[rel] = data
        elif etype in ('write_binary_file', 'write_binary_artifact'):
            full, e = resolve(root, rel, False)
            if e:
                print(json.dumps(err(e)))
                return
            allow = ed.get('allow_overwrite', False)
            if os.path.exists(full) and not allow:
                print(json.dumps(err('File exists and allow_overwrite is false: ' + rel)))
                return
            data, e = decode_binary(ed.get('base64_content', ''), rel)
            if e:
                print(json.dumps(err(e)))
                return
            if os.path.exists(full) and rel not in binary_originals:
                try:
                    with open(full, 'rb') as f:
                        binary_originals[rel] = f.read()
                except Exception as e:
                    print(json.dumps(err('Failed to read binary file: ' + str(e))))
                    return
            elif rel not in binary_originals:
                binary_originals.setdefault(rel, None)
            paths_map[rel] = full
            binary_current[rel] = data
        elif etype == 'create_binary_file_from_upload':
            full, e = resolve(root, rel, False)
            if e:
                print(json.dumps(err(e)))
                return
            if os.path.exists(full) or rel in binary_current:
                print(json.dumps(err('File already exists: ' + rel)))
                return
            data, e = read_upload(root, ed.get('source_file', ''), rel)
            if e:
                print(json.dumps(err(e)))
                return
            paths_map[rel] = full
            binary_originals.setdefault(rel, None)
            binary_current[rel] = data
        elif etype == 'write_binary_file_from_upload':
            full, e = resolve(root, rel, False)
            if e:
                print(json.dumps(err(e)))
                return
            allow = ed.get('allow_overwrite', False)
            if os.path.exists(full) and not allow:
                print(json.dumps(err('File exists and allow_overwrite is false: ' + rel)))
                return
            data, e = read_upload(root, ed.get('source_file', ''), rel)
            if e:
                print(json.dumps(err(e)))
                return
            if os.path.exists(full) and rel not in binary_originals:
                try:
                    with open(full, 'rb') as f:
                        binary_originals[rel] = f.read()
                except Exception as e:
                    print(json.dumps(err('Failed to read binary file: ' + str(e))))
                    return
            elif rel not in binary_originals:
                binary_originals.setdefault(rel, None)
            paths_map[rel] = full
            binary_current[rel] = data
        elif etype == 'create_binary_file_from_url':
            full, e = resolve(root, rel, False)
            if e:
                print(json.dumps(err(e)))
                return
            if os.path.exists(full) or rel in binary_current:
                print(json.dumps(err('File already exists: ' + rel)))
                return
            data, e = read_url(ed.get('source_url', ''), rel)
            if e:
                print(json.dumps(err(e)))
                return
            paths_map[rel] = full
            binary_originals.setdefault(rel, None)
            binary_current[rel] = data
        elif etype == 'write_binary_file_from_url':
            full, e = resolve(root, rel, False)
            if e:
                print(json.dumps(err(e)))
                return
            allow = ed.get('allow_overwrite', False)
            if os.path.exists(full) and not allow:
                print(json.dumps(err('File exists and allow_overwrite is false: ' + rel)))
                return
            data, e = read_url(ed.get('source_url', ''), rel)
            if e:
                print(json.dumps(err(e)))
                return
            if os.path.exists(full) and rel not in binary_originals:
                try:
                    with open(full, 'rb') as f:
                        binary_originals[rel] = f.read()
                except Exception as e:
                    print(json.dumps(err('Failed to read binary file: ' + str(e))))
                    return
            elif rel not in binary_originals:
                binary_originals.setdefault(rel, None)
            paths_map[rel] = full
            binary_current[rel] = data
        else:
            print(json.dumps(err('Unknown edit type: ' + etype)))
            return
        changed.add(rel)
    diff_parts = []
    for p in sorted(changed):
        old = originals.get(p)
        new = current.get(p)
        if new is not None and old != new:
            diff_parts.append(simple_diff(p, old, new))
        elif p in binary_current:
            old_b = binary_originals.get(p)
            new_b = binary_current.get(p)
            if new_b is not None and old_b != new_b:
                diff_parts.append(simple_binary_diff(p, None if old_b is None else len(old_b), len(new_b)))
    diff = ''.join(diff_parts)
    if not dry_run:
        for p in sorted(changed):
            full = paths_map.get(p)
            content = current.get(p)
            if full and content is not None:
                os.makedirs(os.path.dirname(full), exist_ok=True)
                with open(full, 'w', encoding='utf-8') as f:
                    f.write(content)
            elif full and p in binary_current and binary_current[p] is not None:
                os.makedirs(os.path.dirname(full), exist_ok=True)
                with open(full, 'wb') as f:
                    f.write(binary_current[p])
    print(json.dumps({
        'success': True,
        'changed_files': sorted(changed),
        'diff': diff,
        'warnings': [],
        'error': None
    }))

main()
"#####;

/// Run edit operations on a remote SSH project by piping JSON to a python3 script.
pub(super) fn ssh_apply_project_edit(
    proj: &ProjectConfig,
    body: &EditRequest,
    ssh_config: Option<&SshConfig>,
) -> EditResponse {
    let ssh_target = match build_ssh_target(proj) {
        Ok(t) => t,
        Err(e) => return edit_error(e),
    };
    let project_path = &proj.path;

    // Serialize the edit request to JSON
    let body_json = match serde_json::to_string(body) {
        Ok(j) => j,
        Err(e) => return edit_error(format!("Failed to serialize edit request: {}", e)),
    };

    // Build the remote command: run python3 with the embedded script
    // Pass project root as first argument; script reads JSON from stdin
    let remote_cmd = format!(
        "python3 -c {} {}",
        shell_escape(REMOTE_EDIT_SCRIPT),
        shell_escape(project_path)
    );

    let start = Instant::now();
    let mut child = match build_ssh_command(&ssh_target, &remote_cmd, ssh_config)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(e) => return edit_error(format!("Failed to spawn SSH edit: {}", e)),
    };

    if let Some(mut stdin) = child.stdin.take() {
        use std::io::Write;
        if let Err(e) = stdin.write_all(body_json.as_bytes()) {
            let _ = child.kill();
            return edit_error(format!("Failed to write SSH edit payload: {}", e));
        }
    }

    let result = loop {
        match child.try_wait() {
            Ok(Some(_status)) => break child.wait_with_output(),
            Ok(None) if start.elapsed() > std::time::Duration::from_secs(CHECK_TIMEOUT_SECS) => {
                let _ = child.kill();
                let _ = child.wait();
                return edit_error(format!(
                    "SSH edit timed out after {} seconds",
                    CHECK_TIMEOUT_SECS
                ));
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(100)),
            Err(e) => {
                let _ = child.kill();
                return edit_error(format!("Failed while waiting for SSH edit: {}", e));
            }
        }
    };

    match result {
        Ok(output) => {
            let _elapsed = start.elapsed().as_millis() as u64;
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let code = output.status.code().unwrap_or(-1);

            if code != 0 && stdout.is_empty() {
                // python3 not available or other exec failure
                if stderr.contains("No such file")
                    || stderr.contains("not found")
                    || stderr.contains("No module")
                {
                    return edit_error(
                        "Remote python3 is not available. Install python3 on the remote host."
                            .to_string(),
                    );
                }
                return edit_error(format!(
                    "SSH edit failed (exit {}): {}",
                    code,
                    stderr.chars().take(500).collect::<String>()
                ));
            }

            let mut resp: EditResponse = match serde_json::from_str(&stdout) {
                Ok(r) => r,
                Err(e) => {
                    return edit_error(format!(
                        "Failed to parse remote edit response: {}. Raw: {}",
                        e,
                        stdout.chars().take(500).collect::<String>()
                    ))
                }
            };
            let (truncated_diff, diff_truncated) = truncate_string(resp.diff, MAX_OUTPUT_LEN);
            resp.diff = truncated_diff;
            if diff_truncated {
                resp.warnings.push("Remote diff was truncated".to_string());
            }
            if !stderr.is_empty() {
                resp.warnings.push(format!(
                    "Remote stderr: {}",
                    stderr.chars().take(500).collect::<String>()
                ));
            }
            resp
        }
        Err(e) => edit_error(format!("Failed to execute SSH edit: {}", e)),
    }
}
