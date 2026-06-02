# Menu Restructure Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Restructure all menus per "НАД ЧЕМ" principle: File=DB lifecycle, Links=bookmark ops, Transfer=import/export, Folder Context=folder ops. No duplication.

**Architecture:** New Rust commands (close_db, get_recent_dbs, get_db_properties, import with parent_id) → JS MENU_DATA rewrite (File/Links/Transfer) → new DB Properties dialog → folder context menu rewrite → dynamic Recent Databases submenu.

**Tech Stack:** Rust (Tauri 2, rusqlite), Vanilla JS, HTML/CSS

---

## Files Modified

| File | Changes |
|------|---------|
| `src-tauri/src/main.rs` | +close_db, +get_recent_dbs, +get_db_properties, +save_recent_db helper; modify import commands to accept parent_id; register new commands |
| `ui/app.js` | Rewrite MENU_DATA; update handleMenuAction; rewrite showFolderContextMenu; add Recent Databases logic; add refresh-favicons/thumbs-folder actions; add manage-browsers action |
| `ui/index.html` | Add #db-props-overlay dialog HTML |
| `ui/style.css` | Add #db-props-overlay styles (reuse existing .dlg-overlay pattern) |

---

## Task 1: Rust — `close_db`, `get_db_properties`, `get_recent_dbs`

**Files:**
- Modify: `src-tauri/src/main.rs`

### What to add

- [ ] **Step 1: Add `DbProperties` struct and `get_db_properties` command**

Find the `#[derive(serde::Serialize)]` pattern (e.g. `UrlCheckResult` around line 120). Add after it:

```rust
#[derive(serde::Serialize)]
struct DbProperties {
    path: String,
    size_bytes: u64,
    folder_count: i64,
    bookmark_count: i64,
}

#[tauri::command]
fn get_db_properties(state: tauri::State<'_, AppState>) -> Result<DbProperties, String> {
    let path = state.db_path.lock().map_err(|e| e.to_string())?.clone();
    let size_bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let folder_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM nodes WHERE kind='folder'", [], |r| r.get(0)
    ).unwrap_or(0);
    let bookmark_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM nodes WHERE kind='bookmark'", [], |r| r.get(0)
    ).unwrap_or(0);
    Ok(DbProperties { path: path.to_string_lossy().into_owned(), size_bytes, folder_count, bookmark_count })
}
```

- [ ] **Step 2: Add `save_recent_db` helper and `get_recent_dbs` command**

Add `save_recent_db` as a plain `fn` (not a command) near other file helpers. Add `get_recent_dbs` as a command:

```rust
fn get_exe_dir() -> std::path::PathBuf {
    std::env::current_exe().ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_default()
}

fn save_recent_db(path: &std::path::Path) {
    let recent_path = get_exe_dir().join("recent_dbs.txt");
    let path_str = path.to_string_lossy().into_owned();
    let existing = std::fs::read_to_string(&recent_path).unwrap_or_default();
    let mut entries: Vec<String> = std::iter::once(path_str.clone())
        .chain(existing.lines().filter(|l| !l.trim().is_empty() && *l != path_str).map(String::from))
        .take(10)
        .collect();
    let _ = std::fs::write(&recent_path, entries.join("\n"));
}

#[tauri::command]
fn get_recent_dbs() -> Vec<String> {
    let path = get_exe_dir().join("recent_dbs.txt");
    std::fs::read_to_string(&path)
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.trim().is_empty() && std::path::Path::new(l).exists())
        .map(String::from)
        .collect()
}
```

**Note:** Check if `get_exe_dir()` already exists by searching main.rs for `current_exe`. If a similar helper exists, use it instead of adding a new one.

- [ ] **Step 3: Add `close_db` command**

```rust
#[tauri::command]
fn close_db(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)").ok();
    Ok(())
}
```

- [ ] **Step 4: Call `save_recent_db` from `open_db`, `create_new_db`, `switch_db`**

In each of those three functions, after the DB path is set, add:
```rust
save_recent_db(&new_path); // pass the actual path variable used in that function
```

Search for these functions and add the call where the path becomes known.

- [ ] **Step 5: Register all new commands in the Tauri builder**

Find `.invoke_handler(tauri::generate_handler![` in main.rs and add:
```
close_db,
get_recent_dbs,
get_db_properties,
```

- [ ] **Step 6: Build and check**

```powershell
cd C:\Projects\url-album-2\src-tauri
cargo build 2>&1 | Select-Object -Last 5
```

Expected: `Finished 'dev' profile`

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/main.rs
git commit -m "feat(db): add close_db, get_recent_dbs, get_db_properties commands"
```

---

## Task 2: Rust — Import commands with optional `parent_id`

**Files:**
- Modify: `src-tauri/src/main.rs`

Import commands currently create a new folder at root. Adding `parent_id: Option<i64>` lets the folder context menu import into an existing folder.

Commands to modify: `import_html`, `import_txt_lines`, `import_uadat_pick`, `import_from_browser`.

- [ ] **Step 1: Add `parent_id` param to `import_html`**

Find `async fn import_html(state: tauri::State<'_, AppState>, window: tauri::Window)`.

Change signature to:
```rust
async fn import_html(state: tauri::State<'_, AppState>, window: tauri::Window, parent_id: Option<i64>) -> Result<usize, String>
```

Inside the function, find where it creates the import folder (something like `conn.execute("INSERT INTO nodes ... kind='folder'")`) and use `parent_id.unwrap_or(root_id)` as the parent for imported items. The exact change depends on how the importer is written — look for `parent` field being set on new nodes, and replace the hardcoded root with `parent_id.unwrap_or(root_parent)`.

- [ ] **Step 2: Same for `import_txt_lines`**

Add `parent_id: Option<i64>` parameter. Apply same logic as Step 1.

- [ ] **Step 3: Same for `import_uadat_pick`**

Add `parent_id: Option<i64>` parameter.

- [ ] **Step 4: Same for `import_from_browser`**

Find `async fn import_from_browser`. Add `parent_id: Option<i64>` parameter.

- [ ] **Step 5: Build and verify**

```powershell
cargo build 2>&1 | Select-Object -Last 5
```

Expected: `Finished 'dev' profile`

If import logic is complex or the parent_id threading is non-trivial, read `src-tauri/src/importer.rs` and `src-tauri/src/db.rs` for context on how nodes are inserted.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/main.rs
git commit -m "feat(import): add optional parent_id to import commands"
```

---

## Task 3: JS — Rewrite MENU_DATA

**Files:**
- Modify: `ui/app.js` — MENU_DATA starting ~line 1987

Replace the entire `MENU_DATA` array. The current MENU_DATA has: `file`, `links`, `search`, `view`. New structure: `file`, `links`, `transfer`, `search`, `view`.

- [ ] **Step 1: Find and replace MENU_DATA**

Find the block:
```js
const MENU_DATA = [
  {
    id: 'file', label: 'Файл',
```

Replace the entire MENU_DATA array with:

```js
const MENU_DATA = [
  {
    id: 'file', label: 'Файл',
    items: [
      { label: 'Создать базу',   icon: 'db',     action: 'new-db'         },
      { label: 'Открыть базу',   icon: 'db',     action: 'open-db'        },
      { label: 'Последние базы', icon: 'db',     action: 'recent-dbs'     },
      { label: 'Закрыть базу',   icon: 'db',     action: 'close-db'       },
      '---',
      { label: 'Резервная копия', icon: 'backup', sub: [
        { label: 'Создать без рисунков',         icon: 'backup', action: 'backup-without'  },
        { label: 'Создать с рисунками',          icon: 'backup', action: 'backup-with'     },
        '---',
        { label: 'Восстановить резервную копию', icon: 'open',   action: 'backup-restore'  },
      ]},
      '---',
      { label: 'Свойства базы',  icon: 'props',  action: 'db-properties'  },
      '---',
      { label: 'Настройки',      icon: 'gear',   action: 'settings'       },
      { label: 'Выход',          icon: 'quit',   shortcut: 'Alt+F4', action: 'quit' },
    ]
  },
  {
    id: 'links', label: 'Ссылки',
    items: [
      { label: 'Новая ссылка',                    icon: 'new',      action: 'new-link'                },
      { label: 'Редактировать',                   icon: 'edit',     shortcut: 'F2',  action: 'properties' },
      { label: 'Удалить ссылку',                  icon: 'trash',    shortcut: 'Del', action: 'delete-link' },
      '---',
      { label: 'Открыть',                         icon: 'open',     shortcut: 'Enter', action: 'open-link' },
      { label: 'Открыть в переносном браузере',   icon: 'openwith', action: 'open-portable'           },
      { label: 'Открыть с помощью...',            icon: 'openwith', action: 'open-with'               },
      '---',
      { label: 'Проверить ссылки',      icon: 'verify',   action: 'check-all-links'         },
      { label: 'Найти дубликаты',       icon: 'dupes',    action: 'find-dupes'              },
      '---',
      { label: "Обновить favicon'ы",    icon: 'favicon',  action: 'refresh-favicons-folder' },
      { label: 'Обновить рисунки',      icon: 'refresh',  action: 'refresh-thumbs-folder'  },
      '---',
      { label: 'Копировать URL',        icon: 'copy',     shortcut: 'Ctrl+C', action: 'copy-url' },
      { label: 'Свойства',              icon: 'props',    shortcut: 'F4',  action: 'properties'  },
    ]
  },
  {
    id: 'transfer', label: 'Перенос',
    items: [
      { label: 'Импорт', icon: 'import', sub: [
        { label: 'Из браузера...',          icon: 'browser', action: 'import-from-browser' },
        '---',
        { label: 'Из файла HTML',           icon: 'import',  action: 'import-html'         },
        { label: 'Из файла TXT',            icon: 'import',  action: 'import-txt-lines'    },
        '---',
        { label: 'Файл синхронизации',      icon: 'backup',  action: 'import-sync'         },
        { label: 'Из ua.dat...',            icon: 'folder',  action: 'import-folder'        },
      ]},
      { label: 'Экспорт', icon: 'backup', sub: [
        { label: 'HTML файл',                   icon: 'import',  action: 'export-html'          },
        { label: 'Текстовый файл',              icon: 'import',  action: 'export-txt'           },
        '---',
        { label: 'Синхронизация с рисунками',   icon: 'backup',  action: 'export-sync-with'    },
        { label: 'Синхронизация без рисунков',  icon: 'backup',  action: 'export-sync-without' },
      ]},
      '---',
      { label: 'Браузеры', icon: 'browser', action: 'manage-browsers' },
    ]
  },
  {
    id: 'search', label: 'Поиск',
    items: [
      { label: 'Найти', icon: 'find', shortcut: 'Ctrl+F', action: 'find' },
    ]
  },
  {
    id: 'view', label: 'Вид',
    items: [
      { label: 'Развернуть/Свернуть все папки', icon: 'expand',   action: 'toggle-expand-all'  },
      { label: 'Настроить toolbar',             icon: 'settings', action: 'customize-toolbar'   },
    ]
  },
];
```

- [ ] **Step 2: Commit**

```bash
git add ui/app.js
git commit -m "feat(menu): rewrite MENU_DATA — File/Links/Transfer/Search/View"
```

---

## Task 4: JS — Update `handleMenuAction` for new actions

**Files:**
- Modify: `ui/app.js` — `handleMenuAction` function

Find `function handleMenuAction(action)` (search for `case 'new-db'` to locate it).

- [ ] **Step 1: Add handlers for new actions**

Inside the switch/if-else block of `handleMenuAction`, add cases for:

```js
case 'close-db':
  invoke('close_db').catch(console.error);
  showImportScreen();
  break;

case 'db-properties':
  openDbPropertiesDialog();
  break;

case 'manage-browsers':
  openBrowserManagerDialog();
  break;

case 'refresh-favicons-folder': {
  const folder = allNodes.find(n => n.id === activeFolderId);
  if (folder) startFaviconBatch(folder, false);
  break;
}

case 'refresh-thumbs-folder': {
  const folder = allNodes.find(n => n.id === activeFolderId);
  if (folder) startThumbBatch(folder);
  break;
}
```

- [ ] **Step 2: Remove dead handlers for actions no longer in MENU_DATA**

Search `handleMenuAction` for these cases and remove them (they were in the old Ссылки menu):
- Cases that only existed due to duplication: `backup-without`, `backup-with`, `backup-restore` may still appear in File menu so keep them. But remove any import/export cases that were added specifically for the old Ссылки menu sub-items if they duplicate what Transfer now handles. Check: the action strings themselves (`import-html`, `export-html`, etc.) are the same, so no change needed — same action, new location.

**Important:** `openBrowserManagerDialog` — verify this function exists in app.js by searching for it. If it doesn't exist under that exact name, search for where the browser manager is opened (grep for 'браузер' or 'browser-manager') and use the correct function name.

- [ ] **Step 3: Commit**

```bash
git add ui/app.js
git commit -m "feat(menu): wire close-db, db-properties, manage-browsers, refresh-folder actions"
```

---

## Task 5: JS + HTML — DB Properties Dialog

**Files:**
- Modify: `ui/index.html` — add dialog HTML
- Modify: `ui/app.js` — add `openDbPropertiesDialog()`
- Modify: `ui/style.css` — add styles if needed (reuse existing patterns)

The dialog shows: path, file size, folder count, bookmark count. Has a "Закрыть базу" danger button and an "OK" button.

- [ ] **Step 1: Add dialog HTML to `ui/index.html`**

Find `<!-- Context menu (built entirely by JS) -->` near the end. Add before it:

```html
<!-- DB Properties dialog -->
<div id="dbprops-overlay" class="dlg-overlay hidden">
  <div id="dbprops-dlg" class="props-dlg">
    <div class="props-drag-handle dlg-title">Свойства базы</div>
    <div class="props-body">
      <table class="props-table">
        <tr><td class="props-label">Путь:</td>      <td><span id="dbp-path" class="props-value-path"></span></td></tr>
        <tr><td class="props-label">Размер:</td>    <td><span id="dbp-size"></span></td></tr>
        <tr><td class="props-label">Папок:</td>     <td><span id="dbp-folders"></span></td></tr>
        <tr><td class="props-label">Ссылок:</td>    <td><span id="dbp-bookmarks"></span></td></tr>
      </table>
    </div>
    <div class="props-footer">
      <button class="win-btn win-btn-danger" id="dbp-clear-btn">Очистить базу</button>
      <button class="win-btn" id="dbp-ok-btn">OK</button>
    </div>
  </div>
</div>
```

- [ ] **Step 2: Add CSS for `#dbprops-overlay` and `#dbp-path`**

In `ui/style.css`, at the end, add:

```css
#dbp-path {
  font-size: 11px;
  color: #555;
  word-break: break-all;
}
.win-btn-danger {
  color: #c00;
  border-color: #c00;
  margin-right: auto;
}
.win-btn-danger:hover { background: #c00; color: #fff; }
```

(The `.props-dlg`, `.props-body`, `.props-label`, `.props-footer` classes already exist — reuse them.)

- [ ] **Step 3: Add `openDbPropertiesDialog` to `ui/app.js`**

Find where other dialog openers are (e.g. `openFolderPropsDialog`). Add:

```js
function openDbPropertiesDialog() {
  invoke('get_db_properties')
    .then(props => {
      document.getElementById('dbp-path').textContent      = props.path;
      document.getElementById('dbp-size').textContent      = (props.size_bytes / 1024).toFixed(1) + ' KB';
      document.getElementById('dbp-folders').textContent   = props.folder_count;
      document.getElementById('dbp-bookmarks').textContent = props.bookmark_count;
      raiseOverlay(document.getElementById('dbprops-overlay'));
      document.getElementById('dbprops-overlay').classList.remove('hidden');
    })
    .catch(console.error);
}

document.getElementById('dbp-ok-btn').onclick = () => {
  document.getElementById('dbprops-overlay').classList.add('hidden');
};

document.getElementById('dbp-clear-btn').onclick = () => {
  document.getElementById('dbprops-overlay').classList.add('hidden');
  handleMenuAction('clear-db');
};

makeDlgDraggable(
  document.getElementById('dbprops-dlg'),
  document.querySelector('#dbprops-dlg .props-drag-handle')
);
```

- [ ] **Step 4: Commit**

```bash
git add ui/app.js ui/index.html ui/style.css
git commit -m "feat(menu): add DB Properties dialog"
```

---

## Task 6: JS — Rewrite `showFolderContextMenu`

**Files:**
- Modify: `ui/app.js` — `showFolderContextMenu` function (~line 899)

New folder context menu structure:
```
Новая папка
Переименовать    F2
Удалить          Del
---
Импорт в папку ▶  (browser, html, txt, ua.dat)
Экспорт папки ▶   (html, txt, sync with, sync without)
---
Проверить ссылки
Обновить favicon'ы
Обновить рисунки
---
Сортировка ▶
---
Свойства         F4
```

- [ ] **Step 1: Rewrite `showFolderContextMenu`**

Find the full `function showFolderContextMenu(e, folderNode) { ... }` block and replace it:

```js
function showFolderContextMenu(e, folderNode) {
  e.preventDefault();
  e.stopPropagation();
  closeSubFloat();
  ctxMenuEl.innerHTML = "";

  // ── Folder management ──────────────────────────────────────────────────
  ctxMenuEl.appendChild(ctxItem("new", "Новая папка", null, () => {
    hideContextMenu();
    doNewSubfolderIn(folderNode.id);
  }));
  ctxMenuEl.appendChild(ctxItem("edit", "Переименовать", "F2", () => {
    hideContextMenu();
    startInlineRename(folderNode.id);
  }));
  ctxMenuEl.appendChild(ctxItem("trash", "Удалить", "Del", () => {
    hideContextMenu();
    deleteFolder(folderNode);
  }));
  ctxMenuEl.appendChild(ctxSep());

  // ── Transfer ───────────────────────────────────────────────────────────
  const impEl = addSubTrigger("Импорт в папку", "import");
  wireMainContextFloat(impEl, () => buildFolderImportSubmenu(folderNode));
  ctxMenuEl.appendChild(impEl);

  const expEl = addSubTrigger("Экспорт папки", "backup");
  wireMainContextFloat(expEl, () => buildExportSubmenu(folderNode));
  ctxMenuEl.appendChild(expEl);
  ctxMenuEl.appendChild(ctxSep());

  // ── Operations ─────────────────────────────────────────────────────────
  ctxMenuEl.appendChild(ctxItem("verify", "Проверить ссылки", null, () => {
    hideContextMenu();
    openCheckerPanel(folderNode);
  }));
  ctxMenuEl.appendChild(ctxItem("favicon", "Обновить favicon'ы", null, () => {
    hideContextMenu();
    startFaviconBatch(folderNode, true);
  }));
  ctxMenuEl.appendChild(ctxItem("refresh", "Обновить рисунки", null, () => {
    hideContextMenu();
    startThumbBatch(folderNode);
  }));
  ctxMenuEl.appendChild(ctxSep());

  // ── Sort ───────────────────────────────────────────────────────────────
  const sortEl = addSubTrigger("Сортировка", "sort");
  wireMainContextFloat(sortEl, () => buildSortSubmenu(folderNode));
  ctxMenuEl.appendChild(sortEl);
  ctxMenuEl.appendChild(ctxSep());

  // ── Properties ─────────────────────────────────────────────────────────
  ctxMenuEl.appendChild(ctxItem("props", "Свойства", "F4", () => {
    hideContextMenu();
    openFolderPropsDialog(folderNode);
  }));

  ctxMenuEl.querySelectorAll(".ctx-item:not(.ctx-has-sub)").forEach(it => {
    it.addEventListener("mouseenter", () => { clearTimeout(_subTimer); closeSubFloat(); });
  });

  ctxMenuEl.classList.remove("hidden");
  const mw = ctxMenuEl.offsetWidth, mh = ctxMenuEl.offsetHeight;
  ctxMenuEl.style.left = Math.min(e.clientX, window.innerWidth  - mw - 4) + "px";
  ctxMenuEl.style.top  = Math.min(e.clientY, window.innerHeight - mh - 4) + "px";
}
```

- [ ] **Step 2: Add `doNewSubfolderIn(parentId)` if it doesn't exist**

Search app.js for `doNewSubfolder`. If the existing function uses `activeFolderId` as parent, add a variant:

```js
function doNewSubfolderIn(parentId) {
  // Same as doNewFolder but uses explicit parentId
  const name = prompt("Название папки:");
  if (!name?.trim()) return;
  invoke('create_folder', { parentId, title: name.trim() })
    .then(() => reloadTree())
    .catch(console.error);
}
```

If a proper inline folder creation dialog exists, use that pattern instead — check how `doNewFolder()` works and mirror it.

- [ ] **Step 3: Add `buildFolderImportSubmenu(folderNode)` function**

Find `buildExportSubmenu` function. Add a new function after it:

```js
function buildFolderImportSubmenu(folderNode) {
  const items = document.createElement("div");
  items.className = "ctx-sub-float";

  const add = (icon, label, action) => {
    const el = ctxItem(icon, label, null, () => {
      hideContextMenu();
      invokeFolderImport(action, folderNode.id);
    });
    items.appendChild(el);
  };

  add("browser", "Из браузера...",    "import-from-browser");
  items.appendChild(ctxSep());
  add("import",  "Из файла HTML",     "import-html");
  add("import",  "Из файла TXT",      "import-txt-lines");
  items.appendChild(ctxSep());
  add("backup",  "Файл синхронизации","import-sync");
  add("folder",  "Из ua.dat...",      "import-folder");

  return items;
}

async function invokeFolderImport(action, parentId) {
  let result;
  try {
    switch (action) {
      case 'import-from-browser':
        result = await openBrowserImportDialogIntoFolder(parentId);
        break;
      case 'import-html':
        result = await invoke('import_html', { parentId });
        break;
      case 'import-txt-lines':
        result = await invoke('import_txt_lines', { parentId });
        break;
      case 'import-sync':
        result = await invoke('import_sync', { parentId });
        break;
      case 'import-folder':
        result = await invoke('import_uadat_pick', { parentId });
        break;
    }
    if (result != null) {
      allNodes = await invoke('get_tree');
      allFolders = allNodes.filter(n => n.kind === 'folder');
      renderTree();
      selectFolder(parentId);
    }
  } catch(e) {
    if (e !== 'Отменено') console.error('import into folder:', e);
  }
}
```

**Note on `openBrowserImportDialogIntoFolder`:** The existing `openBrowserImportDialog()` does not accept a target folder. For now, call `openBrowserImportDialog()` without the folder parameter — the imported items will go to a new subfolder. Add `parentId` support as a follow-up task. The key architectural change is the context menu is correct; the deep import-into-folder for browser imports is a separate enhancement.

- [ ] **Step 4: Commit**

```bash
git add ui/app.js
git commit -m "feat(menu): rewrite folder context menu per new spec"
```

---

## Task 7: JS — Recent Databases dynamic submenu

**Files:**
- Modify: `ui/app.js`

The menubar builder (lines ~2537-2637) builds sub-menus from `item.sub` array statically at DOM creation time. For Recent Databases we need to populate dynamically when the File menu is opened.

**Approach:** Keep `sub: []` in MENU_DATA for the recent-dbs item. In `buildMenubar`, for the File menu group specifically, hook the `lbl.addEventListener('click', ...)` to async-populate the sub element before showing.

- [ ] **Step 1: Change recent-dbs entry in MENU_DATA to have `sub: []`**

In Task 3's MENU_DATA, the entry already has `action: 'recent-dbs'`. Change it to also have an empty sub array so the menubar builder creates a sub-menu container for it:

```js
{ label: 'Последние базы', icon: 'db', sub: [], _dynamic: true },
```

Remove the `action` property — the sub will contain the clickable items.

- [ ] **Step 2: Add `_recentDbsSubEl` variable and populate logic in `buildMenubar`**

In `buildMenubar`, the loop already creates sub elements for `hasSub` items. After building the File menu's dropdown, find the sub element for the recent-dbs entry and store it. Then hook the File menu click to populate it:

Find the section in `buildMenubar` where `menu.id === 'file'` would be processed — specifically, after the `for (const item of menu.items)` loop for the file menu. You'll need to add logic inside that loop to detect the `_dynamic: true` item.

Inside the `buildMenubar` for-loop where items are built, add special handling:

```js
// Inside the per-item loop, after `const hasSub = Array.isArray(item.sub);`:
if (item._dynamic) {
  // This is the Recent Databases item — store a ref to its subEl for later population
  // subEl is built below in the hasSub branch; we'll populate it on menu open
  entry.dataset.dynamicRecent = '1';
}
```

Then, in the `lbl.addEventListener('click', ...)` for the file group, add async population:

```js
lbl.addEventListener('click', (e) => {
  e.stopPropagation();
  const isOpen = group.classList.contains('open');
  closeAllMenus();
  if (!isOpen) {
    group.classList.add('open');
    if (menu.id === 'view') _syncExpandToggleUI();
    // Populate Recent Databases on every open
    if (menu.id === 'file') {
      const recentEntry = drop.querySelector('[data-dynamic-recent]');
      if (recentEntry) {
        const subEl = recentEntry.querySelector('.menu-sub');
        if (subEl) {
          subEl.innerHTML = '';
          invoke('get_recent_dbs').then(paths => {
            if (paths.length === 0) {
              const empty = document.createElement('div');
              empty.className = 'menu-entry disabled';
              empty.innerHTML = '<span class="entry-icon"></span><span class="entry-label">(пусто)</span>';
              subEl.appendChild(empty);
              return;
            }
            for (const p of paths) {
              const name = p.split(/[\\/]/).pop();
              const el = document.createElement('div');
              el.className = 'menu-entry';
              el.innerHTML = `<span class="entry-icon">${ICONS['db'] || ''}</span><span class="entry-label" title="${p}">${name}</span>`;
              el.addEventListener('click', (ev) => {
                ev.stopPropagation();
                closeAllMenus();
                invoke('switch_db', { newPath: p })
                  .then(() => showApp())
                  .catch(console.error);
              });
              subEl.appendChild(el);
            }
          }).catch(() => {});
        }
      }
    }
  }
});
```

Also add the same population logic to `lbl.addEventListener('mouseenter', ...)` so it works when hovering between menus.

- [ ] **Step 3: Add `'open-portable'` action to `handleMenuAction`**

The Links menu has "Открыть в переносном браузере" with action `open-portable`. Add handler:

```js
case 'open-portable': {
  // Open in first configured portable browser; fall back to open-with if none
  const url = activeBookmarkNode?.url;
  if (!url) break;
  invoke('load_browsers_config').then(config => {
    const browsers = config?.browsers || [];
    if (browsers.length > 0) {
      invoke('open_url_with', { url: normalize_url_js(url), browser: browsers[0].path }).catch(console.error);
    } else {
      handleMenuAction('open-with');
    }
  }).catch(() => handleMenuAction('open-with'));
  break;
}
```

Where `normalize_url_js(url)` is the JS equivalent — check if `normalizeUrl(url)` or similar exists in app.js (search for `https://` prepending logic). If not, use: `url.startsWith('http') ? url : 'https://' + url`.

- [ ] **Step 4: Commit**

```bash
git add ui/app.js
git commit -m "feat(menu): dynamic Recent Databases submenu + open-portable action"
```

---

## Task 8: Build, verify, cleanup

**Files:** all modified files

- [ ] **Step 1: Kill process and rebuild**

```powershell
Stop-Process -Name "url-album" -Force -ErrorAction SilentlyContinue
cd C:\Projects\url-album-2\src-tauri
cargo build 2>&1 | Select-Object -Last 5
```

Expected: `Finished 'dev' profile`

- [ ] **Step 2: Launch**

```powershell
Start-Process ".\target\debug\url-album.exe" -WorkingDirectory ".\target\debug"
```

- [ ] **Step 3: Verify File menu**

- Open File menu → see: Создать базу / Открыть базу / Последние базы ▶ / Закрыть базу / --- / Резервная копия ▶ / --- / Свойства базы / --- / Настройки / Выход
- Click "Свойства базы" → dialog shows path, size, folder count, bookmark count
- Click "Закрыть базу" → welcome screen appears
- Re-open a database → it appears in Последние базы submenu

- [ ] **Step 4: Verify Links menu**

- Open Links menu → see new structure (Новая ссылка at top, Редактировать, no Import/Export/Backup)
- With bookmark selected: Открыть, Открыть с помощью..., Свойства all work
- "Обновить favicon'ы" and "Обновить рисунки" fire for active folder

- [ ] **Step 5: Verify Transfer menu**

- Open Transfer → Import ▶ and Export ▶ submenus work
- All existing import/export actions function

- [ ] **Step 6: Verify Folder Context Menu**

- Right-click folder → new structure: Новая папка / Переименовать / Удалить / --- / Импорт в папку ▶ / Экспорт папки ▶ / --- / Проверить / Обновить favicons / Обновить рисунки / --- / Сортировка ▶ / --- / Свойства
- Импорт в папку ▶ opens submenu with import options
- Экспорт папки ▶ opens export submenu

- [ ] **Step 7: Remove any dead code**

Search app.js for any remaining references to old actions that no longer exist in MENU_DATA (e.g. old sort-all-* actions that were in old Ссылки menu). Remove unused cases from handleMenuAction.

Search for duplicate `case 'backup-without':` etc. in handleMenuAction — keep one, remove duplicates.

- [ ] **Step 8: Final commit**

```bash
git add ui/app.js ui/index.html ui/style.css src-tauri/src/main.rs
git commit -m "feat(menu): complete menu restructure — File/Links/Transfer/FolderContext"
```
