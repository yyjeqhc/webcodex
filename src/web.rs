use salvo::prelude::*;

// ============================================================================
// Web UI
// ============================================================================

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

fn app_shell(title: &str, page_js: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8"><meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>{title} - Private Drop</title>
<link rel="stylesheet" href="/assets/styles.css">
<script src="/assets/app.js" defer></script>
</head>
<body>
<div class="container">
<div class="header"><h1>Private Drop</h1></div>
<div class="nav"><a href="/channels">Channels</a><a href="/c/inbox">Inbox</a><a href="/c/files">Files</a><a href="/send">Send</a><a href="/desktop">Desktop</a></div>
<div id="app"><div class="loading">Loading...</div></div>
</div>
<script defer>
window.addEventListener('DOMContentLoaded', function() {{
{page_js}
}});
</script>
</body>
</html>"#,
        title = html_escape(title),
        page_js = page_js
    )
}

#[handler]
pub async fn frontend_app_js(res: &mut Response) {
    res.add_header(
        "content-type",
        "application/javascript; charset=utf-8",
        true,
    )
    .ok();
    res.render(Text::Plain(include_str!("../frontend/dist/app.js")));
}

#[handler]
pub async fn frontend_styles_css(res: &mut Response) {
    res.add_header("content-type", "text/css; charset=utf-8", true)
        .ok();
    res.render(Text::Plain(include_str!("../frontend/dist/styles.css")));
}

#[handler]
pub async fn login_page(_req: &mut Request, _depot: &mut Depot, res: &mut Response) {
    let page_js = r#"
(function(){
    if(getToken()){window.location.href='/c/inbox';return}
    document.getElementById('app').innerHTML=
        '<div class="token-form"><div class="card">'+
        '<h2 style="margin-bottom:16px">Login</h2>'+
        '<div id="err"></div>'+
        '<form id="lf">'+
        '<div class="form-group"><label for="token">Access Token</label>'+
        '<input type="password" id="token" placeholder="Enter your token" required autofocus></div>'+
        '<div class="form-actions"><button type="submit" class="btn btn-primary">Login</button></div>'+
        '</form></div></div>';
    document.getElementById('lf').addEventListener('submit',function(e){
        e.preventDefault();
        var t=document.getElementById('token').value.trim();
        if(!t)return;
        setToken(t);
        window.location.href='/c/inbox';
    });
})()
"#;
    res.render(Text::Html(app_shell("Login", page_js)));
}

#[handler]
pub async fn home_page(_req: &mut Request, _depot: &mut Depot, res: &mut Response) {
    // Client-side: redirect to /c/inbox if logged in, else /login
    let page_js = r#"
(function(){
    if(!getToken()){window.location.href='/login';return}
    window.location.href='/channels';
})()
"#;
    res.render(Text::Html(app_shell("Home", page_js)));
}

#[handler]
pub async fn channels_page(_req: &mut Request, _depot: &mut Depot, res: &mut Response) {
    let page_js = r#"
(async function(){
    if(!requireToken())return;
    var app=document.getElementById('app');
    try{
        var r=await apiCall('/api/channels');
        if(!r)return;
        if(!r.ok){var d=await r.json();app.innerHTML='<div class="alert alert-error">'+escapeHtml(d.error||'Failed to load channels')+'</div>';return}
        var channels=await r.json();
        var html='<div style="display:flex;justify-content:space-between;align-items:center;margin-bottom:16px"><h2>Channels</h2><a href="/send" class="btn btn-primary">Send</a></div>';
        if(channels.length===0){
            html+='<div class="card"><p style="color:#666;text-align:center">No channels yet</p></div>';
        }else{
            channels.forEach(function(ch){
                html+='<a href="/c/'+encodeURIComponent(ch.name)+'" style="color:inherit;text-decoration:none"><div class="card"><div class="card-header"><div><div class="card-title">'+escapeHtml(ch.display_name||ch.name)+'</div><div class="card-meta">'+escapeHtml(ch.name)+'</div></div><span class="channel-badge">'+ch.message_count+' message'+(ch.message_count===1?'':'s')+'</span></div></div></a>';
            });
        }
        app.innerHTML=html;
    }catch(e){
        app.innerHTML='<div class="alert alert-error">Error: '+escapeHtml(e.message)+'</div>';
    }
})()
"#;
    res.render(Text::Html(app_shell("Channels", page_js)));
}

#[handler]
pub async fn channel_page(req: &mut Request, _depot: &mut Depot, res: &mut Response) {
    let channel = req.param::<String>("channel").unwrap_or_default();
    let page_js = format!(
        r#"
(async function(){{
    if(!requireToken())return;
    var ch={channel_json};
    var app=document.getElementById('app');
    try{{
        var r=await apiCall('/api/messages?channel='+encodeURIComponent(ch)+'&limit=50');
        if(!r)return;
        if(!r.ok){{var d=await r.json();app.innerHTML='<div class="alert alert-error">'+escapeHtml(d.error||'Failed to load')+'</div>';return}}
        var data=await r.json();
        var html='<div style="display:flex;justify-content:space-between;align-items:center;margin-bottom:16px">'+
            '<div><a href="/channels" style="display:inline-block;margin-bottom:8px;color:#3498db;text-decoration:none">← Back to Channels</a>'+
            '<h2>'+escapeHtml(ch)+'</h2></div>'+
            '<a href="/send?channel='+encodeURIComponent(ch)+'" class="btn btn-primary">Send</a></div>';
        if(data.messages.length===0){{
            html+='<div class="card"><p style="color:#666;text-align:center">No messages yet</p></div>';
        }}else{{
            data.messages.forEach(function(m){{
                var title=m.title||(m.kind==='file'?(m.file_name||'File'):'Text');
                var ts=fmtTime(m.created_at);
                var body='';
                if(m.kind==='text'){{
                    var t=m.text||'';
                    body='<div class="card-text">'+escapeHtml(t.length>200?t.substring(0,200)+'...':t)+'</div>';
                }}else{{
                    body='<div class="file-info"><span class="file-icon">📎</span><div>'+
                        '<div style="font-weight:bold">'+escapeHtml(m.file_name||'unknown')+'</div>'+
                        '<div class="file-size">'+formatSize(m.file_size||0)+'</div></div></div>';
                }}
                var actions='';
                if(m.kind==='text'){{
                    actions='<button class="btn btn-sm btn-primary js-copy" data-text-id="t-'+m.id+'">Copy</button> '+
                        '<button class="btn btn-sm btn-danger js-delete" data-delete-id="'+m.id+'">Del</button>';
                }}else{{
                    actions='<a href="/api/files/'+m.id+'" class="btn btn-sm btn-success" download>Download</a> '+
                        '<button class="btn btn-sm btn-danger js-delete" data-delete-id="'+m.id+'">Del</button>';
                }}
                html+='<div class="card" id="t-'+m.id+'"><div class="card-header"><div>'+
                    '<div class="card-title"><a href="/m/'+m.id+'" style="color:inherit;text-decoration:none">'+escapeHtml(title)+'</a></div>'+
                    '<div class="card-meta">'+ts+'</div></div>'+
                    '<div class="form-actions">'+actions+'</div></div>'+body+'</div>';
            }});
        }}
        app.innerHTML=html;
        app.addEventListener('click',function(e){{
            var btn=e.target.closest('.js-copy');
            if(btn){{var el=document.getElementById(btn.getAttribute('data-text-id'));if(el)copyText(el.textContent);return}}
            btn=e.target.closest('.js-delete');
            if(btn){{deleteMsg(btn.getAttribute('data-delete-id'));return}}
        }});
    }}catch(e){{
        app.innerHTML='<div class="alert alert-error">Error: '+escapeHtml(e.message)+'</div>';
    }}
}})()
"#,
        channel_json = serde_json::to_string(&channel).unwrap()
    );
    res.render(Text::Html(app_shell(&channel, &page_js)));
}

#[handler]
pub async fn message_page(req: &mut Request, _depot: &mut Depot, res: &mut Response) {
    let id = req.param::<String>("id").unwrap_or_default();
    let page_js = format!(
        r#"
(async function(){{
    if(!requireToken())return;
    var msgId={id_json};
    var app=document.getElementById('app');
    try{{
        var r=await apiCall('/api/messages/'+msgId);
        if(!r)return;
        if(!r.ok){{app.innerHTML='<div class="alert alert-error">Message not found</div>';return}}
        var m=await r.json();
        var ts=fmtTime(m.created_at);
        var html='<div class="card"><div style="display:flex;justify-content:space-between;margin-bottom:12px">'+
            '<div><span class="channel-badge">'+escapeHtml(m.channel)+'</span> <span class="card-meta">'+ts+'</span></div>'+
            '<div class="form-actions">';
        if(m.kind==='text'){{
            html+='<button class="btn btn-sm btn-primary js-copy" data-text-id="ft">Copy</button> ';
        }}else{{
            html+='<a href="/api/files/'+m.id+'" class="btn btn-sm btn-success" download>Download</a> ';
        }}
        html+='<button class="btn btn-sm btn-danger js-delete" data-delete-id="'+m.id+'">Del</button></div></div>';
        if(m.kind==='text'){{
            html+='<div id="ft" class="card-text" style="max-height:none">'+escapeHtml(m.text||'')+'</div>';
        }}else{{
            html+='<div class="file-info" style="font-size:1.2em"><span class="file-icon" style="font-size:48px">📎</span>'+
                '<div><div style="font-weight:bold;font-size:1.2em">'+escapeHtml(m.file_name||'unknown')+'</div>'+
                '<div class="file-size">'+formatSize(m.file_size||0)+'</div>'+
                '<div class="file-size">'+escapeHtml(m.mime_type||'')+'</div></div></div>';
        }}
        html+='</div>';
        var title=m.title||(m.kind==='file'?(m.file_name||'File'):'Message');
        app.innerHTML='<h2 style="margin-bottom:16px">'+escapeHtml(title)+'</h2>'+html;
        app.addEventListener('click',function(e){{
            var btn=e.target.closest('.js-copy');
            if(btn){{var el=document.getElementById(btn.getAttribute('data-text-id'));if(el)copyText(el.textContent);return}}
            btn=e.target.closest('.js-delete');
            if(btn){{deleteMsg(btn.getAttribute('data-delete-id'));return}}
        }});
    }}catch(e){{
        app.innerHTML='<div class="alert alert-error">Error: '+escapeHtml(e.message)+'</div>';
    }}
}})()
"#,
        id_json = serde_json::to_string(&id).unwrap()
    );
    res.render(Text::Html(app_shell("Message", &page_js)));
}

#[handler]
pub async fn desktop_page(_req: &mut Request, _depot: &mut Depot, res: &mut Response) {
    let page_js = r#"
(async function(){
    if(!requireToken())return;
    var app=document.getElementById('app');
    function taskCard(t){
        var shot=t.screenshot_url?'<div style="margin-top:8px"><a href="'+escapeHtml(t.screenshot_url)+'" target="_blank">Screenshot</a></div>':'';
        return '<a href="/desktop/tasks/'+encodeURIComponent(t.id)+'" style="color:inherit;text-decoration:none"><div class="card">'+
            '<div class="card-header"><div><div class="card-title">'+escapeHtml(t.title)+'</div><div class="card-meta">'+fmtTime(t.updated_at)+' · '+escapeHtml(t.claimed_by||'')+'</div></div><span class="channel-badge">'+escapeHtml(t.status)+'</span></div>'+
            '<div class="card-text">'+escapeHtml(t.last_event||t.instructions||'')+'</div>'+shot+'</div></a>';
    }
    async function loadTasks(){
        var r=await apiCall('/api/desktop/tasks?limit=20');
        if(!r)return;
        if(!r.ok)return;
        var d=await r.json();
        var html='';
        (d.tasks||[]).forEach(function(t){html+=taskCard(t)});
        document.getElementById('task-list').innerHTML=html||'<div class="card"><p>No desktop tasks yet</p></div>';
    }
    app.innerHTML='<div style="display:flex;justify-content:space-between;align-items:center;margin-bottom:16px"><h2>Desktop Agent</h2><button id="refresh" class="btn btn-sm btn-primary">Refresh</button></div>'+ 
        '<div id="msg"></div><div class="card"><h3 style="margin-bottom:12px">Send Desktop Task</h3>'+ 
        '<form id="df">'+
        '<div class="form-group"><label for="title">Title</label><input id="title" placeholder="Open site and type message" required></div>'+ 
        '<div class="form-group"><label for="url">URL to open (optional)</label><input id="url" placeholder="https://example.com"></div>'+ 
        '<div class="form-group"><label for="text">Text to type/send (optional)</label><textarea id="text" rows="5" placeholder="Text the worker should paste into the active page/app"></textarea></div>'+ 
        '<div class="form-group"><label><input type="checkbox" id="sendKey"> Press Enter after typing</label></div>'+ 
        '<div class="form-group"><label for="extra">Extra instructions</label><textarea id="extra" rows="4" placeholder="Wait for page load, click the input first if needed..."></textarea></div>'+ 
        '<div class="form-actions"><button class="btn btn-primary" type="submit">Create Task</button></div></form></div>'+ 
        '<h3 style="margin:20px 0 12px">Recent Desktop Tasks</h3><div id="task-list"><div class="loading">Loading...</div></div>';
    document.getElementById('refresh').addEventListener('click',loadTasks);
    document.getElementById('df').addEventListener('submit',async function(e){
        e.preventDefault();
        var title=document.getElementById('title').value.trim();
        var url=document.getElementById('url').value.trim();
        var text=document.getElementById('text').value;
        var extra=document.getElementById('extra').value.trim();
        var sendKey=document.getElementById('sendKey').checked;
        var parts=[];
        if(url)parts.push('open: '+url);
        if(text)parts.push('type: '+text);
        if(sendKey)parts.push('press_enter: true');
        if(extra)parts.push(extra);
        var instructions=parts.join('\n');
        if(!instructions){instructions=title}
        var r=await apiCall('/api/desktop/tasks',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({title:title,instructions:instructions,priority:10})});
        if(!r)return;
        var d=await r.json();
        if(r.ok&&d.task){window.location.href='/desktop/tasks/'+encodeURIComponent(d.task.id)}else{document.getElementById('msg').innerHTML='<div class="alert alert-error">'+escapeHtml(d.error||'Failed to create task')+'</div>'}
    });
    await loadTasks();
})()
"#;
    res.render(Text::Html(app_shell("Desktop", page_js)));
}

#[handler]
pub async fn desktop_task_page(req: &mut Request, _depot: &mut Depot, res: &mut Response) {
    let id = req.param::<String>("id").unwrap_or_default();
    let page_js = format!(
        r#"
(async function(){{
    if(!requireToken())return;
    var taskId={id_json};
    var app=document.getElementById('app');
    function row(label,value){{return '<div style="margin:6px 0"><strong>'+escapeHtml(label)+':</strong> '+escapeHtml(value||'')+'</div>'}}
    try{{
        var r=await apiCall('/api/desktop/tasks/'+encodeURIComponent(taskId));
        if(!r)return;
        if(!r.ok){{app.innerHTML='<div class="alert alert-error">Desktop task not found</div>';return}}
        var d=await r.json();
        var t=d.task;
        var events=d.events||[];
        var html='<div style="display:flex;justify-content:space-between;align-items:center;margin-bottom:16px">'+
            '<div><a href="/channels" style="display:inline-block;margin-bottom:8px;color:#3498db;text-decoration:none">← Back</a>'+
            '<h2>Desktop Task</h2></div><span class="channel-badge">'+escapeHtml(t.status)+'</span></div>';
        html+='<div class="card"><div class="card-title">'+escapeHtml(t.title)+'</div>'+
            row('ID',t.id)+row('Worker',t.claimed_by||'')+row('Priority',String(t.priority))+row('Updated',fmtTime(t.updated_at))+
            '<div style="margin-top:12px"><strong>Instructions</strong><div class="card-text" style="max-height:none">'+escapeHtml(t.instructions)+'</div></div>';
        if(t.screenshot_url){{html+='<div style="margin-top:16px"><a href="'+escapeHtml(t.screenshot_url)+'" target="_blank"><img src="'+escapeHtml(t.screenshot_url)+'" style="max-width:100%;border:1px solid #ddd;border-radius:8px"></a></div>'}}
        html+='</div>';
        html+='<h3 style="margin:20px 0 12px">Event Timeline</h3>';
        if(events.length===0){{html+='<div class="card"><p>No events recorded yet</p></div>'}}else{{
            events.forEach(function(ev){{
                html+='<div class="card"><div class="card-header"><div><div class="card-title">'+escapeHtml(ev.status)+'</div><div class="card-meta">'+fmtTime(ev.created_at)+' '+escapeHtml(ev.worker||'')+'</div></div></div>'+
                    '<div class="card-text" style="max-height:none">'+escapeHtml(ev.message||'')+'</div>';
                if(ev.screenshot_url){{html+='<div style="margin-top:12px"><a href="'+escapeHtml(ev.screenshot_url)+'" target="_blank">Open screenshot</a></div>'}}
                html+='</div>';
            }});
        }}
        app.innerHTML=html;
    }}catch(e){{
        app.innerHTML='<div class="alert alert-error">Error: '+escapeHtml(e.message)+'</div>';
    }}
}})()
"#,
        id_json = serde_json::to_string(&id).unwrap()
    );
    res.render(Text::Html(app_shell("Desktop Task", &page_js)));
}

#[handler]
pub async fn send_page(req: &mut Request, _depot: &mut Depot, res: &mut Response) {
    let default_channel = req
        .query::<String>("channel")
        .unwrap_or_else(|| "inbox".to_string());
    let page_js = format!(
        r#"
(async function(){{
    if(!requireToken())return;
    var defCh={channel_json};
    var app=document.getElementById('app');
    app.innerHTML=
        '<h2 style="margin-bottom:16px">Send Message</h2>'+
        '<div id="msg"></div>'+
        '<div class="card"><h3 style="margin-bottom:12px">Text Message</h3>'+
        '<form id="sf">'+
        '<div class="form-group"><label for="channel">Channel</label>'+
        '<select id="channel">'+
        '<option value="inbox">inbox</option>'+
        '<option value="xline">xline</option>'+
        '<option value="thesis">thesis</option>'+
        '<option value="packfix">packfix</option>'+
        '<option value="omo">omo</option>'+
        '<option value="files">files</option>'+
        '</select></div>'+
        '<div class="form-group"><label for="title">Title (optional)</label>'+
        '<input type="text" id="title" placeholder="Message title"></div>'+
        '<div class="form-group"><label for="text">Text</label>'+
        '<textarea id="text" placeholder="Paste your text here..." rows="10" required></textarea></div>'+
        '<div class="form-actions"><button type="submit" class="btn btn-primary">Send</button></div>'+
        '</form></div>'+
        '<div class="card" style="margin-top:16px"><h3 style="margin-bottom:12px">Upload File</h3>'+
        '<form id="ff">'+
        '<div class="form-group"><label for="file">File</label>'+
        '<input type="file" id="file" required></div>'+
        '<div class="form-actions"><button type="submit" class="btn btn-success">Upload</button></div>'+
        '</form></div>';
    document.getElementById('channel').value=defCh;
    document.getElementById('sf').addEventListener('submit',async function(e){{
        e.preventDefault();
        var ch=document.getElementById('channel').value;
        var title=document.getElementById('title').value||null;
        var text=document.getElementById('text').value;
        var msgEl=document.getElementById('msg');
        try{{
            var r=await apiCall('/api/messages',{{
                method:'POST',
                headers:{{'Content-Type':'application/json'}},
                body:JSON.stringify({{channel:ch,title:title,text:text}})
            }});
            if(!r)return;
            if(r.ok){{
                window.location.href='/c/'+encodeURIComponent(ch);
            }}else{{
                var d=await r.json();
                msgEl.innerHTML='<div class="alert alert-error">'+escapeHtml(d.error||'Failed to send')+'</div>';
            }}
        }}catch(err){{
            msgEl.innerHTML='<div class="alert alert-error">'+escapeHtml(err.message)+'</div>';
        }}
    }});
    document.getElementById('ff').addEventListener('submit',async function(e){{
        e.preventDefault();
        var ch=document.getElementById('channel').value;
        var fileInput=document.getElementById('file');
        var msgEl=document.getElementById('msg');
        if(!fileInput.files[0])return;
        var fd=new FormData();
        fd.append('file',fileInput.files[0]);
        try{{
            var r=await apiCall('/api/files?channel='+encodeURIComponent(ch),{{method:'POST',body:fd}});
            if(!r)return;
            if(r.ok){{
                window.location.href='/c/'+encodeURIComponent(ch);
            }}else{{
                var d=await r.json();
                msgEl.innerHTML='<div class="alert alert-error">'+escapeHtml(d.error||'Failed to upload')+'</div>';
            }}
        }}catch(err){{
            msgEl.innerHTML='<div class="alert alert-error">'+escapeHtml(err.message)+'</div>';
        }}
    }});
}})()
"#,
        channel_json = serde_json::to_string(&default_channel).unwrap()
    );
    res.render(Text::Html(app_shell("Send", &page_js)));
}
