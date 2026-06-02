# Batch Thumbnail Refresh Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add "Обновить рисунки" to the folder context menu — refreshes screenshots for all direct bookmarks in the folder with a progress panel and cancel support.

**Architecture:** A new `#thumb-panel` HTML element mirrors `#favicon-panel`. JS-side: state vars + `startThumbBatch()` + `_runThumbWorker()` process the queue with 2 parallel workers, updating `allNodes` and grid DOM on each completion. No Rust changes needed — `refresh_thumb` already exists.

**Tech Stack:** Vanilla JS, HTML, CSS, Tauri `invoke('refresh_thumb')`

---

### Task 1: Add `#thumb-panel` HTML element

**Files:**
- Modify: `ui/index.html` — add panel after `#favicon-panel` (line ~509)

- [ ] **Step 1: Open `ui/index.html` and locate the favicon-panel block**

Find (around line 493–509):
```html
  <!-- Favicon loader panel (non-modal background task) -->
  <div id="favicon-panel" class="hidden">
    ...
  </div>
```

- [ ] **Step 2: Add `#thumb-panel` immediately after the closing `</div>` of `#favicon-panel`**

```html
  <!-- Thumb batch panel (non-modal background task) -->
  <div id="thumb-panel" class="hidden">
    <div id="tp-titlebar">
      <span id="tp-title">Загрузка рисунков</span>
      <button id="tp-close-btn" title="Закрыть">×</button>
    </div>
    <div id="tp-body">
      <div id="tp-info-row">
        <span>рисунки: <b id="tp-done">0</b>/<b id="tp-total">0</b></span>
        <span id="tp-label"></span>
      </div>
      <div id="tp-bar-track"><div id="tp-bar-fill"></div></div>
      <div id="tp-btn-row">
        <button class="win-btn" id="tp-cancel-btn">Отмена</button>
      </div>
    </div>
  </div>
```

- [ ] **Step 3: Commit**

```bash
git add ui/index.html
git commit -m "feat(thumb-batch): add #thumb-panel HTML"
```

---

### Task 2: Style `#thumb-panel` in CSS

**Files:**
- Modify: `ui/style.css` — add thumb-panel styles after `#fv-btn-row` block (line ~1451)

- [ ] **Step 1: Open `ui/style.css` and locate the end of the favicon-panel block**

Find (around line 1451):
```css
#fv-btn-row { display: flex; justify-content: flex-end; }
```

- [ ] **Step 2: Add thumb-panel styles immediately after**

```css
#thumb-panel {
  position: fixed;
  bottom: 24px;
  left: 24px;
  width: 360px;
  background: #f0f0f0;
  border: 1px solid #767676;
  box-shadow: 3px 3px 10px rgba(0,0,0,0.32);
  z-index: 501;
  font-family: "Segoe UI", system-ui, sans-serif;
  font-size: 12px;
  color: #000;
}

#tp-titlebar {
  background: #0078d4;
  color: #fff;
  padding: 4px 6px 4px 10px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  user-select: none;
}
#tp-title { font-size: 12px; }
#tp-close-btn {
  background: none;
  border: none;
  color: #fff;
  font-size: 14px;
  line-height: 1;
  padding: 0 6px 1px;
  cursor: pointer;
}
#tp-close-btn:hover { background: #e81123; }

#tp-body { padding: 8px 10px 8px; display: flex; flex-direction: column; gap: 5px; }

#tp-info-row {
  display: flex;
  justify-content: space-between;
  align-items: baseline;
  gap: 8px;
  font-size: 12px;
}
#tp-label {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: #555;
  font-size: 11px;
  text-align: right;
}

#tp-bar-track {
  height: 14px;
  background: #ddd;
  border: 1px inset #aaa;
  overflow: hidden;
}
#tp-bar-fill {
  height: 100%;
  background: #0078d4;
  width: 0%;
  transition: width 0.25s;
}
#tp-btn-row { display: flex; justify-content: flex-end; }
```

- [ ] **Step 3: Commit**

```bash
git add ui/style.css
git commit -m "feat(thumb-batch): add #thumb-panel CSS styles"
```

---

### Task 3: Add thumb batch state vars and panel helpers in JS

**Files:**
- Modify: `ui/app.js` — add after favicon queue state block (line ~2924)

- [ ] **Step 1: Open `ui/app.js` and locate the favicon queue state block**

Find (around line 2918–2924):
```js
// ── Favicon queue state ───────────────────────────────────────────────────
let _faviconQueue     = [];
let _faviconActive    = 0;
let _faviconCancelled = false;
let _faviconTotal     = 0;
let _faviconDone      = 0;
```

- [ ] **Step 2: Add thumb batch state vars immediately after (after the blank line on ~2925)**

```js
// ── Thumb batch state ─────────────────────────────────────────────────────
const MAX_THUMB_CONCURRENCY = 2;
let _thumbQueue     = [];   // Array<{id, url, title}>
let _thumbActive    = 0;
let _thumbCancelled = false;
let _thumbTotal     = 0;
let _thumbDone      = 0;
```

- [ ] **Step 3: Find `showFaviconPanel` function (around line 393) and add thumb panel helpers directly after `hideFaviconPanel` (around line 409)**

Find the block:
```js
function hideFaviconPanel() {
  document.getElementById('favicon-panel').classList.add('hidden');
}
```

Add immediately after:
```js
function showThumbPanel(total) {
  _thumbTotal = total;
  _thumbDone  = 0;
  document.getElementById('tp-done').textContent  = '0';
  document.getElementById('tp-total').textContent = total;
  document.getElementById('tp-bar-fill').style.width = '0%';
  document.getElementById('tp-label').textContent   = '';
  document.getElementById('thumb-panel').classList.remove('hidden');
}

function _updateThumbPanelProgress() {
  document.getElementById('tp-done').textContent = _thumbDone;
  const pct = _thumbTotal > 0 ? Math.round(_thumbDone / _thumbTotal * 100) : 0;
  document.getElementById('tp-bar-fill').style.width = pct + '%';
}

function hideThumbPanel() {
  document.getElementById('thumb-panel').classList.add('hidden');
}

function _finishThumbBatch() {
  document.getElementById('tp-label').textContent = 'Готово';
  setTimeout(hideThumbPanel, 2000);
}

document.getElementById('tp-cancel-btn').addEventListener('click', () => {
  _thumbCancelled = true;
  _thumbQueue = [];
  hideThumbPanel();
});

document.getElementById('tp-close-btn').addEventListener('click', () => {
  _thumbCancelled = true;
  _thumbQueue = [];
  hideThumbPanel();
});
```

- [ ] **Step 4: Commit**

```bash
git add ui/app.js
git commit -m "feat(thumb-batch): add thumb batch state vars and panel helpers"
```

---

### Task 4: Implement `_runThumbWorker` and `startThumbBatch`

**Files:**
- Modify: `ui/app.js` — add after `startFaviconBatch` function (around line 4276)

- [ ] **Step 1: Find `startFaviconBatch` in `ui/app.js` (around line 4260) and locate the blank line after its closing brace (~line 4276)**

The block ends with:
```js
  showFaviconPanel(_faviconQueue.length);
  startFaviconWorkers();
}
```

- [ ] **Step 2: Add `_runThumbWorker` and `startThumbBatch` immediately after**

```js
function _runThumbWorker() {
  if (_thumbCancelled || _thumbQueue.length === 0 || _thumbActive >= MAX_THUMB_CONCURRENCY) return;
  _thumbActive++;
  const item = _thumbQueue.shift();

  const labelEl = document.getElementById('tp-label');
  if (labelEl) labelEl.textContent = item.title || item.url;

  invoke('refresh_thumb', {
    id:      item.id,
    url:     item.url,
    width:   appSettings.thumbWidth   || 1280,
    height:  appSettings.thumbHeight  || 800,
    timeout: appSettings.thumbTimeout || 30,
  })
    .then(newPath => {
      if (!newPath) return;
      const n = allNodes.find(n => n.id === item.id);
      if (n) n.thumb = newPath;
      // Update grid card if visible
      const card = gridEl.querySelector(`.card[data-id="${item.id}"]`);
      if (card) {
        card.dataset.thumb = newPath;
        const thumbDiv = card.querySelector('.card-thumb');
        if (thumbDiv) {
          thumbDiv.innerHTML = '';
          const img = document.createElement('img');
          img.src = convertFileSrc(newPath);
          img.onerror = () => { img.remove(); thumbDiv.appendChild(makeNoImg(item.title)); };
          thumbDiv.appendChild(img);
        }
      }
    })
    .catch(() => {})
    .finally(() => {
      _thumbDone++;
      _thumbActive--;
      _updateThumbPanelProgress();
      if (_thumbQueue.length === 0 && _thumbActive === 0) {
        _finishThumbBatch();
      } else {
        _runThumbWorker();
      }
    });
}

function startThumbBatch(folderNode) {
  _thumbCancelled = false;
  _thumbQueue     = [];
  _thumbActive    = 0;

  const bookmarks = allNodes.filter(
    n => n.parent === folderNode.id && n.kind === 'bookmark' && n.url
  );
  if (bookmarks.length === 0) return;

  _thumbQueue = bookmarks.map(n => ({ id: n.id, url: n.url, title: n.title }));
  showThumbPanel(_thumbQueue.length);
  for (let i = 0; i < MAX_THUMB_CONCURRENCY; i++) _runThumbWorker();
}
```

- [ ] **Step 3: Commit**

```bash
git add ui/app.js
git commit -m "feat(thumb-batch): implement _runThumbWorker and startThumbBatch"
```

---

### Task 5: Add menu item to folder context menu

**Files:**
- Modify: `ui/app.js` — `showFolderContextMenu` function (around line 919)

- [ ] **Step 1: Find the favicon menu item in `showFolderContextMenu` (around line 919)**

Find:
```js
  ctxMenuEl.appendChild(ctxItem("favicon", "Загрузить favicon'ы", null, () => {
    hideContextMenu();
    startFaviconBatch(folderNode, true);
  }));
  ctxMenuEl.appendChild(ctxItem("edit", "Переименовать", "F2", () => {
```

- [ ] **Step 2: Insert the new menu item between them**

```js
  ctxMenuEl.appendChild(ctxItem("favicon", "Загрузить favicon'ы", null, () => {
    hideContextMenu();
    startFaviconBatch(folderNode, true);
  }));
  ctxMenuEl.appendChild(ctxItem("refresh", "Обновить рисунки", null, () => {
    hideContextMenu();
    startThumbBatch(folderNode);
  }));
  ctxMenuEl.appendChild(ctxItem("edit", "Переименовать", "F2", () => {
```

- [ ] **Step 3: Commit**

```bash
git add ui/app.js
git commit -m "feat(thumb-batch): add 'Обновить рисунки' to folder context menu"
```

---

### Task 6: Build and verify

**Files:** none

- [ ] **Step 1: Kill running process and rebuild**

```powershell
Stop-Process -Name "url-album" -Force -ErrorAction SilentlyContinue
cd C:\Projects\url-album-2\src-tauri
cargo build
```

Expected: `Finished \`dev\` profile`

- [ ] **Step 2: Launch**

```powershell
Start-Process ".\target\debug\url-album.exe" -WorkingDirectory ".\target\debug"
```

- [ ] **Step 3: Manual test — right-click a folder with bookmarks**

- Right-click a folder that has at least 2 direct bookmarks with URLs
- Verify "Обновить рисунки" appears in the context menu between "Загрузить favicon'ы" and "Переименовать"
- Click it
- Verify `#thumb-panel` appears in the bottom-left corner with counter "рисунки: 0/N"
- Verify progress bar advances as screenshots complete
- Verify grid cards update with new thumbnails after each completes
- Verify "Готово" appears and panel auto-hides after 2 seconds

- [ ] **Step 4: Manual test — cancel**

- Right-click a folder with many bookmarks, click "Обновить рисунки"
- While panel is visible, click "Отмена"
- Verify panel hides immediately and no further screenshots are taken

- [ ] **Step 5: Manual test — empty folder**

- Right-click a folder with no bookmarks (or only subfolders)
- Click "Обновить рисунки"
- Verify nothing happens (no panel, no error)

- [ ] **Step 6: Final commit if any fixups were needed, then done**
