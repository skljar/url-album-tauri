/// One record parsed from a ua.dat / ua.dat.bak file.
pub struct ParsedNode {
    pub depth: usize,
    pub is_folder: bool,
    pub title: String,
    pub url: Option<String>,
    pub thumb: Option<String>,
    pub note: Option<String>,
    pub created: Option<String>,
    pub visited: Option<String>,
}

/// Parse Windows-1251-decoded text of ua.dat into a flat list of nodes.
/// The tree structure is encoded as leading tab characters (depth = tab count).
pub fn parse(text: &str) -> Vec<ParsedNode> {
    let mut result = Vec::new();

    for line in text.lines() {
        // Count leading tabs to determine depth
        let depth = line.bytes().take_while(|&b| b == b'\t').count();
        let content = &line[depth..];
        if content.trim().is_empty() {
            continue;
        }

        // Columns are tab-separated:
        // [0] title  [1] url  [2] thumb  [3] note  [4] created  [5] visited  [6] flag
        let mut cols = content.splitn(7, '\t');
        let title_raw = cols.next().unwrap_or("").trim();
        let url_raw   = cols.next().unwrap_or("").trim();
        let thumb_raw = cols.next().unwrap_or("").trim();
        let note_raw  = cols.next().unwrap_or("").trim();
        let created   = cols.next().unwrap_or("").trim();
        let visited   = cols.next().unwrap_or("").trim();

        if title_raw.is_empty() {
            continue;
        }

        let is_folder = url_raw == "#";

        // The root node has title ending in "!!!" — strip it
        let title = if is_folder && depth == 0 {
            title_raw.trim_end_matches('!').trim().to_string()
        } else {
            title_raw.to_string()
        };

        result.push(ParsedNode {
            depth,
            is_folder,
            title,
            url: (!is_folder && !url_raw.is_empty())
                .then(|| url_raw.to_string()),
            thumb: (!thumb_raw.is_empty())
                .then(|| thumb_raw.to_string()),
            // "^^" is a line-break escape used in comments
            note: (!note_raw.is_empty())
                .then(|| note_raw.replace("^^", "\n")),
            created: (!created.is_empty())
                .then(|| created.to_string()),
            visited: (!visited.is_empty())
                .then(|| visited.to_string()),
        });
    }

    result
}
