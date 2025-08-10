mod collapse_whitespace;
mod node;
mod root_node;
mod rules;
mod utilities;

pub use self::node::{Node, NodeType};
use self::root_node::RootNode;
pub use self::rules::{Filter, Rule, Rules};

use self::utilities::{trim_leading_newlines, trim_trailing_newlines};
use anyhow::bail;
use fancy_regex::Regex;
use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::LazyLock;

static ESCAPE_PATTERNS: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
    vec![
        (Regex::new(r"\\").expect("valid regex"), r"\\"),
        (Regex::new(r"\*").expect("valid regex"), r"\*"),
        (Regex::new(r"^-").expect("valid regex"), r"\-"),
        (Regex::new(r"^\+ ").expect("valid regex"), r"\+ "),
        (Regex::new(r"^(=+)").expect("valid regex"), r"\$1"),
        (Regex::new(r"^(#{1,6}) ").expect("valid regex"), r"\$1 "),
        (Regex::new(r"`").expect("valid regex"), r"\`"),
        (Regex::new(r"^~~~").expect("valid regex"), r"\~~~"),
        (Regex::new(r"\[").expect("valid regex"), r"\["),
        (Regex::new(r"\]").expect("valid regex"), r"\]"),
        (Regex::new(r"^>").expect("valid regex"), r"\>"),
        (Regex::new(r"_").expect("valid regex"), r"\_"),
        (Regex::new(r"^(\d+)\. ").expect("valid regex"), r"$1\. "),
    ]
});

const BR_MARKER: &str = "  ";
const BULLET_LIST_MARKER: &str = "-";
const FENCE_CHAR: char = '`';
const DEFAULT_FENCE_SIZE: usize = 3;
const HR_MARKER: &str = "* * *";
const EM_DELIMITER: &str = "_";
const STRONG_DELIMITER: &str = "**";

pub struct TurndownService {
    pub rules: Rules,
    custom_rules: HashMap<String, Rule>,
}

impl TurndownService {
    pub fn new() -> Self {
        let rules = Rules::default();

        TurndownService {
            rules,
            custom_rules: Default::default(),
        }
    }

    pub fn turndown(&mut self, input: &str) -> anyhow::Result<String> {
        if !self.can_convert(input) {
            bail!(
                "{} is not a string, or an element/document/fragment node.",
                input
            );
        }

        if input.is_empty() {
            return Ok(String::new());
        }

        let root_node = RootNode::new(input);
        let output = self.process(&root_node)?;
        Ok(self.post_process(&output))
    }

    pub fn add_rule(&mut self, key: &str, rule: Rule) -> &mut Self {
        self.custom_rules.insert(key.to_string(), rule);
        self
    }

    pub fn escape(&self, string: &str) -> String {
        ESCAPE_PATTERNS
            .iter()
            .fold(string.to_string(), |acc, (regex, replacement)| {
                regex.replace_all(&acc, *replacement).to_string()
            })
    }

    fn can_convert(&self, input: &str) -> bool {
        !input.is_empty()
    }

    fn process(&mut self, parent_node: &RootNode) -> anyhow::Result<String> {
        let mut output = String::new();

        for child in &parent_node.children {
            let node_borrow = child.borrow();
            let node_data = node_borrow.data.borrow();
            let replacement = match node_borrow.node_type {
                NodeType::Text => {
                    let text_data = node_data.as_deref().unwrap_or("");

                    if Node::is_code(child) {
                        Cow::Borrowed(text_data)
                    } else {
                        Cow::Owned(self.escape(text_data))
                    }
                }
                NodeType::Element => self.replacement_for_node(child)?,
            };

            output = Self::join(&output, &replacement);
        }

        Ok(output)
    }

    fn post_process(&self, output: &str) -> String {
        output.trim().to_string()
    }

    fn replacement_for_node(&mut self, node: &Rc<RefCell<Node>>) -> anyhow::Result<Cow<'_, str>> {
        // Create a minimal root node to process this node's children
        let temp_root = RootNode {
            children: node.borrow().children.borrow().clone(),
        };
        let content = self.process(&temp_root)?;

        // Get flanking whitespace
        let whitespace = Node::flanking_whitespace(node);

        let trimmed_content = if !whitespace.leading.is_empty() || !whitespace.trailing.is_empty() {
            content.trim()
        } else {
            &content
        };

        // Find matching rule by trying different rule names
        let node_name = node.borrow().node_name.to_lowercase();
        let mut replacement_result = match node_name.as_str() {
            "p" => self.rules.get("paragraph").map_or_else(
                || Cow::Owned(format!("\n\n{}\n\n", trimmed_content)),
                |rule| (rule.replacement)(trimmed_content, node),
            ),
            "br" => self.rules.get("lineBreak").map_or_else(
                || Cow::Owned(format!("{}\\n", BR_MARKER)),
                |rule| (rule.replacement)(trimmed_content, node),
            ),
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => self.rules.get("heading").map_or_else(
                || {
                    let level = node_name
                        .chars()
                        .nth(1)
                        .unwrap_or('1')
                        .to_digit(10)
                        .unwrap_or(1) as usize;
                    let hashes = "#".repeat(level);
                    Cow::Owned(format!("\n\n{} {}\n\n", hashes, trimmed_content))
                },
                |rule| (rule.replacement)(trimmed_content, node),
            ),
            "blockquote" => self.rules.get("blockquote").map_or_else(
                || {
                    let prefixed = trimmed_content
                        .lines()
                        .map(|line| format!("> {}", line))
                        .collect::<Vec<_>>()
                        .join("\n");
                    Cow::Owned(format!("\n\n{}\n\n", prefixed))
                },
                |rule| (rule.replacement)(trimmed_content, node),
            ),
            _ => {
                // Check for other common rules by name mapping
                let rule_name = match node_name.as_str() {
                    "ul" | "ol" => "list",
                    "li" => "listItem",
                    "pre" => "fencedCodeBlock",
                    "hr" => "horizontalRule",
                    "a" => "inlineLink",
                    "em" | "i" => "emphasis",
                    "strong" | "b" => "strong",
                    "code" => "code",
                    "img" => "image",
                    _ => "",
                };

                if !rule_name.is_empty() {
                    self.rules.get(rule_name).map_or_else(
                        || {
                            if node.borrow().is_block() {
                                Cow::Owned(format!("\n\n{}\n\n", trimmed_content))
                            } else {
                                Cow::Borrowed(trimmed_content)
                            }
                        },
                        |rule| (rule.replacement)(trimmed_content, node),
                    )
                } else {
                    // Default replacement
                    if node.borrow().is_block() {
                        Cow::Owned(format!("\n\n{}\n\n", trimmed_content))
                    } else {
                        Cow::Borrowed(trimmed_content)
                    }
                }
            }
        };

        for rule in self.custom_rules.values() {
            if rule.filter.matches(node) {
                let content = replacement_result;
                replacement_result = (rule.replacement)(&content, node).to_string().into();
            }
        }

        Ok(Cow::Owned(format!(
            "{}{}{}",
            whitespace.leading, replacement_result, whitespace.trailing
        )))
    }

    fn join(output: &str, replacement: &str) -> String {
        let s1 = trim_trailing_newlines(output);
        let s2 = trim_leading_newlines(replacement);
        let nls = std::cmp::max(output.len() - s1.len(), replacement.len() - s2.len());
        let separator = "\n\n".chars().take(nls).collect::<String>();

        format!("{}{}{}", s1, separator, s2)
    }
}
