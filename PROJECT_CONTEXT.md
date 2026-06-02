# URL-Album 2 — Project Context

## Что это

Современный порт классического URL-Album (Win32, ~2005–2010).
Оригинал: визуальный менеджер закладок с PNG-скриншотами, иерархией папок,
локальным хранением данных. IE-only, Windows-only, мёртв.

Цель: сохранить дух оригинала — локальность, минимализм, быстрый старт —
на современном стеке.

## Стек

| Слой       | Технология                          |
|------------|-------------------------------------|
| Оболочка   | Tauri 2                             |
| Бэкенд     | Rust (синхронный, минимальный)      |
| БД         | SQLite — rusqlite (bundled feature) |
| Кодировки  | encoding_rs (Windows-1251 → UTF-8)  |
| Фронтенд   | Vanilla HTML + CSS + JS (без npm)   |

## Portable-архитектура

```
url-album/
├── url-album.exe     <- основной бинарник
├── album.db          <- создаётся при первом запуске
├── ua.dat.bak        <- старые данные (для импорта, опционально)
└── Data/             <- PNG-скриншоты из оригинала
    ├── 050509105335.png
    └── ...
```

- `album.db` создаётся через `current_exe().parent()` — рядом с exe
- Вся папка переносима: скопировал → всё работает
- Никакого AppData, никакого реестра

## Структура кода

```
url-album-2/
├── src-tauri/
│   ├── Cargo.toml
│   ├── build.rs
│   ├── tauri.conf.json
│   ├── capabilities/default.json
│   └── src/
│       ├── main.rs      <- точка входа + 6 Tauri-команд
│       ├── db.rs        <- SQLite schema + CRUD
│       └── importer.rs  <- парсер ua.dat
├── ui/
│   ├── index.html
│   ├── style.css        <- тёмная тема, CSS variables
│   └── app.js           <- vanilla JS, event delegation
└── package.json         <- только скрипты dev/build
```

## Tauri-команды (IPC)

| Команда         | Аргументы        | Возврат           |
|-----------------|------------------|-------------------|
| `is_empty`      | —                | `bool`            |
| `get_tree`      | —                | `Vec<FolderNode>` |
| `get_bookmarks` | `folder_id: i64` | `Vec<Bookmark>`   |
| `find_uadat`    | —                | `Option<String>`  |
| `import_uadat`  | `path: String`   | `Result<usize>`   |
| `open_url`      | `url: String`    | `Result<()>`      |

## Схема БД

```sql
nodes (
  id       INTEGER PRIMARY KEY AUTOINCREMENT,
  parent   INTEGER,          -- NULL у корневой папки
  kind     TEXT NOT NULL,    -- 'folder' | 'bookmark'
  title    TEXT NOT NULL,
  url      TEXT,
  thumb    TEXT,             -- абсолютный путь к PNG
  note     TEXT,
  created  TEXT,             -- метка времени ddMMyyHHmmss
  visited  TEXT,
  sort_idx INTEGER DEFAULT 0
)
```

## Формат ua.dat (оригинал)

- Кодировка: Windows-1251
- Структура: tab-indented TSV
- Глубина = количество ведущих `\t`
- Колонки: `Title\tURL\tThumb.png\tNote\tCreated\tVisited\tFlag`
- URL == `#` -> папка
- Корневой узел: `title!!!` на глубине 0
- Примечания: `^^` = перенос строки
- Скриншоты: `Data/{timestamp}.png`

## UX-поток

1. Запуск -> проверка `is_empty()`
2. БД пуста -> экран импорта, `find_uadat()` ищет ua.dat рядом с exe
3. Импорт -> парсинг CP1251 -> вставка в SQLite
4. Главный вид: дерево папок слева + сетка карточек справа
5. Клик по папке -> `get_bookmarks(id)` -> рендер карточек
6. Клик по карточке -> `open_url(url)` -> открывает в браузере
7. PNG-превью: `convertFileSrc(abs_path)` -> `asset://localhost/...`
