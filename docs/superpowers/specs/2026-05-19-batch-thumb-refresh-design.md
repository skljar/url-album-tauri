# Batch Thumbnail Refresh — Design Spec

**Date:** 2026-05-19  
**Status:** Approved

---

## Overview

Add "Обновить рисунки" to the folder context menu. Refreshes (or creates) screenshots for all direct bookmarks in the selected folder. Non-recursive. Shows a progress panel with cancel support.

---

## Menu

In `showFolderContextMenu`, add one item after "Загрузить favicon'ы":

```
Загрузить favicon'ы
Обновить рисунки       ← new
Переименовать
```

Calls `startThumbBatch(folderNode)`.

---

## Architecture

### No Rust changes

`refresh_thumb(id, url, width, height, timeout)` already exists and works per-node. No backend changes needed.

### JS: `startThumbBatch(folderNode)`

1. Collect direct bookmarks: `allNodes.filter(n => n.parent === folderNode.id && n.kind === 'bookmark' && n.url)`
2. If empty — return silently
3. Build queue: `[{id, url, title}, ...]`
4. Show `#thumb-panel` with total count
5. Launch 2 workers (`MAX_THUMB_CONCURRENCY = 2`)

### JS: `_runThumbWorker()`

Loop pattern mirrors `_runFaviconWorker()`:

```
if cancelled or queue empty or active >= 2 → return
active++
item = queue.shift()
show item.title in panel
invoke('refresh_thumb', {id, url, width, height, timeout})
  .then(path → update allNodes + DOM)
  .catch(() => {})  // silent skip on error
  .finally → active--, update progress, recurse or finish
```

**DOM update on completion:** update `allNodes[n].thumb = path`, update `card.dataset.thumb`, replace `<img>` in card's `.card-thumb` (or insert if none). Do NOT switch detail view.

### JS: Progress panel

New `#thumb-panel` element, visually identical to `#favicon-panel` but independent. Position: `bottom: 24px; left: 24px` (same corner — panels can coexist if both run). State vars:

```js
let _thumbQueue     = [];
let _thumbActive    = 0;
let _thumbCancelled = false;
let _thumbTotal     = 0;
let _thumbDone      = 0;
```

Panel shows: `"Рисунки: N / M"`, progress bar, current title, "Отмена" button.

---

## Settings

Uses existing `appSettings.thumbWidth / thumbHeight / thumbTimeout` — same values as single-refresh.

---

## Error handling

Per-item errors are silently skipped (same as favicon batch). The worker continues to next item.

---

## What is NOT in scope

- Recursive (subfolders) — intentionally excluded
- Force-clear before refresh — just overwrites existing thumbnail
- Retry logic
- Separate settings for batch concurrency
