#[cfg(test)]
mod tests;

use super::node::Node;
use super::utilities::repeat;
use super::{CodeBlockStyle, HeadingStyle, LinkReferenceStyle, LinkStyle, TurndownOptions};
use fancy_regex::Regex;
use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

pub type FilterFn = Rc<dyn Fn(&Rc<RefCell<Node>>, &TurndownOptions) -> bool>;
pub type ReplacementFn =
    Rc<dyn for<'a> Fn(&'a str, &Rc<RefCell<Node>>, &TurndownOptions) -> Cow<'a, str>>;

pub enum Filter {
    TagName(&'static str),
    TagNames(Vec<&'static str>),
    Function(FilterFn),
}

impl Filter {
    pub fn matches(&self, node: &Rc<RefCell<Node>>, opts: &TurndownOptions) -> bool {
        match self {
            Filter::TagName(name) => node.borrow().node_name.eq_ignore_ascii_case(name),
            Filter::TagNames(items) => items
                .iter()
                .any(|name| node.borrow().node_name.eq_ignore_ascii_case(name)),
            Filter::Function(func) => func(node, opts),
        }
    }
}

pub type AppendFn = Rc<dyn Fn(&TurndownOptions) -> String>;

pub struct Rule {
    pub filter: Filter,
    pub replacement: ReplacementFn,
    pub references: RefCell<Vec<String>>,
    pub append: Option<AppendFn>,
    pub reference_counter: RefCell<usize>,
}

impl Rule {
    pub fn new(filter: Filter, replacement: ReplacementFn) -> Self {
        Rule {
            filter,
            replacement,
            references: RefCell::new(Vec::new()),
            append: None,
            reference_counter: RefCell::new(0),
        }
    }

    pub fn with_append(filter: Filter, replacement: ReplacementFn, append: AppendFn) -> Self {
        Rule {
            filter,
            replacement,
            references: RefCell::new(Vec::new()),
            append: Some(append),
            reference_counter: RefCell::new(0),
        }
    }

    pub fn add_reference(&self, reference: String) {
        self.references.borrow_mut().push(reference);
    }

    pub fn next_reference_id(&self) -> usize {
        let mut counter = self.reference_counter.borrow_mut();
        *counter += 1;
        *counter
    }

    pub fn clear_references(&self) {
        self.references.borrow_mut().clear();
        *self.reference_counter.borrow_mut() = 0;
    }
}

pub struct Rules {
    rules: HashMap<String, Rule>,
}

impl Rules {
    fn new() -> Self {
        let mut rules = HashMap::new();

        // Paragraph rule
        rules.insert(
            "paragraph".to_string(),
            Rule::new(
                Filter::TagName("p"),
                Rc::new(
                    |content: &str, _node: &Rc<RefCell<Node>>, _options: &TurndownOptions| {
                        Cow::Owned(format!("\n\n{}\n\n", content))
                    },
                ),
            ),
        );

        // Line break rule
        rules.insert(
            "lineBreak".to_string(),
            Rule::new(
                Filter::TagName("br"),
                Rc::new(
                    |_content: &str, _node: &Rc<RefCell<Node>>, options: &TurndownOptions| {
                        Cow::Owned(format!("{}\n", options.br))
                    },
                ),
            ),
        );

        // Heading rule
        rules.insert(
            "heading".to_string(),
            Rule::new(
                Filter::TagNames(vec!["h1", "h2", "h3", "h4", "h5", "h6"]),
                Rc::new(
                    |content: &str, node: &Rc<RefCell<Node>>, options: &TurndownOptions| {
                        let node_name = &node.borrow().node_name;
                        let h_level = node_name
                            .chars()
                            .nth(1)
                            .expect("tag name has 2 chars")
                            .to_digit(10)
                            .expect("2nd char is numeric")
                            as usize;

                        if options.heading_style == HeadingStyle::Setext && h_level < 3 {
                            let underline_char = if h_level == 1 { '=' } else { '-' };
                            let underline = repeat(underline_char, content.len());
                            Cow::Owned(format!("\n\n{}\n{}\n\n", content, underline))
                        } else {
                            let hashes = repeat('#', h_level);
                            Cow::Owned(format!("\n\n{} {}\n\n", hashes, content))
                        }
                    },
                ),
            ),
        );

        // Blockquote rule
        rules.insert(
            "blockquote".to_string(),
            Rule::new(
                Filter::TagName("blockquote"),
                Rc::new(
                    |content: &str, _node: &Rc<RefCell<Node>>, _options: &TurndownOptions| {
                        let trimmed = content.trim_start_matches('\n').trim_end_matches('\n');
                        let prefixed = trimmed
                            .lines()
                            .map(|line| format!("> {}", line))
                            .collect::<Vec<_>>()
                            .join("\n");
                        Cow::Owned(format!("\n\n{}\n\n", prefixed))
                    },
                ),
            ),
        );

        // List rule
        rules.insert(
            "list".to_string(),
            Rule::new(
                Filter::TagNames(vec!["ul", "ol"]),
                Rc::new(
                    |content: &str, node: &Rc<RefCell<Node>>, _options: &TurndownOptions| {
                        let parent = node.borrow().parent.borrow().upgrade();
                        parent.map_or_else(
                            || Cow::Owned(format!("\n\n{}\n\n", content)),
                            |parent_node| {
                                let parent_name = &parent_node.borrow().node_name;
                                if parent_name == "LI"
                                    && Node::is_last_element_child_of_parent(node)
                                {
                                    Cow::Owned(format!("\n{}", content))
                                } else {
                                    Cow::Owned(format!("\n\n{}\n\n", content))
                                }
                            },
                        )
                    },
                ),
            ),
        );

        // List item rule
        rules.insert(
            "listItem".to_string(),
            Rule::new(
                Filter::TagName("li"),
                Rc::new(
                    |content: &str, node: &Rc<RefCell<Node>>, options: &TurndownOptions| {
                        let processed_content = content
                            .trim_start_matches('\n')
                            .trim_end_matches('\n')
                            .lines()
                            .map(|line| format!("    {}", line))
                            .collect::<Vec<_>>()
                            .join("\n");

                        let mut prefix = format!("{}   ", options.bullet_list_marker);

                        if let Some(parent) = node.borrow().parent.borrow().upgrade()
                            && parent.borrow().node_name == "OL"
                        {
                            // Get the start attribute from the parent, defaulting to 1
                            let start = parent
                                .borrow()
                                .get_attribute("start")
                                .and_then(|s| s.parse::<usize>().ok())
                                .unwrap_or(1);

                            // Calculate the index of this item within its parent's children
                            let mut index = 0;
                            let parent_children = parent.borrow().children.borrow().clone();
                            for (i, child) in parent_children.iter().enumerate() {
                                if Rc::ptr_eq(child, node) {
                                    index = i;
                                    break;
                                }
                            }

                            let number = start + index;
                            prefix = format!("{}.  ", number);
                        }

                        let has_next_sibling = node.borrow().next_sibling.borrow().is_some();
                        let trailing_newline =
                            if has_next_sibling && !processed_content.ends_with('\n') {
                                "\n"
                            } else {
                                ""
                            };

                        Cow::Owned(format!(
                            "{}{}{}",
                            prefix, processed_content, trailing_newline
                        ))
                    },
                ),
            ),
        );

        // Indented code block rule
        rules.insert(
            "indentedCodeBlock".to_string(),
            Rule::new(
                Filter::Function(Rc::new(
                    |node: &Rc<RefCell<Node>>, options: &TurndownOptions| {
                        let node_borrow = node.borrow();
                        options.code_block_style == CodeBlockStyle::Indented
                            && node_borrow.node_name == "PRE"
                            && node_borrow.first_child.borrow().is_some()
                            && node_borrow
                                .first_child
                                .borrow()
                                .as_ref()
                                .expect("is some")
                                .borrow()
                                .node_name
                                == "CODE"
                    },
                )),
                Rc::new(
                    |_content: &str, node: &Rc<RefCell<Node>>, _options: &TurndownOptions| {
                        let first_child = node
                            .borrow()
                            .first_child
                            .borrow()
                            .clone()
                            .expect("has child");
                        let text_content = first_child
                            .borrow()
                            .data
                            .borrow()
                            .as_ref()
                            .unwrap_or(&String::new())
                            .clone();
                        let indented = text_content
                            .lines()
                            .map(|line| format!("    {}", line))
                            .collect::<Vec<_>>()
                            .join("\n");
                        Cow::Owned(format!("\n\n{}\n\n", indented))
                    },
                ),
            ),
        );

        // Fenced code block rule
        rules.insert(
            "fencedCodeBlock".to_string(),
            Rule::new(
                Filter::Function(Rc::new(
                    |node: &Rc<RefCell<Node>>, options: &TurndownOptions| {
                        let node_borrow = node.borrow();
                        options.code_block_style == CodeBlockStyle::Fenced
                            && node_borrow.node_name == "PRE"
                            && node_borrow.first_child.borrow().is_some()
                            && node_borrow
                                .first_child
                                .borrow()
                                .as_ref()
                                .expect("is some")
                                .borrow()
                                .node_name
                                == "CODE"
                    },
                )),
                Rc::new(
                    |_content: &str, node: &Rc<RefCell<Node>>, options: &TurndownOptions| {
                        let first_child = node
                            .borrow()
                            .first_child
                            .borrow()
                            .clone()
                            .expect("has child");
                        let first_child_borrow = first_child.borrow();

                        // Get language from class attribute
                        let language = first_child_borrow
                            .get_attribute("class")
                            .map(|class_attr| extract_language_from_class(&class_attr))
                            .unwrap_or_default();

                        let code = first_child_borrow
                            .data
                            .borrow()
                            .as_ref()
                            .unwrap_or(&String::new())
                            .clone();

                        let fence_char = options.fence.chars().next().expect("is not empty");
                        let mut fence_size = 3;

                        // Find the longest sequence of fence characters in the code
                        let fence_regex = Regex::new(&format!(r"^{}{{{}}}", fence_char, "{3,}"))
                            .expect("valid regex");
                        for line in code.lines() {
                            if let Ok(Some(mat)) = fence_regex.find(line) {
                                let len = mat.end() - mat.start();
                                if len >= fence_size {
                                    fence_size = len + 1;
                                }
                            }
                        }

                        let fence = repeat(fence_char, fence_size);
                        let trimmed_code = code.trim_end_matches('\n');

                        Cow::Owned(format!(
                            "\n\n{}{}\n{}\n{}\n\n",
                            fence, language, trimmed_code, fence
                        ))
                    },
                ),
            ),
        );

        // Horizontal rule
        rules.insert(
            "horizontalRule".to_string(),
            Rule::new(
                Filter::TagName("hr"),
                Rc::new(
                    |_content: &str, _node: &Rc<RefCell<Node>>, options: &TurndownOptions| {
                        Cow::Owned(format!("\n\n{}\n\n", options.hr))
                    },
                ),
            ),
        );

        // Inline link rule
        rules.insert(
            "inlineLink".to_string(),
            Rule::new(
                Filter::Function(Rc::new(
                    |node: &Rc<RefCell<Node>>, options: &TurndownOptions| {
                        let node_borrow = node.borrow();
                        options.link_style == LinkStyle::Inlined
                            && node_borrow.node_name == "A"
                            && node_borrow.get_attribute("href").is_some()
                    },
                )),
                Rc::new(
                    |content: &str, node: &Rc<RefCell<Node>>, _options: &TurndownOptions| {
                        let node_borrow = node.borrow();
                        let mut href = node_borrow.get_attribute("href").unwrap_or_default();

                        // Escape parentheses in href
                        href = href.replace('(', "\\(").replace(')', "\\)");

                        let title = node_borrow
                            .get_attribute("title")
                            .map(|t| clean_attribute(Some(&t)))
                            .filter(|t| !t.is_empty())
                            .map(|t| format!(" \"{}\"", t.replace('"', "\\\"")))
                            .unwrap_or_default();

                        Cow::Owned(format!("[{}]({}{})", content, href, title))
                    },
                ),
            ),
        );

        // Reference link rule with append function
        rules.insert(
            "referenceLink".to_string(),
            Rule::with_append(
                Filter::Function(Rc::new(
                    |node: &Rc<RefCell<Node>>, options: &TurndownOptions| {
                        let node_borrow = node.borrow();
                        options.link_style == LinkStyle::Referenced
                            && node_borrow.node_name == "A"
                            && node_borrow.get_attribute("href").is_some()
                    },
                )),
                Rc::new(
                    |content: &str, _node: &Rc<RefCell<Node>>, _options: &TurndownOptions| {
                        // The actual logic is handled in Rules::apply_rule and Rules::handle_reference_link
                        // This is just a placeholder that should not be called directly
                        Cow::Owned(format!("[{}]", content))
                    },
                ),
                Rc::new(|_options: &TurndownOptions| {
                    // The actual append logic is handled in Rules::get_references
                    String::new()
                }),
            ),
        );

        // Emphasis rule
        rules.insert(
            "emphasis".to_string(),
            Rule::new(
                Filter::TagNames(vec!["em", "i"]),
                Rc::new(
                    |content: &str, _node: &Rc<RefCell<Node>>, options: &TurndownOptions| {
                        if content.trim().is_empty() {
                            Cow::Borrowed("")
                        } else {
                            Cow::Owned(format!(
                                "{}{}{}",
                                options.em_delimiter, content, options.em_delimiter
                            ))
                        }
                    },
                ),
            ),
        );

        // Strong rule
        rules.insert(
            "strong".to_string(),
            Rule::new(
                Filter::TagNames(vec!["strong", "b"]),
                Rc::new(
                    |content: &str, _node: &Rc<RefCell<Node>>, options: &TurndownOptions| {
                        if content.trim().is_empty() {
                            Cow::Borrowed("")
                        } else {
                            Cow::Owned(format!(
                                "{}{}{}",
                                options.strong_delimiter, content, options.strong_delimiter
                            ))
                        }
                    },
                ),
            ),
        );

        // Code rule
        rules.insert(
            "code".to_string(),
            Rule::new(
                Filter::Function(Rc::new(
                    |node: &Rc<RefCell<Node>>, _options: &TurndownOptions| {
                        let node_borrow = node.borrow();
                        if node_borrow.node_name != "CODE" {
                            return false;
                        }

                        let has_siblings = node_borrow.next_sibling.borrow().is_some();
                        let parent = node_borrow.parent.borrow().upgrade();
                        let is_code_block =
                            parent.is_some_and(|p| p.borrow().node_name == "PRE" && !has_siblings);

                        !is_code_block
                    },
                )),
                Rc::new(
                    |content: &str, _node: &Rc<RefCell<Node>>, _options: &TurndownOptions| {
                        if content.is_empty() {
                            return Cow::Borrowed("");
                        }

                        let content = content.replace('\n', " ").replace('\r', "");

                        let extra_space = if content.starts_with('`')
                            || content.ends_with('`')
                            || (content.starts_with(' ')
                                && content.ends_with(' ')
                                && content.trim() != content)
                        {
                            " "
                        } else {
                            ""
                        };

                        let mut delimiter = "`".to_string();
                        let backtick_regex = Regex::new(r"`+").expect("valid regex");
                        for mat in backtick_regex.find_iter(&content).flatten() {
                            if mat.as_str() == delimiter {
                                delimiter.push('`');
                            }
                        }

                        Cow::Owned(format!(
                            "{}{}{}{}{}",
                            delimiter, extra_space, content, extra_space, delimiter
                        ))
                    },
                ),
            ),
        );

        // Image rule
        rules.insert(
            "image".to_string(),
            Rule::new(
                Filter::TagName("img"),
                Rc::new(
                    |_content: &str, node: &Rc<RefCell<Node>>, _options: &TurndownOptions| {
                        let node_borrow = node.borrow();
                        let alt = node_borrow
                            .get_attribute("alt")
                            .map(|a| clean_attribute(Some(&a)))
                            .unwrap_or_default();
                        let src = node_borrow.get_attribute("src").unwrap_or_default();
                        let title = node_borrow
                            .get_attribute("title")
                            .map(|t| clean_attribute(Some(&t)))
                            .filter(|t| !t.is_empty())
                            .map(|t| format!(" \"{}\"", t))
                            .unwrap_or_default();

                        if src.is_empty() {
                            Cow::Borrowed("")
                        } else {
                            Cow::Owned(format!("![{}]({}{})", alt, src, title))
                        }
                    },
                ),
            ),
        );

        Rules { rules }
    }

    pub fn get(&self, name: &str) -> Option<&Rule> {
        self.rules.get(name)
    }

    pub fn get_mut(&mut self, name: &str) -> Option<&mut Rule> {
        self.rules.get_mut(name)
    }

    pub fn insert(&mut self, name: String, rule: Rule) {
        self.rules.insert(name, rule);
    }

    pub fn keep(&mut self, filter: Filter, options: &TurndownOptions) {
        self.rules.insert(
            format!("keep_{}", self.rules.len()),
            Rule::new(filter, Rc::clone(&options.keep_replacement)),
        );
    }

    pub fn remove(&mut self, filter: Filter) {
        self.rules.insert(
            format!("remove_{}", self.rules.len()),
            Rule::new(filter, Rc::new(|_, _, _| Cow::Borrowed(""))),
        );
    }

    /// Apply a rule and manage references if it's a reference-aware rule
    pub fn apply_rule<'a>(
        &mut self,
        rule_name: &str,
        content: &'a str,
        node: &Rc<RefCell<Node>>,
        options: &TurndownOptions,
    ) -> Cow<'a, str> {
        if let Some(rule) = self.rules.get_mut(rule_name) {
            let result = (rule.replacement)(content, node, options);

            // Special handling for reference link rule
            if rule_name == "referenceLink" {
                self.handle_reference_link(content, node, options)
            } else {
                result
            }
        } else {
            Cow::Borrowed(content)
        }
    }

    fn handle_reference_link<'a>(
        &mut self,
        content: &'a str,
        node: &Rc<RefCell<Node>>,
        options: &TurndownOptions,
    ) -> Cow<'a, str> {
        let node_borrow = node.borrow();
        let href = node_borrow.get_attribute("href").unwrap_or_default();
        let title = node_borrow
            .get_attribute("title")
            .map(|t| clean_attribute(Some(&t)))
            .filter(|t| !t.is_empty())
            .map(|t| format!(" \"{}\"", t))
            .unwrap_or_default();

        self.rules.get_mut("referenceLink").map_or_else(
            || Cow::Borrowed(content),
            |rule| {
                let (replacement, reference) = match options.link_reference_style {
                    LinkReferenceStyle::Collapsed => {
                        let replacement = format!("[{}][]", content);
                        let reference = format!("[{}]: {}{}", content, href, title);
                        (replacement, reference)
                    }
                    LinkReferenceStyle::Shortcut => {
                        let replacement = format!("[{}]", content);
                        let reference = format!("[{}]: {}{}", content, href, title);
                        (replacement, reference)
                    }
                    LinkReferenceStyle::Full => {
                        let id = rule.next_reference_id();
                        let replacement = format!("[{}][{}]", content, id);
                        let reference = format!("[{}]: {}{}", id, href, title);
                        (replacement, reference)
                    }
                };

                rule.add_reference(reference);
                Cow::Owned(replacement)
            },
        )
    }

    /// Get all accumulated references and optionally clear them
    pub fn get_references(&mut self, rule_name: &str, clear: bool) -> String {
        if let Some(rule) = self.rules.get_mut(rule_name) {
            let refs = rule.references.borrow().clone();
            let result = if refs.is_empty() {
                String::new()
            } else {
                format!("\n\n{}\n\n", refs.join("\n"))
            };

            if clear {
                rule.clear_references();
            }

            result
        } else {
            String::new()
        }
    }
}

impl Default for Rules {
    fn default() -> Self {
        Self::new()
    }
}

fn clean_attribute(attribute: Option<&str>) -> String {
    attribute
        .map(|attr| {
            let newline_regex = Regex::new(r"(\n+\s*)+").expect("valid regex");
            newline_regex.replace_all(attr, "\n").to_string()
        })
        .unwrap_or_default()
}

pub fn extract_language_from_class(class_attr: &str) -> String {
    let language_regex = Regex::new(r"language-(\S+)").expect("valid regex");
    language_regex
        .captures(class_attr)
        .ok()
        .flatten()
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str().to_string())
        .unwrap_or_default()
}
