pub fn repeat(character: char, count: usize) -> String {
    character.to_string().repeat(count)
}

pub fn trim_leading_newlines(string: &str) -> &str {
    string.trim_start_matches('\n')
}

pub fn trim_trailing_newlines(string: &str) -> &str {
    string.trim_end_matches('\n')
}

pub const BLOCK_ELEMENTS: &[&str] = &[
    "ADDRESS",
    "ARTICLE",
    "ASIDE",
    "AUDIO",
    "BLOCKQUOTE",
    "BODY",
    "CANVAS",
    "CENTER",
    "DD",
    "DIR",
    "DIV",
    "DL",
    "DT",
    "FIELDSET",
    "FIGCAPTION",
    "FIGURE",
    "FOOTER",
    "FORM",
    "FRAMESET",
    "H1",
    "H2",
    "H3",
    "H4",
    "H5",
    "H6",
    "HEADER",
    "HGROUP",
    "HR",
    "HTML",
    "ISINDEX",
    "LI",
    "MAIN",
    "MENU",
    "NAV",
    "NOFRAMES",
    "NOSCRIPT",
    "OL",
    "OUTPUT",
    "P",
    "PRE",
    "SECTION",
    "TABLE",
    "TBODY",
    "TD",
    "TFOOT",
    "TH",
    "THEAD",
    "TR",
    "UL",
];

pub fn is_block(node_name: &str) -> bool {
    is_element(node_name, BLOCK_ELEMENTS)
}

pub const VOID_ELEMENTS: &[&str] = &[
    "AREA", "BASE", "BR", "COL", "COMMAND", "EMBED", "HR", "IMG", "INPUT", "KEYGEN", "LINK",
    "META", "PARAM", "SOURCE", "TRACK", "WBR",
];

pub fn is_void(node_name: &str) -> bool {
    is_element(node_name, VOID_ELEMENTS)
}

pub fn has_void(element_names: &[&str]) -> bool {
    has_element(element_names, VOID_ELEMENTS)
}

const MEANINGFUL_WHEN_BLANK_ELEMENTS: &[&str] = &[
    "A", "TABLE", "THEAD", "TBODY", "TFOOT", "TH", "TD", "IFRAME", "SCRIPT", "AUDIO", "VIDEO",
];

pub fn is_meaningful_when_blank(node_name: &str) -> bool {
    is_element(node_name, MEANINGFUL_WHEN_BLANK_ELEMENTS)
}

pub fn has_meaningful_when_blank(element_names: &[&str]) -> bool {
    has_element(element_names, MEANINGFUL_WHEN_BLANK_ELEMENTS)
}

fn is_element(node_name: &str, tag_names: &[&str]) -> bool {
    tag_names.contains(&node_name)
}

fn has_element(element_names: &[&str], tag_names: &[&str]) -> bool {
    tag_names
        .iter()
        .any(|tag_name| element_names.contains(tag_name))
}
