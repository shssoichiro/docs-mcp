#[cfg(test)]
mod tests;

use super::utilities::is_block;
use fancy_regex::Regex;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::{Rc, Weak};
use std::sync::LazyLock;

#[derive(Debug, Clone)]
pub enum NodeType {
    Element = 1,
    Text = 3,
}

#[derive(Debug)]
pub struct Node {
    pub node_type: NodeType,
    pub node_name: String,
    pub data: RefCell<Option<String>>,
    pub attributes: RefCell<HashMap<String, String>>,
    pub parent: RefCell<Weak<RefCell<Node>>>,
    pub first_child: RefCell<Option<Rc<RefCell<Node>>>>,
    pub next_sibling: RefCell<Option<Rc<RefCell<Node>>>>,
    pub previous_sibling: RefCell<Option<Rc<RefCell<Node>>>>,
    pub children: RefCell<Vec<Rc<RefCell<Node>>>>,
    // Cache computed values
    pub is_block: RefCell<Option<bool>>,
    pub is_code: RefCell<Option<bool>>,
}

#[derive(Debug, Clone)]
pub struct FlankingWhitespace {
    pub leading: String,
    pub trailing: String,
}

#[derive(Debug, Clone)]
pub struct EdgeWhitespace {
    pub leading: String,
    pub leading_ascii: String,
    pub leading_non_ascii: String,
    pub trailing: String,
    pub trailing_non_ascii: String,
    pub trailing_ascii: String,
}

impl Node {
    pub fn new(node_type: NodeType, node_name: String, data: Option<String>) -> Rc<RefCell<Self>> {
        Rc::new(RefCell::new(Node {
            node_type,
            node_name,
            data: RefCell::new(data),
            attributes: RefCell::new(HashMap::new()),
            parent: RefCell::new(Weak::new()),
            first_child: RefCell::new(None),
            next_sibling: RefCell::new(None),
            previous_sibling: RefCell::new(None),
            children: RefCell::new(Vec::new()),
            is_block: RefCell::new(None),
            is_code: RefCell::new(None),
        }))
    }

    pub fn with_attributes(
        node_type: NodeType,
        node_name: String,
        data: Option<String>,
        attributes: HashMap<String, String>,
    ) -> Rc<RefCell<Self>> {
        Rc::new(RefCell::new(Node {
            node_type,
            node_name,
            data: RefCell::new(data),
            attributes: RefCell::new(attributes),
            parent: RefCell::new(Weak::new()),
            first_child: RefCell::new(None),
            next_sibling: RefCell::new(None),
            previous_sibling: RefCell::new(None),
            children: RefCell::new(Vec::new()),
            is_block: RefCell::new(None),
            is_code: RefCell::new(None),
        }))
    }

    pub fn get_attribute(&self, name: &str) -> Option<String> {
        self.attributes.borrow().get(name).cloned()
    }

    /// Check if this node (as an Rc<RefCell<Node>>) is the last element child of its parent
    pub fn is_last_element_child_of_parent(node: &Rc<RefCell<Node>>) -> bool {
        let Some(parent) = node.borrow().parent.borrow().upgrade() else {
            return false;
        };

        // Get all element children (not text nodes) of the parent
        let parent_children = parent.borrow().children.borrow().clone();
        let element_children: Vec<_> = parent_children
            .iter()
            .filter(|child| matches!(child.borrow().node_type, NodeType::Element))
            .collect();

        // Check if this node is the last element child
        element_children
            .last()
            .is_some_and(|last_element| Rc::ptr_eq(last_element, node))
    }

    /// Get text content from this node and all its children
    pub fn text_content(&self) -> String {
        match self.node_type {
            NodeType::Text => self
                .data
                .borrow()
                .as_ref()
                .unwrap_or(&String::new())
                .clone(),
            NodeType::Element => {
                let children = self.children.borrow();
                children
                    .iter()
                    .map(|child| child.borrow().text_content())
                    .collect::<String>()
            }
        }
    }

    /// Check if this node is a block element (cached)
    pub fn is_block(&self) -> bool {
        if let Some(cached) = *self.is_block.borrow() {
            return cached;
        }

        let result = is_block(&self.node_name);
        *self.is_block.borrow_mut() = Some(result);
        result
    }

    /// Check if this node is code or has a parent that is code (cached)
    pub fn is_code(node: &Rc<RefCell<Node>>) -> bool {
        if let Some(cached) = *node.borrow().is_code.borrow() {
            return cached;
        }

        let result = {
            let node_borrow = node.borrow();
            if node_borrow.node_name == "CODE" {
                true
            } else if let Some(parent) = node_borrow.parent.borrow().upgrade() {
                Self::is_code(&parent)
            } else {
                false
            }
        };

        *node.borrow().is_code.borrow_mut() = Some(result);
        result
    }

    /// Get flanking whitespace for this node
    pub fn flanking_whitespace(node: &Rc<RefCell<Node>>) -> FlankingWhitespace {
        let node_borrow = node.borrow();

        if node_borrow.is_block() {
            return FlankingWhitespace {
                leading: String::new(),
                trailing: String::new(),
            };
        }

        let text_content = node_borrow.text_content();
        let mut edges = edge_whitespace(&text_content);

        // abandon leading ASCII WS if left-flanked by ASCII WS
        if !edges.leading_ascii.is_empty() && is_flanked_by_whitespace("left", node) {
            edges.leading = edges.leading_non_ascii;
        }

        // abandon trailing ASCII WS if right-flanked by ASCII WS
        if !edges.trailing_ascii.is_empty() && is_flanked_by_whitespace("right", node) {
            edges.trailing = edges.trailing_non_ascii;
        }

        FlankingWhitespace {
            leading: edges.leading,
            trailing: edges.trailing,
        }
    }
}

static EDGE_WHITESPACE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(([ \t\r\n]*)(\s*))(?:(?=\S)[\s\S]*\S)?((\s*?)([ \t\r\n]*))$")
        .expect("valid regex")
});

/// Extract edge whitespace from a string
fn edge_whitespace(string: &str) -> EdgeWhitespace {
    if let Ok(Some(captures)) = EDGE_WHITESPACE_REGEX.captures(string) {
        EdgeWhitespace {
            leading: captures
                .get(1)
                .map_or(String::new(), |m| m.as_str().to_string()),
            leading_ascii: captures
                .get(2)
                .map_or(String::new(), |m| m.as_str().to_string()),
            leading_non_ascii: captures
                .get(3)
                .map_or(String::new(), |m| m.as_str().to_string()),
            trailing: captures
                .get(4)
                .map_or(String::new(), |m| m.as_str().to_string()),
            trailing_non_ascii: captures
                .get(5)
                .map_or(String::new(), |m| m.as_str().to_string()),
            trailing_ascii: captures
                .get(6)
                .map_or(String::new(), |m| m.as_str().to_string()),
        }
    } else {
        // For whitespace-only strings, leading contains the whole string
        EdgeWhitespace {
            leading: string.to_string(),
            leading_ascii: string.to_string(),
            leading_non_ascii: String::new(),
            trailing: String::new(),
            trailing_non_ascii: String::new(),
            trailing_ascii: String::new(),
        }
    }
}

/// Check if a node is flanked by whitespace on the given side
fn is_flanked_by_whitespace(side: &str, node: &Rc<RefCell<Node>>) -> bool {
    let sibling = if side == "left" {
        node.borrow().previous_sibling.borrow().clone()
    } else {
        node.borrow().next_sibling.borrow().clone()
    };

    let Some(sibling_node) = sibling else {
        return false;
    };

    let sibling_borrow = sibling_node.borrow();
    match sibling_borrow.node_type {
        NodeType::Text => {
            let text = sibling_borrow.data.borrow();
            let Some(text) = text.as_ref() else {
                return false;
            };
            if side == "left" {
                text.ends_with(' ')
            } else {
                text.starts_with(' ')
            }
        }
        NodeType::Element => {
            if !sibling_borrow.is_block() {
                let text_content = sibling_borrow.text_content();
                if side == "left" {
                    text_content.ends_with(' ')
                } else {
                    text_content.starts_with(' ')
                }
            } else {
                false
            }
        }
    }
}
