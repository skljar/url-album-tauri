# Favicon Loading — Design Spec

**Date:** 2026-05-15  
**Project:** URL Album 2 (Tauri 2 + Rust + Vanilla JS)  
**Status:** Approved

---

## Summary

Add favicon loading to the bookmark manager:
- Single bookmark: context menu → "Загрузить favicon"
- Folder (recursive): context menu → "Загрузить favicon'ы"
- Background batch with non-modal progress panel (Total Commander / old Opera UX)
- Favicons displayed in tree, grid rows, and detail view

---

## 1. Database

### Migration

```sql
ALTER TABLE nodes ADD COLUMN favicon TEXT;
```

Added in `db::init()` via `.ok()` to silently ignore "column already exists" on existing databases.

### Storage format

The `favicon` column stores **only the filename** (e.g., `github.com.png`), never a full path.  
The full path is assembled at runtime: `{exe_dir}/Data/favicons/{filename}`.

This ensures portable directories move without broken paths — consistent with how the project handles all other data.

Affected Rust structs: `TreeNode`, `Bookmark`, `SearchResult` — each gains `favicon: Option<String>`.  
`allNodes` in JS gains the `favicon` field transparently via `get_tree`.

---

## 2. File Cache

**Location:** `Data/favicons/` next to the exe (created on first write).

**Filename:** `{sanitized_domain}{.ext}` where:
- Domain is extracted from URL, `www.` prefix stripped.
- Domain is sanitized to `[a-z0-9.-]`, other chars replaced with `_`.
- Extension is the real type of the fetched file: `.ico`, `.png`, `.svg`, `.gif`.

**Examples:**
```
github.com.ico
stackoverflow.com.png
fonts.googleapis.com.png
```

**Cache hit logic (domain-first):** Before any HTTP request, Rust scans `Data/favicons/` for any file matching `{domain}.*`. If found, that file is reused — the DB row is updated with the existing filename and the function returns immediately without a network call.

This means: **the extension is determined once at first fetch and never changes** unless the file is manually deleted.

---

## 3. Rust Command: `fetch_favicon`

```rust
#[tauri::command]
fn fetch_favicon(state: tauri::State<AppState>, id: i64, url: String) -> Result<Option<String>, String>
```

### Algorithm

1. Extract domain from `url` (strip `www.`). If URL is unparseable → return `Ok(None)`.
2. Scan `Data/favicons/{domain}.*` — if any file exists: `UPDATE nodes SET favicon=filename WHERE id=?`, return `Ok(Some(filename))` (cache hit).
3. HTTP fetch with **8-second timeout**, tried in order:
   - `GET https://{domain}/favicon.ico`
   - If that fails or returns non-image: `GET https://{domain}/` → parse `<head>` for first `<link rel="icon|shortcut icon|apple-touch-icon">` → `GET` that URL
4. On success: detect content-type to determine extension, save to `Data/favicons/{domain}.{ext}`, `UPDATE nodes SET favicon=filename WHERE id=?`, return `Ok(Some(filename))`.
5. On all failures: return `Ok(None)` — no DB update. JS shows fallback `●`.

### Notes

- Uses existing `reqwest` client (same as `check_url`).
- `Data/favicons/` directory created automatically if absent.
- No retry logic — one attempt per strategy, then fallback. The queue will not block on a slow host.

---

## 4. JS Queue

### Constant

```js
const MAX_FAVICON_CONCURRENCY = 5;
```

Defined at the top of `app.js` as a named constant. Do not inline this value.

### Domain deduplication — intentional rate limiting

Before building the queue, JS groups bookmarks by domain.  
**Only one bookmark per domain enters the queue.** After its favicon resolves, JS updates the DOM for all other bookmarks sharing that domain from `allNodes` using the returned filename.

This is **intentional per-domain rate limiting** — not an optimization to be refactored away. It must be preserved in any future changes to queue logic.

```js
// Dedup logic (pseudocode):
const seen = new Set();
for (const node of bookmarks) {
  const domain = extractDomain(node.url);
  if (!domain || seen.has(domain)) {
    node._faviconSameAs = domain; // will be updated from cache after primary resolves
    continue;
  }
  seen.add(domain);
  queue.push({ id: node.id, url: node.url, domain, sameIds: [] });
}
// attach sameIds (other nodes with same domain)
```

### Queue state

```js
let _faviconQueue     = [];   // Array<{id, url, domain, sameIds: number[]}>
let _faviconActive    = 0;    // concurrent in-flight invokes
let _faviconCancelled = false;
let _faviconTotal     = 0;
let _faviconDone      = 0;
```

### Worker loop

```js
function _runFaviconWorker() {
  if (_faviconCancelled || _faviconQueue.length === 0 || _faviconActive >= MAX_FAVICON_CONCURRENCY) return;
  _faviconActive++;
  const item = _faviconQueue.shift();
  updateFaviconPanel(item.domain);
  invoke('fetch_favicon', { id: item.id, url: item.url })
    .then(filename => {
      if (filename) applyFaviconToDOM(item, filename);
    })
    .catch(() => {}) // failure = silent fallback, queue continues
    .finally(() => {
      _faviconDone++;
      _faviconActive--;
      updateFaviconPanelProgress();
      if (_faviconQueue.length === 0 && _faviconActive === 0) finishFaviconBatch();
      else _runFaviconWorker();
    });
}

// Start: fill up to MAX workers
function startFaviconWorkers() {
  for (let i = 0; i < MAX_FAVICON_CONCURRENCY; i++) _runFaviconWorker();
}
```

### DOM update after resolve

```js
function applyFaviconToDOM(item, filename) {
  const path = /* assembled by: */ `${dataDir}/favicons/${filename}`;
  // Update allNodes for the primary node
  const n = allNodes.find(n => n.id === item.id);
  if (n) n.favicon = filename;
  // Update all same-domain nodes
  for (const sid of item.sameIds) {
    const sn = allNodes.find(n => n.id === sid);
    if (sn) sn.favicon = filename;
    updateFaviconInDOM(sid, path);
  }
  updateFaviconInDOM(item.id, path);
}
```

`dataDir` is obtained once at startup via `invoke('get_data_dir')` (new trivial Rust command returning the `Data/` directory path).

---

## 5. Progress Panel

### HTML (new, bottom of layout, analogous to `#checker-panel`)

```html
<div id="favicon-panel" class="hidden">
  <div id="favicon-panel-label">favicon: <span id="fv-done">0</span>/<span id="fv-total">0</span></div>
  <div id="favicon-panel-domain"></div>
  <div id="fv-bar-wrap"><div id="fv-bar"></div></div>
  <button id="fv-cancel-btn">Отмена</button>
</div>
```

### Behavior

- Appears when batch starts, hidden when batch finishes (or 2s after completion).
- Shows: `favicon: 127/500`, current domain being fetched, progress bar fill %.
- Cancel button: sets `_faviconCancelled = true`, drains in-flight promises silently, hides panel.
- Single-favicon load (from bookmark context menu): no panel shown — silent DOM update.
- **Non-modal.** User can navigate the tree, click folders, use the app normally while it runs.

---

## 6. Context Menus

### Bookmark context menu (`showContextMenu`)

Added after "Обновить рисунок":

```js
ctxMenuEl.appendChild(
  ctxItem("favicon", "Загрузить favicon", null,
    () => { hideContextMenu(); loadSingleFavicon(node); },
    !node.url)
);
```

`loadSingleFavicon` calls `invoke('fetch_favicon', ...)` once, updates `allNodes` and DOM on resolve. No progress panel.

### Folder context menu (`showFolderContextMenu`)

Added after "Проверить":

```js
ctxMenuEl.appendChild(
  ctxItem("favicon", "Загрузить favicon'ы", null,
    () => { hideContextMenu(); startFaviconBatch(folderNode, true); })
);
```

`startFaviconBatch(folderNode, recursive=true)` collects bookmarks from `allNodes` recursively, deduplicates by domain, builds queue, shows panel, starts workers.

---

## 7. Display

Favicon is shown as a 16×16 `<img>` wherever `node.favicon` is set. Falls back to `●` on missing/error.

### CSS

```css
.favicon-icon {
  width: 16px;
  height: 16px;
  object-fit: contain;
  flex-shrink: 0;
  image-rendering: pixelated; /* keeps crisp at 16px */
}
```

On `img.onerror`: replace with the original `●` span (remove img, restore span).

### Tree (bookmark leaf — `createTreeNode`)

Replace `icon.textContent = "●"` with favicon img if `node.favicon` exists; keep `●` span as fallback.

### Grid row (bookmark — `createCard`)

Replace `dot.textContent = "●"` with favicon img if `b.favicon` exists; keep `●` as fallback.

### Detail view (`openDetailView`)

Show favicon img (16×16) **immediately before the URL text** in the URL strip (`#detail-url`).  
Layout: `[favicon] https://example.com`

---

## 8. Rust helper: `get_data_dir`

```rust
#[tauri::command]
fn get_data_dir(state: tauri::State<AppState>) -> Result<String, String>
```

Returns absolute path to `Data/` directory (same logic as existing thumb path). Called once at startup in JS, stored in `let dataDir`. Used by JS to assemble full favicon paths for `<img src>`.

---

## 9. Out of Scope (Future Work)

- **Orphaned favicon cleanup** — files in `Data/favicons/` with no matching DB rows. Not implemented in this version.
- **Force refresh / TTL** — Shift+reload to bypass cache. Not implemented (YAGNI).
- **Favicon in export** — HTML/TXT/sync export ignores `favicon` column. No change to export logic.
- **Import** — imported bookmarks have `favicon = null`. Loaded on demand.

---

## 10. Compatibility

| Area | Impact |
|---|---|
| DB migration | `ALTER TABLE … ADD COLUMN` with `.ok()` — backward safe |
| Portable move | Only filenames in DB, paths assembled at runtime — safe |
| Export (HTML/TXT/sync) | No change |
| Import (all formats) | favicon = null, loaded on demand |
| `allNodes` cache | New `favicon` field, null for old data |
| `refresh_thumb` | Unchanged — thumb and favicon are independent |
