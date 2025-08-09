#![allow(dead_code, reason = "idk maybe we'll need it later xdd")]

mod collapse_whitespace;
mod node;
mod root_node;
mod rules;
mod utilities;

pub use self::node::{Node, NodeType};
use self::root_node::RootNode;
use self::rules::ReplacementFn;
pub use self::rules::{Filter, Rule, Rules};

use self::utilities::{trim_leading_newlines, trim_trailing_newlines};
use anyhow::bail;
use fancy_regex::Regex;
use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeadingStyle {
    Atx,
    Setext,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeBlockStyle {
    Fenced,
    Indented,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkStyle {
    Inlined,
    Referenced,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkReferenceStyle {
    Full,
    Collapsed,
    Shortcut,
}

pub struct TurndownOptions {
    pub br: &'static str,
    pub heading_style: HeadingStyle,
    pub bullet_list_marker: &'static str,
    pub code_block_style: CodeBlockStyle,
    pub fence: &'static str,
    pub hr: &'static str,
    pub link_style: LinkStyle,
    pub link_reference_style: LinkReferenceStyle,
    pub em_delimiter: &'static str,
    pub strong_delimiter: &'static str,
    pub preformatted_code: bool,
    pub blank_replacement: ReplacementFn,
    pub keep_replacement: ReplacementFn,
    pub default_replacement: ReplacementFn,
}

impl Default for TurndownOptions {
    fn default() -> Self {
        TurndownOptions {
            br: "  ",
            heading_style: HeadingStyle::Setext,
            bullet_list_marker: "*",
            code_block_style: CodeBlockStyle::Indented,
            fence: "```",
            hr: "* * *",
            link_style: LinkStyle::Inlined,
            link_reference_style: LinkReferenceStyle::Full,
            em_delimiter: "_",
            strong_delimiter: "**",
            preformatted_code: false,
            blank_replacement: Rc::new(|_content, node, _opts| {
                if node.borrow().is_block.borrow().unwrap_or(false) {
                    Cow::Borrowed("\n\n")
                } else {
                    Cow::Borrowed("")
                }
            }),
            keep_replacement: Rc::new(|_content, node, _opts| {
                let node = node.borrow();
                if node.is_block.borrow().unwrap_or(false) {
                    Cow::Owned(format!(
                        "\n\n{}\n\n",
                        node.data.borrow().as_deref().unwrap_or("")
                    ))
                } else {
                    Cow::Owned(node.data.borrow().as_deref().unwrap_or("").to_string())
                }
            }),
            default_replacement: Rc::new(|content, node, _opts| {
                let node = node.borrow();
                if node.is_block.borrow().unwrap_or(false) {
                    Cow::Owned(format!("\n\n{}\n\n", content))
                } else {
                    Cow::Borrowed(content)
                }
            }),
        }
    }
}

pub struct TurndownService {
    pub options: TurndownOptions,
    pub rules: Rules,
    custom_rules: HashMap<String, Rule>,
    escape_patterns: Vec<(Regex, String)>,
}

impl TurndownService {
    pub fn new(options: Option<TurndownOptions>) -> Self {
        let default_options = TurndownOptions::default();

        let final_options = options.unwrap_or(default_options);
        let rules = Rules::default();

        // Initialize escape patterns
        let escape_patterns = vec![
            (Regex::new(r"\\").expect("valid regex"), r"\\".to_string()),
            (Regex::new(r"\*").expect("valid regex"), r"\*".to_string()),
            (Regex::new(r"^-").expect("valid regex"), r"\-".to_string()),
            (
                Regex::new(r"^\+ ").expect("valid regex"),
                r"\+ ".to_string(),
            ),
            (
                Regex::new(r"^(=+)").expect("valid regex"),
                r"\$1".to_string(),
            ),
            (
                Regex::new(r"^(#{1,6}) ").expect("valid regex"),
                r"\$1 ".to_string(),
            ),
            (Regex::new(r"`").expect("valid regex"), r"\`".to_string()),
            (
                Regex::new(r"^~~~").expect("valid regex"),
                r"\~~~".to_string(),
            ),
            (Regex::new(r"\[").expect("valid regex"), r"\[".to_string()),
            (Regex::new(r"\]").expect("valid regex"), r"\]".to_string()),
            (Regex::new(r"^>").expect("valid regex"), r"\>".to_string()),
            (Regex::new(r"_").expect("valid regex"), r"\_".to_string()),
            (
                Regex::new(r"^(\d+)\. ").expect("valid regex"),
                r"$1\. ".to_string(),
            ),
        ];

        TurndownService {
            options: final_options,
            rules,
            custom_rules: Default::default(),
            escape_patterns,
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
        Ok(self.post_process(output))
    }

    pub fn add_rule(&mut self, key: &str, rule: Rule) -> &mut Self {
        self.custom_rules.insert(key.to_string(), rule);
        self
    }

    pub fn use_plugin<F>(&mut self, plugin: F) -> &mut Self
    where
        F: FnOnce(&mut TurndownService),
    {
        plugin(self);
        self
    }

    pub fn use_plugins<F>(&mut self, plugins: Vec<F>) -> &mut Self
    where
        F: FnOnce(&mut TurndownService),
    {
        for plugin in plugins {
            plugin(self);
        }
        self
    }

    pub fn escape(&self, string: &str) -> String {
        self.escape_patterns
            .iter()
            .fold(string.to_string(), |acc, (regex, replacement)| {
                regex.replace_all(&acc, replacement.as_str()).to_string()
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
                NodeType::Text | NodeType::CDataSection => {
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

    fn post_process(&mut self, mut output: String) -> String {
        // Handle reference links append functionality
        let reference_output = self.rules.get_references("referenceLink", false);
        if !reference_output.is_empty() {
            output = Self::join(&output, &reference_output);
        }

        // Trim leading and trailing whitespace/newlines
        let leading_regex = Regex::new(r"^[\t\r\n]+").expect("valid regex");
        let trailing_regex = Regex::new(r"[\t\r\n\s]+$").expect("valid regex");

        output = leading_regex.replace(&output, "").to_string();
        output = trailing_regex.replace(&output, "").to_string();

        output
    }

    fn replacement_for_node(&mut self, node: &Rc<RefCell<Node>>) -> anyhow::Result<Cow<'_, str>> {
        // Create a minimal root node to process this node's children
        let temp_root = RootNode {
            children: node.borrow().children.borrow().clone(),
        };
        let content = self.process(&temp_root)?;

        // Get flanking whitespace
        let whitespace = Node::flanking_whitespace(node, &self.options);

        let trimmed_content = if !whitespace.leading.is_empty() || !whitespace.trailing.is_empty() {
            content.trim()
        } else {
            &content
        };

        // Find matching rule by trying different rule names
        let node_name = node.borrow().node_name.to_lowercase();
        let mut replacement_result = match node_name.as_str() {
            "p" => {
                if let Some(rule) = self.rules.get("paragraph") {
                    (rule.replacement)(trimmed_content, node, &self.options)
                } else {
                    Cow::Owned(format!("\n\n{}\n\n", trimmed_content))
                }
            }
            "br" => {
                if let Some(rule) = self.rules.get("lineBreak") {
                    (rule.replacement)(trimmed_content, node, &self.options)
                } else {
                    Cow::Owned(format!("{}\\n", self.options.br))
                }
            }
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                if let Some(rule) = self.rules.get("heading") {
                    (rule.replacement)(trimmed_content, node, &self.options)
                } else {
                    let level = node_name
                        .chars()
                        .nth(1)
                        .unwrap_or('1')
                        .to_digit(10)
                        .unwrap_or(1) as usize;
                    let hashes = "#".repeat(level);
                    Cow::Owned(format!("\n\n{} {}\n\n", hashes, trimmed_content))
                }
            }
            "blockquote" => {
                if let Some(rule) = self.rules.get("blockquote") {
                    (rule.replacement)(trimmed_content, node, &self.options)
                } else {
                    let prefixed = trimmed_content
                        .lines()
                        .map(|line| format!("> {}", line))
                        .collect::<Vec<_>>()
                        .join("\n");
                    Cow::Owned(format!("\n\n{}\n\n", prefixed))
                }
            }
            _ => {
                // Check for other common rules by name mapping
                let rule_name = match node_name.as_str() {
                    "ul" | "ol" => "list",
                    "li" => "listItem",
                    "pre" => {
                        if self.options.code_block_style == CodeBlockStyle::Indented {
                            "indentedCodeBlock"
                        } else {
                            "fencedCodeBlock"
                        }
                    }
                    "hr" => "horizontalRule",
                    "a" => {
                        if self.options.link_style == LinkStyle::Inlined {
                            "inlineLink"
                        } else {
                            "referenceLink"
                        }
                    }
                    "em" | "i" => "emphasis",
                    "strong" | "b" => "strong",
                    "code" => "code",
                    "img" => "image",
                    _ => "",
                };

                if !rule_name.is_empty() {
                    if let Some(rule) = self.rules.get(rule_name) {
                        (rule.replacement)(trimmed_content, node, &self.options)
                    } else {
                        // Fallback default replacement
                        if node.borrow().is_block() {
                            Cow::Owned(format!("\n\n{}\n\n", trimmed_content))
                        } else {
                            Cow::Borrowed(trimmed_content)
                        }
                    }
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
            if rule.filter.matches(node, &self.options) {
                let content = replacement_result;
                replacement_result = (rule.replacement)(&content, node, &self.options)
                    .to_string()
                    .into();
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
