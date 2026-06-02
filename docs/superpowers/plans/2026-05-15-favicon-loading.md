# Favicon Loading — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add favicon fetching to URL Album 2 — single-bookmark and folder-recursive (background batch with non-modal progress panel).

**Architecture:** Rust `fetch_favicon` command fetches one favicon per call (cache-by-domain in `Data/favicons/`), updates DB with filename only. JS manages a concurrency-limited queue (5 workers), deduplicates by domain, updates DOM progressively. Progress shown in a bottom panel matching the existing checker-panel pattern.

**Tech Stack:** Rust/Tauri 2, rusqlite, reqwest 0.12 async (rustls-tls), Vanilla JS

**Spec:** `docs/superpowers/specs/2026-05-15-favicon-design.md`

---

## File Map

| File | Changes |
|---|---|
| `src-tauri/src/db.rs` | Add `favicon` field to structs + queries + migration |
| `src-tauri/src/main.rs` | Add `get_data_dir`, helper fns, `fetch_favicon` command |
| `ui/index.html` | Add `#favicon-panel`, `#detail-favicon` img |
| `ui/style.css` | Add `.favicon-icon`, `#favicon-panel` styles |
| `ui/app.js` | All JS: constant, startup, display, queue, panel, context menus |

---

## Task 1: DB — Add `favicon` field to structs, migration, queries

**Files:**
- Modify: `src-tauri/src/db.rs:8-29` (structs)
- Modify: `src-tauri/src/db.rs:33-53` (init)
- Modify: `src-tauri/src/db.rs:74-99` (get_tree)
- Modify: `src-tauri/src/db.rs:102-119` (get_bookmarks)
- Modify: `src-tauri/src/db.rs:121-167` (search_bookmarks)

- [ ] **Step 1: Add `favicon` field to `TreeNode`, `Bookmark`, `SearchResult`**

In `db.rs`, add `pub favicon: Option<String>` to each struct:

```rust
// TreeNode (line ~8): add after `thumb`
pub struct TreeNode {
    pub id:      i64,
    pub parent:  Option<i64>,
    pub kind:    String,
    pub title:   String,
    pub url:     Option<String>,
    pub thumb:   Option<String>,
    pub favicon: Option<String>,   // ← new
    pub note:    Option<String>,
    pub created: Option<String>,
    pub visited: Option<String>,
    pub count:   i64,
}

// Bookmark (line ~23): add after `thumb`
pub struct Bookmark {
    pub id:     i64,
    pub title:  String,
    pub url:    String,
    pub thumb:  Option<String>,
    pub favicon: Option<String>,   // ← new
    pub note:   Option<String>,
}

// SearchResult (line ~55): add after `thumb`
pub struct SearchResult {
    pub id:     i64,
    pub parent: Option<i64>,
    pub kind:   String,
    pub title:  String,
    pub url:    String,
    pub thumb:  Option<String>,
    pub favicon: Option<String>,   // ← new
    pub note:   Option<String>,
}
```

- [ ] **Step 2: Add migration to `init()`**

After the `conn.execute_batch(...)` call in `init()` (line ~33), add:

```rust
pub fn init(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA synchronous  = FULL;
         PRAGMA wal_checkpoint(PASSIVE);
         CREATE TABLE IF NOT EXISTS nodes (
             id       INTEGER PRIMARY KEY AUTOINCREMENT,
             parent   INTEGER,
             kind     TEXT NOT NULL DEFAULT 'bookmark',
             title    TEXT NOT NULL,
             url      TEXT,
             thumb    TEXT,
             note     TEXT,
             created  TEXT,
             visited  TEXT,
             sort_idx INTEGER DEFAULT 0
         );
         CREATE INDEX IF NOT EXISTS idx_parent
             ON nodes (parent, kind, sort_idx);",
    )?;
    // Migration: add favicon column if absent (silent on existing DBs)
    conn.execute("ALTER TABLE nodes ADD COLUMN favicon TEXT", []).ok();
    Ok(())
}
```

- [ ] **Step 3: Update `get_tree` query to include `favicon`**

Replace the SQL in `get_tree` so `favicon` is selected at column index 9, `count` moves to index 10:

```rust
pub fn get_tree(conn: &Connection) -> Result<Vec<TreeNode>> {
    let mut stmt = conn.prepare(
        "SELECT id, parent, kind, title, url, thumb, note, created, visited, favicon,
                CASE WHEN kind = 'folder'
                     THEN (SELECT COUNT(*) FROM nodes b
                           WHERE b.parent = nodes.id AND b.kind = 'bookmark')
                     ELSE 0
                END AS count
         FROM nodes
         ORDER BY sort_idx, id",
    )?;
    let result: rusqlite::Result<Vec<TreeNode>> = stmt.query_map([], |row| {
        Ok(TreeNode {
            id:      row.get(0)?,
            parent:  row.get(1)?,
            kind:    row.get(2)?,
            title:   row.get(3)?,
            url:     row.get(4)?,
            thumb:   row.get(5)?,
            note:    row.get(6)?,
            created: row.get(7)?,
            visited: row.get(8)?,
            favicon: row.get(9)?,
            count:   row.get(10)?,
        })
    })?.collect();
    result
}
```

- [ ] **Step 4: Update `get_bookmarks` query**

```rust
pub fn get_bookmarks(conn: &Connection, folder_id: i64) -> Result<Vec<Bookmark>> {
    let mut stmt = conn.prepare(
        "SELECT id, title, url, thumb, note, favicon
         FROM nodes
         WHERE kind = 'bookmark' AND parent = ?1
         ORDER BY sort_idx, id",
    )?;
    let result: rusqlite::Result<Vec<Bookmark>> = stmt.query_map(params![folder_id], |row| {
        Ok(Bookmark {
            id:     row.get(0)?,
            title:  row.get(1)?,
            url:    row.get::<_, Option<String>>(2)?.unwrap_or_default(),
            thumb:  row.get(3)?,
            note:   row.get(4)?,
            favicon: row.get(5)?,
        })
    })?.collect();
    result
}
```

- [ ] **Step 5: Update `search_bookmarks` query**

In both SQL branches of `search_bookmarks`, add `favicon` at column index 7:

```rust
// Both the "folders only" and "combined" branches:
"SELECT id, parent, kind, title, url, thumb, note, favicon
 FROM nodes
 WHERE kind = 'folder' AND title LIKE ?1
 ORDER BY title"

// and:
format!(
    "SELECT id, parent, kind, title, url, thumb, note, favicon
     FROM nodes
     WHERE (kind = 'folder' AND title LIKE ?1)
        OR (kind = 'bookmark' AND ({bm}))
     ORDER BY CASE kind WHEN 'folder' THEN 0 ELSE 1 END, title",
    bm = bm_conds.join(" OR ")
)
```

Update the row mapper to read `favicon` at index 7:

```rust
|row| Ok(SearchResult {
    id:     row.get(0)?,
    parent: row.get(1)?,
    kind:   row.get(2)?,
    title:  row.get(3)?,
    url:    row.get::<_, Option<String>>(4)?.unwrap_or_default(),
    thumb:  row.get(5)?,
    note:   row.get(6)?,
    favicon: row.get(7)?,
})
```

- [ ] **Step 6: Verify compilation**

```powershell
cd src-tauri; cargo check 2>&1
```

Expected: no errors. If struct field errors appear, double-check index assignments.

- [ ] **Step 7: Commit**

```powershell
git add src-tauri/src/db.rs
git commit -m "feat(db): add favicon column — migration, structs, query updates"
```

---

## Task 2: Rust helper functions (`main.rs`)

**Files:**
- Modify: `src-tauri/src/main.rs` — add after `normalize_url` (~line 570)

- [ ] **Step 1: Add domain extraction + sanitize helpers**

Add these four functions after `normalize_url` in `main.rs`:

```rust
fn extract_domain(url: &str) -> Option<String> {
    let url = url.trim();
    let after_scheme = if let Some(pos) = url.find("://") {
        &url[pos + 3..]
    } else {
        url
    };
    let host = after_scheme.split(|c: char| c == '/' || c == '?' || c == '#').next()?;
    let host = host.split('@').last().unwrap_or(host);
    let host = host.split(':').next().unwrap_or(host).trim().to_lowercase();
    if host.is_empty() { return None; }
    Some(host.strip_prefix("www.").unwrap_or(&host).to_string())
}

fn sanitize_domain(domain: &str) -> String {
    domain.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' { c } else { '_' })
        .collect::<String>()
        .to_lowercase()
}

fn ext_from_content_type(ct: &str) -> &'static str {
    if ct.contains("svg")         { "svg"  }
    else if ct.contains("png")    { "png"  }
    else if ct.contains("gif")    { "gif"  }
    else if ct.contains("webp")   { "webp" }
    else                          { "ico"  }
}

fn find_icon_href(html: &str, base: &str) -> Option<String> {
    let lower = html.to_lowercase();
    let mut pos = 0;
    while pos < lower.len() {
        let Some(offset) = lower[pos..].find("<link") else { break };
        let link_start = pos + offset;
        let end_offset = lower[link_start..].find('>').unwrap_or(0);
        let link_end   = link_start + end_offset + 1;
        let tag_lower  = &lower[link_start..link_end];
        let tag_orig   = &html[link_start..link_end];

        if (tag_lower.contains("rel=\"icon\"")
            || tag_lower.contains("rel='icon'")
            || tag_lower.contains("shortcut icon")
            || tag_lower.contains("apple-touch-icon"))
            && tag_lower.contains("href=")
        {
            if let Some(href) = attr_value(tag_orig, "href") {
                if !href.is_empty() {
                    return Some(resolve_href(&href, base));
                }
            }
        }
        pos = link_end;
    }
    None
}

fn attr_value(tag: &str, attr: &str) -> Option<String> {
    let ltag  = tag.to_lowercase();
    let lattr = attr.to_lowercase();
    // Try double-quoted
    let dq = format!("{}=\"", lattr);
    if let Some(s) = ltag.find(&dq) {
        let vs = s + dq.len();
        if let Some(e) = ltag[vs..].find('"') {
            return Some(tag[vs..vs + e].to_string());
        }
    }
    // Try single-quoted
    let sq = format!("{}='", lattr);
    if let Some(s) = ltag.find(&sq) {
        let vs = s + sq.len();
        if let Some(e) = ltag[vs..].find('\'') {
            return Some(tag[vs..vs + e].to_string());
        }
    }
    None
}

fn resolve_href(href: &str, base: &str) -> String {
    if href.starts_with("http://") || href.starts_with("https://") {
        href.to_string()
    } else if href.starts_with("//") {
        format!("https:{}", href)
    } else if href.starts_with('/') {
        format!("{}{}", base, href)
    } else {
        format!("{}/{}", base.trim_end_matches('/'), href)
    }
}
```

- [ ] **Step 2: Verify compilation**

```powershell
cd src-tauri; cargo check 2>&1
```

Expected: no errors.

- [ ] **Step 3: Commit**

```powershell
git add src-tauri/src/main.rs
git commit -m "feat(favicon): add domain/icon URL helper functions"
```

---

## Task 3: Rust `get_data_dir` + `fetch_favicon` commands

**Files:**
- Modify: `src-tauri/src/main.rs` — add after the helpers from Task 2

- [ ] **Step 1: Add `get_data_dir` command**

```rust
#[tauri::command]
fn get_data_dir(state: tauri::State<AppState>) -> Result<String, String> {
    let dir = state.db_path.lock().map_err(|e| e.to_string())?
        .parent().ok_or("no parent dir")?.to_path_buf().join("Data");
    Ok(dir.to_string_lossy().into_owned())
}
```

- [ ] **Step 2: Add `fetch_favicon` async command**

Add this after `get_data_dir`. The function must NOT hold any `MutexGuard` across an `.await` point — each lock scope is explicit.

```rust
#[tauri::command]
async fn fetch_favicon(
    state: tauri::State<'_, AppState>,
    id: i64,
    url: String,
) -> Result<Option<String>, String> {
    // ── 1. Extract domain ────────────────────────────────────────────────
    let normalized = normalize_url(&url);
    let domain = match extract_domain(&normalized) {
        Some(d) if !d.is_empty() => d,
        _ => return Ok(None),
    };
    let safe = sanitize_domain(&domain);

    // ── 2. Build favicons dir (lock db_path briefly, then release) ────────
    let favicons_dir = {
        let p = state.db_path.lock().map_err(|e| e.to_string())?;
        p.parent().ok_or("no parent dir")?.to_path_buf()
            .join("Data").join("favicons")
    };
    std::fs::create_dir_all(&favicons_dir).map_err(|e| e.to_string())?;

    // ── 3. Cache hit: scan for {safe_domain}.* ────────────────────────────
    if let Ok(entries) = std::fs::read_dir(&favicons_dir) {
        for entry in entries.flatten() {
            let fname = entry.file_name().to_string_lossy().to_string();
            let stem  = std::path::Path::new(&fname)
                .file_stem().unwrap_or_default()
                .to_string_lossy().to_string();
            if stem == safe {
                let conn = state.db.lock().map_err(|e| e.to_string())?;
                conn.execute(
                    "UPDATE nodes SET favicon = ?1 WHERE id = ?2",
                    rusqlite::params![fname, id],
                ).map_err(|e| e.to_string())?;
                return Ok(Some(fname));
            }
        }
    }

    // ── 4. HTTP client ───────────────────────────────────────────────────
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .user_agent("Mozilla/5.0 (compatible; url-album/2.0)")
        .build()
        .map_err(|e| e.to_string())?;

    // ── 5. Attempt favicon.ico ────────────────────────────────────────────
    let favicon_ico = format!("https://{}/favicon.ico", domain);
    let (raw_bytes, ext) = match client.get(&favicon_ico).send().await {
        Ok(resp) if resp.status().is_success() => {
            let ct  = resp.headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_string();
            let ext = ext_from_content_type(&ct);
            match resp.bytes().await {
                Ok(b) if b.len() > 20 => (Some(b), ext),
                _ => (None, "ico"),
            }
        }
        _ => (None, "ico"),
    };

    // ── 6. Fallback: parse HTML <head> for <link rel="icon"> ─────────────
    let (raw_bytes, ext) = if raw_bytes.is_none() {
        let page = format!("https://{}/", domain);
        let base = format!("https://{}", domain);
        match client.get(&page).send().await {
            Ok(resp) => match resp.text().await {
                Ok(html) => match find_icon_href(&html, &base) {
                    Some(icon_url) => match client.get(&icon_url).send().await {
                        Ok(r2) if r2.status().is_success() => {
                            let ct  = r2.headers()
                                .get("content-type")
                                .and_then(|v| v.to_str().ok())
                                .unwrap_or("")
                                .to_string();
                            let ext = ext_from_content_type(&ct);
                            match r2.bytes().await {
                                Ok(b) if b.len() > 20 => (Some(b), ext),
                                _ => (None, "ico"),
                            }
                        }
                        _ => (None, "ico"),
                    },
                    None => (None, "ico"),
                },
                _ => (None, "ico"),
            },
            _ => (None, "ico"),
        }
    } else {
        (raw_bytes, ext)
    };

    // ── 7. Nothing found — return None (JS shows ● fallback) ─────────────
    let bytes = match raw_bytes {
        Some(b) => b,
        None => return Ok(None),
    };

    // ── 8. Save file + update DB ─────────────────────────────────────────
    let filename  = format!("{}.{}", safe, ext);
    let file_path = favicons_dir.join(&filename);
    std::fs::write(&file_path, &bytes).map_err(|e| e.to_string())?;

    {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE nodes SET favicon = ?1 WHERE id = ?2",
            rusqlite::params![filename, id],
        ).map_err(|e| e.to_string())?;
    }

    Ok(Some(filename))
}
```

- [ ] **Step 3: Verify compilation**

```powershell
cd src-tauri; cargo check 2>&1
```

Expected: no errors. Common issues:
- `'_` lifetime on `State` — required for async commands in Tauri 2, already shown above.
- If `reqwest` async types error: confirm `Cargo.toml` has `reqwest = { version = "0.12", default-features = false, features = ["rustls-tls"] }`.

- [ ] **Step 4: Commit**

```powershell
git add src-tauri/src/main.rs
git commit -m "feat(favicon): add get_data_dir and fetch_favicon Tauri commands"
```

---

## Task 4: Register new commands + full build check

**Files:**
- Modify: `src-tauri/src/main.rs:1271-1327` (invoke_handler)

- [ ] **Step 1: Add commands to invoke_handler**

In `main.rs`, find the `.invoke_handler(tauri::generate_handler![` block (line ~1271). Add the two new commands:

```rust
.invoke_handler(tauri::generate_handler![
    // ... existing commands ...
    get_data_dir,    // ← add
    fetch_favicon,   // ← add
])
```

Place them just before the closing `])`, after `checkpoint_db`.

- [ ] **Step 2: Full build**

```powershell
cd src-tauri; cargo build 2>&1
```

Expected: compiles successfully. Fix any errors before proceeding.

- [ ] **Step 3: Commit**

```powershell
git add src-tauri/src/main.rs
git commit -m "feat(favicon): register get_data_dir and fetch_favicon in invoke_handler"
```

---

## Task 5: HTML — add `#favicon-panel` and `#detail-favicon`

**Files:**
- Modify: `ui/index.html`

- [ ] **Step 1: Add `#detail-favicon` img to detail-url-row**

Find (line ~98):
```html
<div id="detail-url-row">
  <span id="detail-url"></span>
```

Replace with:
```html
<div id="detail-url-row">
  <img id="detail-favicon" class="favicon-icon hidden" src="" alt="">
  <span id="detail-url"></span>
```

- [ ] **Step 2: Add `#favicon-panel` before `</body>`**

Find the closing `<!-- Context menu (built entirely by JS) -->` block (line ~495). Add the favicon panel just before it, after `#checker-panel`:

```html
<!-- Favicon loader panel (non-modal background task) -->
<div id="favicon-panel" class="hidden">
  <div id="fv-titlebar">
    <span id="fv-title">Загрузка favicon</span>
    <button id="fv-close-btn" title="Закрыть">×</button>
  </div>
  <div id="fv-body">
    <div id="fv-info-row">
      <span>favicon: <b id="fv-done">0</b>/<b id="fv-total">0</b></span>
      <span id="fv-domain"></span>
    </div>
    <div id="fv-bar-track"><div id="fv-bar-fill"></div></div>
    <div id="fv-btn-row">
      <button class="win-btn" id="fv-cancel-btn">Отмена</button>
    </div>
  </div>
</div>
```

- [ ] **Step 3: Commit**

```powershell
git add ui/index.html
git commit -m "feat(favicon): add favicon-panel and detail-favicon elements to HTML"
```

---

## Task 6: CSS — favicon icon + panel styles

**Files:**
- Modify: `ui/style.css` — append at end

- [ ] **Step 1: Add all favicon CSS**

Append to the end of `style.css`:

```css
/* ── Favicon icon (16×16, replaces ● dot) ────────────────────────────────── */
.favicon-icon {
  width: 16px;
  height: 16px;
  object-fit: contain;
  flex-shrink: 0;
  image-rendering: pixelated;
  display: inline-block;
  vertical-align: middle;
}
.favicon-icon.hidden { display: none !important; }

/* ── Favicon loader panel ─────────────────────────────────────────────────── */
#favicon-panel {
  position: fixed;
  bottom: 24px;
  left: 24px;
  width: 360px;
  background: #f0f0f0;
  border: 1px solid #767676;
  box-shadow: 3px 3px 10px rgba(0,0,0,0.32);
  z-index: 500;
  font-family: "Segoe UI", system-ui, sans-serif;
  font-size: 12px;
  color: #000;
}
[data-theme="dark"] #favicon-panel {
  background: #2d2d2d;
  border-color: #555;
  color: #e0e0e0;
}

#fv-titlebar {
  background: #0078d4;
  color: #fff;
  padding: 4px 6px 4px 10px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  user-select: none;
}
#fv-title { font-size: 12px; }
#fv-close-btn {
  background: none;
  border: none;
  color: #fff;
  font-size: 14px;
  line-height: 1;
  padding: 0 6px 1px;
  cursor: pointer;
}
#fv-close-btn:hover { background: #e81123; }

#fv-body { padding: 8px 10px 8px; display: flex; flex-direction: column; gap: 5px; }

#fv-info-row {
  display: flex;
  justify-content: space-between;
  align-items: baseline;
  gap: 8px;
  font-size: 12px;
}
#fv-domain {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: #555;
  font-size: 11px;
  text-align: right;
}
[data-theme="dark"] #fv-domain { color: #aaa; }

#fv-bar-track {
  height: 14px;
  background: #ddd;
  border: 1px inset #aaa;
  overflow: hidden;
}
[data-theme="dark"] #fv-bar-track { background: #444; border-color: #666; }

#fv-bar-fill {
  height: 100%;
  background: #0078d4;
  width: 0%;
  transition: width 0.25s;
}

#fv-btn-row { display: flex; justify-content: flex-end; }
```

- [ ] **Step 2: Commit**

```powershell
git add ui/style.css
git commit -m "feat(favicon): add CSS for favicon-icon and favicon-panel"
```

---

## Task 7: JS — constant, startup, helper functions

**Files:**
- Modify: `ui/app.js` — top of file (constants section) + `init()` function

- [ ] **Step 1: Add `MAX_FAVICON_CONCURRENCY` constant**

Find the top of `app.js` (lines 1-10, after the `const { invoke }` declarations). Add:

```js
const MAX_FAVICON_CONCURRENCY = 5; // intentional per-domain rate limiting
```

- [ ] **Step 2: Add `dataDir` state variable**

Find the section where global state variables are declared (near `let allNodes`, `let allFolders`, etc.). Add:

```js
let dataDir = ""; // absolute path to Data/ dir, set at startup via get_data_dir
```

- [ ] **Step 3: Load `dataDir` in `init()`**

In `init()` (line ~3113), add the `get_data_dir` call inside `Promise.all`:

```js
async function init() {
  await Promise.all([
    loadBrowsersConfig(),
    loadToolbarConfig(),
    loadAppSettings(),
    invoke('get_data_dir').then(d => { dataDir = d; }).catch(() => {}),
  ]);
  buildToolbar();
  await showApp();
}
```

- [ ] **Step 4: Add `extractDomain` helper**

Add near other utility helpers (e.g., after `normalize_url` JS equivalent or near `makeNoImg`):

```js
function extractDomain(url) {
  try {
    const u = new URL(url.startsWith('http') ? url : 'https://' + url);
    return u.hostname.replace(/^www\./, '').toLowerCase();
  } catch { return null; }
}
```

- [ ] **Step 5: Add `setFaviconOnEl` helper**

```js
function setFaviconOnEl(el, src) {
  const img = document.createElement('img');
  img.src = src;
  img.className = 'favicon-icon';
  img.onerror = () => img.remove(); // if load fails, ● text stays visible
  el.innerHTML = '';
  el.appendChild(img);
}
```

- [ ] **Step 6: Commit**

```powershell
git add ui/app.js
git commit -m "feat(favicon): add constant, dataDir state, startup load, and helpers"
```

---

## Task 8: JS — Display favicon in `createCard`, `createTreeNode`, `showDetailView`

**Files:**
- Modify: `ui/app.js` — three render functions

- [ ] **Step 1: Update `createCard` to show favicon**

In `createCard` (line ~3952), the `dot` span currently sets `"●"`. Replace with favicon-aware version:

```js
// Replace:
const dot = document.createElement("span");
dot.className = "row-dot";
dot.textContent = "●";

// With:
const dot = document.createElement("span");
dot.className = "row-dot";
if (b.favicon && dataDir) {
  setFaviconOnEl(dot, convertFileSrc(dataDir + '/favicons/' + b.favicon));
} else {
  dot.textContent = "●";
}
```

- [ ] **Step 2: Update `createTreeNode` bookmark leaf to show favicon**

In `createTreeNode` (line ~3419), the `icon` span for bookmark leaves:

```js
// Replace:
const icon = document.createElement("span");
icon.className = "tree-link-icon";
icon.textContent = "●";

// With:
const icon = document.createElement("span");
icon.className = "tree-link-icon";
if (node.favicon && dataDir) {
  setFaviconOnEl(icon, convertFileSrc(dataDir + '/favicons/' + node.favicon));
} else {
  icon.textContent = "●";
}
```

- [ ] **Step 3: Update `showDetailView` to show favicon**

In `showDetailView` (line ~3701), after setting `detailUrlEl.textContent = url`:

```js
// After: detailUrlEl.title = url;
// Add:
const detailFavEl = document.getElementById('detail-favicon');
if (detailFavEl) {
  if (node.favicon && dataDir) {
    detailFavEl.src = convertFileSrc(dataDir + '/favicons/' + node.favicon);
    detailFavEl.classList.remove('hidden');
    detailFavEl.onerror = () => detailFavEl.classList.add('hidden');
  } else {
    detailFavEl.classList.add('hidden');
    detailFavEl.src = '';
  }
}
```

- [ ] **Step 4: Build + verify rendering**

```powershell
cd src-tauri; cargo build 2>&1
```

Then launch the exe. Navigate to any folder with bookmarks — they should still show `●` (no favicons loaded yet). No regressions expected.

```powershell
Start-Process ".\target\debug\url-album.exe" -WorkingDirectory ".\target\debug"
```

- [ ] **Step 5: Commit**

```powershell
git add ui/app.js
git commit -m "feat(favicon): render favicon in grid rows, tree leaves, and detail view"
```

---

## Task 9: JS — Queue engine

**Files:**
- Modify: `ui/app.js` — add after favicon display helpers

- [ ] **Step 1: Add queue state variables**

Add near other global state variables (near `let _dragNode`, etc.):

```js
// ── Favicon queue state ───────────────────────────────────────────────────
let _faviconQueue     = [];   // Array<{id, url, domain, sameIds: number[]}>
let _faviconActive    = 0;    // current in-flight invoke count
let _faviconCancelled = false;
let _faviconTotal     = 0;    // total unique domains queued
let _faviconDone      = 0;    // completed (success or failure)
```

- [ ] **Step 2: Add `updateFaviconInDOM`**

```js
function updateFaviconInDOM(nodeId, filePath) {
  const src = convertFileSrc(filePath);

  // Grid row
  const card = gridEl.querySelector(`.card[data-id="${nodeId}"]`);
  if (card) {
    const dot = card.querySelector('.row-dot');
    if (dot) setFaviconOnEl(dot, src);
  }

  // Tree leaf
  const treeItem = treeEl.querySelector(`.tree-item[data-id="${nodeId}"]`);
  if (treeItem) {
    const icon = treeItem.querySelector('.tree-link-icon');
    if (icon) setFaviconOnEl(icon, src);
  }

  // Detail view
  if (activeBookmarkNode?.id === nodeId) {
    const detailFav = document.getElementById('detail-favicon');
    if (detailFav) {
      detailFav.src = src;
      detailFav.classList.remove('hidden');
      detailFav.onerror = () => detailFav.classList.add('hidden');
    }
  }
}
```

- [ ] **Step 3: Add `applyFaviconToDOM`**

```js
function applyFaviconToDOM(item, filename) {
  const filePath = dataDir + '/favicons/' + filename;

  // Update allNodes for primary + sameIds
  const primary = allNodes.find(n => n.id === item.id);
  if (primary) primary.favicon = filename;
  for (const sid of item.sameIds) {
    const sn = allNodes.find(n => n.id === sid);
    if (sn) sn.favicon = filename;
  }

  // Update DOM for primary
  updateFaviconInDOM(item.id, filePath);

  // Update DOM for same-domain nodes
  for (const sid of item.sameIds) {
    updateFaviconInDOM(sid, filePath);
  }
}
```

- [ ] **Step 4: Add `_runFaviconWorker` + `startFaviconWorkers`**

```js
function _runFaviconWorker() {
  if (_faviconCancelled || _faviconQueue.length === 0 || _faviconActive >= MAX_FAVICON_CONCURRENCY) return;
  _faviconActive++;
  const item = _faviconQueue.shift();

  // Update domain display in panel
  const domainEl = document.getElementById('fv-domain');
  if (domainEl) domainEl.textContent = item.domain;

  invoke('fetch_favicon', { id: item.id, url: item.url })
    .then(filename => {
      if (filename) applyFaviconToDOM(item, filename);
    })
    .catch(() => {}) // silent fallback — ● stays
    .finally(() => {
      _faviconDone++;
      _faviconActive--;
      _updateFaviconPanelProgress();
      if (_faviconQueue.length === 0 && _faviconActive === 0) {
        _finishFaviconBatch();
      } else {
        _runFaviconWorker();
      }
    });
}

function startFaviconWorkers() {
  for (let i = 0; i < MAX_FAVICON_CONCURRENCY; i++) _runFaviconWorker();
}
```

- [ ] **Step 5: Commit**

```powershell
git add ui/app.js
git commit -m "feat(favicon): add JS queue engine — state, DOM updater, worker loop"
```

---

## Task 10: JS — Progress panel functions

**Files:**
- Modify: `ui/app.js`

- [ ] **Step 1: Add panel show/update/finish functions**

```js
// ── Favicon panel ─────────────────────────────────────────────────────────
const _faviconPanelEl = () => document.getElementById('favicon-panel');

function showFaviconPanel(total) {
  _faviconTotal  = total;
  _faviconDone   = 0;
  const panel = _faviconPanelEl();
  if (!panel) return;
  document.getElementById('fv-done').textContent  = '0';
  document.getElementById('fv-total').textContent = total;
  document.getElementById('fv-bar-fill').style.width = '0%';
  document.getElementById('fv-domain').textContent   = '';
  panel.classList.remove('hidden');
}

function _updateFaviconPanelProgress() {
  document.getElementById('fv-done').textContent = _faviconDone;
  const pct = _faviconTotal > 0 ? Math.round(_faviconDone / _faviconTotal * 100) : 0;
  document.getElementById('fv-bar-fill').style.width = pct + '%';
}

function hideFaviconPanel() {
  _faviconPanelEl()?.classList.add('hidden');
}

function _finishFaviconBatch() {
  document.getElementById('fv-domain').textContent = 'Готово';
  setTimeout(hideFaviconPanel, 2000);
}
```

- [ ] **Step 2: Wire Cancel and Close buttons**

Find where other panel buttons are wired (e.g., near `chk-stop` wiring). Add:

```js
document.getElementById('fv-cancel-btn').addEventListener('click', () => {
  _faviconCancelled = true;
  _faviconQueue = [];
  hideFaviconPanel();
});

document.getElementById('fv-close-btn').addEventListener('click', () => {
  _faviconCancelled = true;
  _faviconQueue = [];
  hideFaviconPanel();
});
```

- [ ] **Step 3: Commit**

```powershell
git add ui/app.js
git commit -m "feat(favicon): add progress panel show/update/finish/cancel functions"
```

---

## Task 11: JS — `loadSingleFavicon` + `startFaviconBatch`

**Files:**
- Modify: `ui/app.js`

- [ ] **Step 1: Add `collectBookmarksRecursive`**

```js
function collectBookmarksRecursive(folderId) {
  const result = [];
  const queue = [folderId];
  while (queue.length > 0) {
    const id = queue.shift();
    for (const n of allNodes) {
      if (n.parent !== id) continue;
      if (n.kind === 'bookmark' && n.url) result.push(n);
      else if (n.kind === 'folder') queue.push(n.id);
    }
  }
  return result;
}
```

- [ ] **Step 2: Add `buildFaviconQueue` with domain dedup**

This is the intentional per-domain rate limiting. One queue item per domain; `sameIds` holds all other node IDs sharing the same domain that will be updated when the primary resolves.

```js
function buildFaviconQueue(bookmarks) {
  const domainMap = new Map(); // domain -> queue item index
  const queue = [];

  for (const node of bookmarks) {
    const domain = extractDomain(node.url);
    if (!domain) continue;
    if (domainMap.has(domain)) {
      // Attach to existing item as same-domain node
      queue[domainMap.get(domain)].sameIds.push(node.id);
    } else {
      domainMap.set(domain, queue.length);
      queue.push({ id: node.id, url: node.url, domain, sameIds: [] });
    }
  }
  return queue;
}
```

- [ ] **Step 3: Add `loadSingleFavicon`**

Single bookmark: no panel, silent update.

```js
async function loadSingleFavicon(node) {
  if (!node.url) return;
  try {
    const filename = await invoke('fetch_favicon', { id: node.id, url: node.url });
    if (filename) {
      const n = allNodes.find(n => n.id === node.id);
      if (n) n.favicon = filename;
      updateFaviconInDOM(node.id, dataDir + '/favicons/' + filename);
    }
  } catch(e) {
    console.error('loadSingleFavicon:', e);
  }
}
```

- [ ] **Step 4: Add `startFaviconBatch`**

```js
function startFaviconBatch(folderNode, recursive = true) {
  // Stop any running batch
  _faviconCancelled = false;
  _faviconQueue = [];
  _faviconActive = 0;

  const bookmarks = recursive
    ? collectBookmarksRecursive(folderNode.id)
    : allNodes.filter(n => n.parent === folderNode.id && n.kind === 'bookmark' && n.url);

  if (bookmarks.length === 0) return;

  _faviconQueue = buildFaviconQueue(bookmarks);
  if (_faviconQueue.length === 0) return;

  showFaviconPanel(_faviconQueue.length);
  startFaviconWorkers();
}
```

- [ ] **Step 5: Commit**

```powershell
git add ui/app.js
git commit -m "feat(favicon): add collect, dedup queue builder, single and batch load"
```

---

## Task 12: JS — Context menu items

**Files:**
- Modify: `ui/app.js` — two functions: `showContextMenu` and `showFolderContextMenu`

- [ ] **Step 1: Add "Загрузить favicon" to bookmark context menu**

In `showContextMenu` (line ~1423), find the block that adds "Обновить рисунок":

```js
ctxMenuEl.appendChild(
  ctxItem("refresh", "Обновить рисунок", null,
    () => refreshThumb(node), !node.url)
);
```

Add immediately after it:

```js
ctxMenuEl.appendChild(
  ctxItem("favicon", "Загрузить favicon", null,
    () => { hideContextMenu(); loadSingleFavicon(node); },
    !node.url)
);
```

- [ ] **Step 2: Add "Загрузить favicon'ы" to folder context menu**

In `showFolderContextMenu` (line ~849), find the block that adds "Проверить":

```js
ctxMenuEl.appendChild(ctxItem("verify", "Проверить", null, () => {
  hideContextMenu();
  openCheckerPanel(folderNode);
}));
```

Add immediately after it:

```js
ctxMenuEl.appendChild(ctxItem("favicon", "Загрузить favicon'ы", null, () => {
  hideContextMenu();
  startFaviconBatch(folderNode, true);
}));
```

- [ ] **Step 3: Commit**

```powershell
git add ui/app.js
git commit -m "feat(favicon): add context menu items for single and batch favicon loading"
```

---

## Task 13: Full build + smoke test

**Files:** None (verification only)

- [ ] **Step 1: Full build**

```powershell
cd src-tauri; cargo build 2>&1
```

Expected: `Compiling url-album ...` then `Finished`. No errors.

- [ ] **Step 2: Launch**

```powershell
Start-Process ".\target\debug\url-album.exe" -WorkingDirectory ".\target\debug"
```

- [ ] **Step 3: Smoke test — single favicon**

1. Right-click any bookmark in the grid or tree.
2. Verify "Загрузить favicon" appears in the context menu, after "Обновить рисунок".
3. Click "Загрузить favicon".
4. After a few seconds: the `●` in the grid row and tree leaf should become a small site icon.
5. Double-click the bookmark to open detail view — favicon should appear before the URL.
6. Right-click again → "Загрузить favicon" → must be instant (cache hit, no HTTP).
7. Check `target\debug\Data\favicons\` — file should exist (e.g., `github.com.ico`).

- [ ] **Step 4: Smoke test — folder batch**

1. Right-click a folder with several bookmarks.
2. Verify "Загрузить favicon'ы" appears after "Проверить".
3. Click it.
4. Verify `#favicon-panel` appears bottom-left with counter `favicon: 0/N`.
5. Icons should appear progressively in the grid/tree as batch runs.
6. Counter increments; domain name updates.
7. Panel disappears 2 seconds after completion.

- [ ] **Step 5: Smoke test — cancel**

1. Start a folder batch on a large folder (50+ unique domains).
2. Click "Отмена" while running.
3. Panel disappears. No new favicons load. App remains fully responsive.

- [ ] **Step 6: Smoke test — navigation during batch**

1. Start a large folder batch.
2. While it runs: click other folders, open/close tree nodes, open detail views.
3. UI must not freeze. Batch continues in background.

- [ ] **Step 7: Final commit**

```powershell
git add -A
git commit -m "feat: favicon loading complete — single, batch, domain cache, progress panel"
```

---

## Self-Review

**Spec coverage check:**

| Spec section | Covered by task |
|---|---|
| DB migration (`favicon TEXT`) | Task 1 Step 2 |
| Filename-only in DB | Task 3 Step 2 (`filename` not full path) |
| `Data/favicons/{domain}.{ext}` cache | Task 3 Step 2 |
| Real extension from content-type | Task 2 Step 1 (`ext_from_content_type`) |
| Domain cache hit (no HTTP) | Task 3 Step 2 (scan + early return) |
| `fetch_favicon` command | Task 3 |
| `get_data_dir` command | Task 3 Step 1 |
| `MAX_FAVICON_CONCURRENCY = 5` constant | Task 7 Step 1 |
| Domain dedup = intentional rate limiting | Task 11 Step 2 + comment |
| 8s timeout | Task 3 Step 4 |
| favicon.ico → HTML fallback | Task 3 Steps 5–6 |
| `Data/favicons/` auto-create | Task 3 Step 2 |
| Tree favicon display | Task 8 Step 2 |
| Grid favicon display | Task 8 Step 1 |
| Detail view favicon before URL | Task 8 Step 3 + Task 5 Step 1 |
| favicon `onerror` → ● fallback | Tasks 7+8 (`setFaviconOnEl`) |
| Bookmark context menu item | Task 12 Step 1 |
| Folder context menu item (recursive) | Task 12 Step 2 |
| Non-modal progress panel | Tasks 5, 6, 10 |
| Panel: counter + domain + bar + cancel | Tasks 5, 10 |
| Cancel = drain queue | Task 10 Step 2 |
| Panel hides 2s after finish | Task 10 Step 1 |
| Single favicon = no panel | Task 11 Step 3 |
| allNodes updated after fetch | Tasks 9+11 |
| Export unchanged | Not touched ✓ |
| Import: favicon = null | Not touched ✓ |
| Orphaned cleanup: out of scope | Not in plan ✓ |
