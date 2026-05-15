//! Web UI — 内嵌管理面板 (HTMX + Alpine.js)

use axum::response::Html;

const STYLE_CSS: &str = r##"
    :root { --bg: #0f172a; --card: #1e293b; --accent: #3b82f6; --text: #e2e8f0; --dim: #94a3b8; --border: #334155; --ok: #22c55e; --warn: #eab308; --err: #ef4444; }
    * { margin: 0; padding: 0; box-sizing: border-box; }
    body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif; background: var(--bg); color: var(--text); min-height: 100vh; }
    nav { background: var(--card); border-bottom: 1px solid var(--border); padding: 1rem 2rem; display: flex; align-items: center; gap: 2rem; position: sticky; top: 0; z-index: 10; }
    nav .logo { font-size: 1.25rem; font-weight: 700; color: var(--accent); }
    nav a { color: var(--dim); text-decoration: none; font-size: 0.9rem; }
    nav a:hover { color: var(--text); }
    nav a.active { color: var(--accent); font-weight: 600; }
    .container { max-width: 1200px; margin: 2rem auto; padding: 0 2rem; }
    .card { background: var(--card); border: 1px solid var(--border); border-radius: 8px; padding: 1.5rem; margin-bottom: 1rem; }
    .card h2 { font-size: 1.1rem; margin-bottom: 1rem; color: var(--accent); }
    .stats-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(180px, 1fr)); gap: 1rem; }
    .stat { text-align: center; padding: 1rem; }
    .stat .number { font-size: 2rem; font-weight: 700; color: var(--accent); }
    .stat .label { font-size: 0.85rem; color: var(--dim); margin-top: 0.3rem; }
    input, select, textarea { padding: 0.6rem 1rem; border-radius: 6px; border: 1px solid var(--border); background: var(--bg); color: var(--text); font-size: 0.9rem; width: 100%; }
    input:focus { outline: none; border-color: var(--accent); }
    button, .btn { padding: 0.6rem 1.2rem; border-radius: 6px; border: none; cursor: pointer; font-weight: 600; font-size: 0.9rem; background: var(--accent); color: white; }
    button:hover { opacity: 0.85; }
    .btn.danger { background: var(--err); }
    .form-group { margin-bottom: 1rem; }
    .form-group label { display: block; font-size: 0.85rem; color: var(--dim); margin-bottom: 0.3rem; }
    table { width: 100%; border-collapse: collapse; }
    th, td { text-align: left; padding: 0.6rem 1rem; border-bottom: 1px solid var(--border); font-size: 0.85rem; }
    th { color: var(--dim); font-weight: 600; }
    .badge { display: inline-block; padding: 0.15rem 0.5rem; border-radius: 4px; font-size: 0.7rem; font-weight: 600; }
    .badge-ok { background: rgba(34,197,94,0.2); color: var(--ok); }
    .badge-info { background: rgba(59,130,246,0.2); color: var(--accent); }
    .badge-err { background: rgba(239,68,68,0.2); color: var(--err); }
    .empty { text-align: center; padding: 3rem; color: var(--dim); }
    .empty .icon { font-size: 3rem; margin-bottom: 1rem; }
    .two-col { display: grid; grid-template-columns: 1fr 1fr; gap: 1rem; }
    @media (max-width: 768px) { .two-col { grid-template-columns: 1fr; } }
    pre { background: var(--bg); border: 1px solid var(--border); border-radius: 6px; padding: 1rem; overflow-x: auto; font-size: 0.8rem; }
"##;

fn nav_html(active: &str) -> String {
    let items = [
        ("dashboard", "/", "📊 Dashboard"),
        ("links", "/ui/links", "🔗 Links"),
        ("routes", "/ui/routes", "🛤️ Routes"),
        ("extensions", "/ui/extensions", "🧩 Extensions"),
        ("agent", "/ui/agent", "🤖 Agent"),
    ];
    let links: Vec<String> = items
        .iter()
        .map(|(key, href, label)| {
            let cls = if *key == active { " class=\"active\"" } else { "" };
            format!("<a href=\"{}\"{}>{}</a>", href, cls, label)
        })
        .collect();
    format!("<nav><span class=\"logo\">🔗 OpenLink</span>{}</nav>", links.join(""))
}

fn page_shell(title: &str, nav_active: &str, body: &str) -> Html<String> {
    Html(format!(
        "<!DOCTYPE html><html lang=\"zh-CN\"><head>\
        <meta charset=\"UTF-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1.0\">\
        <title>{t} — OpenLink</title>\
        <script src=\"https://unpkg.com/htmx.org@1.9.10\"></script>\
        <script defer src=\"https://unpkg.com/alpinejs@3.13.3\"></script>\
        <style>{css}</style></head>\
        <body>{nav}<div class=\"container\">{body}</div></body></html>",
        t = title,
        css = STYLE_CSS,
        nav = nav_html(nav_active),
        body = body,
    ))
}

pub fn dashboard_page() -> Html<String> {
    page_shell(
        "Dashboard",
        "dashboard",
        r##"
    <div class="stats-grid">
        <div class="card stat">
            <div class="number" x-data="{count:0}" x-init="fetch('/api/v1/stats/overview').then(r=>r.json()).then(d=>count=d.total_links||0)"><span x-text="count">-</span></div>
            <div class="label">Total Links</div>
        </div>
        <div class="card stat">
            <div class="number" x-data="{count:0}" x-init="fetch('/api/v1/stats/overview').then(r=>r.json()).then(d=>count=d.total_clicks||0)"><span x-text="count">-</span></div>
            <div class="label">Total Clicks</div>
        </div>
        <div class="card stat">
            <div class="number" x-data="{count:0}" x-init="fetch('/api/v1/extensions').then(r=>r.json()).then(d=>count=Array.isArray(d)?d.length:(d.extensions||[]).length||0)"><span x-text="count">-</span></div>
            <div class="label">Extensions</div>
        </div>
        <div class="card stat">
            <div class="number" x-data="{status:'...'}" x-init="fetch('/health').then(r=>r.json()).then(d=>status=d.healthy?'OK':'ERR')"><span x-text="status">-</span></div>
            <div class="label">Health</div>
        </div>
    </div>
    <div class="two-col">
        <div class="card">
            <h2>📋 Recent Links</h2>
            <div hx-get="/ui/links-table" hx-trigger="load" hx-swap="innerHTML">
                <div class="empty"><div class="icon">⏳</div>Loading...</div>
            </div>
        </div>
        <div class="card">
            <h2>🧩 Active Extensions</h2>
            <div hx-get="/ui/extensions-list" hx-trigger="load" hx-swap="innerHTML">
                <div class="empty"><div class="icon">⏳</div>Loading...</div>
            </div>
        </div>
    </div>
    "##,
    )
}

pub fn links_page() -> Html<String> {
    page_shell(
        "Links",
        "links",
        r##"
    <div class="card">
        <h2>➕ Create Link</h2>
        <form x-data="{url:'',code:'',expires:''}" @submit.prevent="fetch('/api/v1/links',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({original_url:url,custom_code:code||undefined,expires_at:expires||undefined})}).then(r=>{if(r.ok){url='';code='';expires='';htmx.trigger('#links-table','refresh')}})">
            <div class="two-col">
                <div class="form-group"><label>Original URL *</label><input type="url" x-model="url" placeholder="https://example.com" required></div>
                <div class="form-group"><label>Custom Code (optional)</label><input type="text" x-model="code" placeholder="my-link"></div>
            </div>
            <div class="form-group"><label>Expires At (optional)</label><input type="datetime-local" x-model="expires"></div>
            <button type="submit">Create Link</button>
        </form>
    </div>
    <div class="card">
        <h2>📋 All Links</h2>
        <div id="links-table" hx-get="/ui/links-table" hx-trigger="load, refresh from:.refresh" hx-swap="innerHTML">
            <div class="empty"><div class="icon">⏳</div>Loading...</div>
        </div>
    </div>
    "##,
    )
}

pub fn links_table_html() -> Html<String> {
    Html(r##"<table>
        <thead><tr><th>Code</th><th>Original URL</th><th>Clicks</th><th>Created</th><th>Actions</th></tr></thead>
        <tbody x-data="{links:[]}" x-init="fetch('/api/v1/links').then(r=>r.json()).then(d=>links=Array.isArray(d)?d:d.links||[])">
            <template x-for="link in links" :key="link.id">
                <tr>
                    <td><span class="badge badge-info" x-text="link.short_code"></span></td>
                    <td style="max-width:300px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap" x-text="link.original_url"></td>
                    <td x-text="link.click_count||0"></td>
                    <td x-text="link.created_at?link.created_at.slice(0,10):'-'"></td>
                    <td><button class="btn danger" style="padding:0.2rem 0.6rem;font-size:0.75rem" @click="fetch('/api/v1/links/'+link.id,{method:'DELETE'}).then(()=>links=links.filter(l=>l.id!==link.id))">Delete</button></td>
                </tr>
            </template>
        </tbody>
    </table>"##.to_string())
}

pub fn routes_page() -> Html<String> {
    page_shell(
        "Routes",
        "routes",
        r##"
    <div class="card">
        <h2>🛤️ Route Rules</h2>
        <p style="color:var(--dim);margin-bottom:1rem">Dynamic routing rules that direct visitors based on context</p>
        <div x-data="{routes:[]}" x-init="fetch('/api/v1/routes').then(r=>r.json()).then(d=>routes=Array.isArray(d)?d:d.routes||[])">
            <template x-for="route in routes" :key="route.id">
                <div class="card" style="margin-bottom:0.5rem;padding:1rem">
                    <div style="display:flex;justify-content:space-between;align-items:center">
                        <div><strong x-text="route.name||route.id"></strong> <span class="badge badge-info" x-text="route.condition_type||'default'"></span></div>
                        <button class="btn danger" style="padding:0.2rem 0.6rem;font-size:0.75rem" @click="fetch('/api/v1/routes/'+route.id,{method:'DELETE'}).then(()=>routes=routes.filter(r=>r.id!==route.id))">Delete</button>
                    </div>
                </div>
            </template>
            <div x-show="routes.length===0" class="empty"><div class="icon">🛤️</div>No routes configured</div>
        </div>
    </div>
    "##,
    )
}

pub fn extensions_page() -> Html<String> {
    page_shell(
        "Extensions",
        "extensions",
        r##"
    <div class="card">
        <h2>🧩 Registered Extensions</h2>
        <p style="color:var(--dim);margin-bottom:1rem">Extensions extend OpenLink capabilities through the Extension Registry</p>
        <div x-data="{exts:[]}" x-init="fetch('/api/v1/extensions').then(r=>r.json()).then(d=>exts=Array.isArray(d)?d:d.extensions||[])">
            <table>
                <thead><tr><th>Name</th><th>Type</th><th>Status</th></tr></thead>
                <tbody>
                    <template x-for="ext in exts" :key="ext.name">
                        <tr>
                            <td><strong x-text="ext.name"></strong></td>
                            <td><span class="badge badge-info" x-text="ext.extension_type||'custom'"></span></td>
                            <td><span class="badge badge-ok">Active</span></td>
                        </tr>
                    </template>
                </tbody>
            </table>
            <div x-show="exts.length===0" class="empty"><div class="icon">🧩</div>No extensions registered</div>
        </div>
    </div>
    "##,
    )
}

pub fn extensions_list_html() -> Html<String> {
    Html(r##"<div x-data="{exts:[]}" x-init="fetch('/api/v1/extensions').then(r=>r.json()).then(d=>exts=Array.isArray(d)?d:d.extensions||[])">
        <template x-for="ext in exts.slice(0,5)" :key="ext.name">
            <div style="padding:0.4rem 0;border-bottom:1px solid var(--border)">
                <strong x-text="ext.name"></strong> <span class="badge badge-info" x-text="ext.extension_type||'custom'"></span>
            </div>
        </template>
        <div x-show="exts.length===0" style="color:var(--dim)">No extensions</div>
    </div>"##.to_string())
}

pub fn agent_page() -> Html<String> {
    page_shell(
        "Agent",
        "agent",
        r##"
    <div class="card">
        <h2>🤖 Agent Discovery</h2>
        <p style="color:var(--dim);margin-bottom:1rem">Person Agent Schema — identity and capabilities for AI agents</p>
        <div hx-get="/.well-known/agent.json" hx-target="#agent-json" hx-trigger="load" hx-swap="innerHTML"></div>
        <pre id="agent-json" style="min-height:200px">Loading...</pre>
    </div>
    <div class="card">
        <h2>📋 API Endpoints</h2>
        <table>
            <thead><tr><th>Method</th><th>Path</th><th>Description</th></tr></thead>
            <tbody>
                <tr><td><span class="badge badge-ok">GET</span></td><td>/api/v1/links</td><td>List all links</td></tr>
                <tr><td><span class="badge badge-info">POST</span></td><td>/api/v1/links</td><td>Create a link</td></tr>
                <tr><td><span class="badge badge-ok">GET</span></td><td>/api/v1/links/:id</td><td>Get link detail</td></tr>
                <tr><td><span class="badge badge-err">DELETE</span></td><td>/api/v1/links/:id</td><td>Delete a link</td></tr>
                <tr><td><span class="badge badge-info">POST</span></td><td>/api/v1/routes</td><td>Create route rule</td></tr>
                <tr><td><span class="badge badge-ok">GET</span></td><td>/api/v1/extensions</td><td>List extensions</td></tr>
                <tr><td><span class="badge badge-ok">GET</span></td><td>/api/v1/stats/overview</td><td>Overview stats</td></tr>
                <tr><td><span class="badge badge-info">POST</span></td><td>/api/v1/agent/resolve</td><td>Agent batch resolve</td></tr>
                <tr><td><span class="badge badge-ok">GET</span></td><td>/.well-known/agent.json</td><td>Agent discovery</td></tr>
                <tr><td><span class="badge badge-ok">GET</span></td><td>/health</td><td>Health check</td></tr>
            </tbody>
        </table>
    </div>
    "##,
    )
}
