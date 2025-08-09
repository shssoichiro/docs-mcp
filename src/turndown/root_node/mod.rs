#[cfg(test)]
mod tests;

use super::collapse_whitespace::{CollapseWhitespaceOptions, collapse_whitespace};
use super::node::{Node, NodeType};
use super::utilities::{is_block, is_void};
use scraper::{Html, Selector};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

pub struct RootNode {
    pub children: Vec<Rc<RefCell<Node>>>,
}

impl RootNode {
    pub fn new(input: &str) -> Self {
        let root_options = RootNodeOptions::default();
        let root_node_rc = root_node(RootNodeInput::Html(input.to_string()), &root_options);
        let children = root_node_rc.borrow().children.borrow().clone();
        RootNode { children }
    }
}

#[derive(Default)]
pub struct RootNodeOptions {
    pub preformatted_code: bool,
}

/// Create a root node from HTML string or existing node
pub fn root_node(input: RootNodeInput, options: &RootNodeOptions) -> Rc<RefCell<Node>> {
    let root = match input {
        RootNodeInput::Html(html_string) => {
            // Wrap in custom element to ensure reliable parsing
            let wrapped_html = format!(
                "<x-turndown id=\"turndown-root\">{}</x-turndown>",
                html_string
            );

            // Parse HTML using scraper
            let document = Html::parse_document(&wrapped_html);

            // Find the root element
            let root_selector = Selector::parse("#turndown-root").expect("valid selector");
            let root_element = document
                .select(&root_selector)
                .next()
                .expect("root element should exist");

            // Convert scraper element to our Node structure
            convert_element_to_node(root_element)
        }
        RootNodeInput::Node(node) => {
            // Clone the existing node
            clone_node(&node)
        }
    };

    // Apply whitespace collapse
    let is_pre_fn = options
        .preformatted_code
        .then(|| Box::new(is_pre_or_code) as Box<dyn Fn(&Rc<RefCell<Node>>) -> bool>);

    let is_block_fn = |node: &Rc<RefCell<Node>>| -> bool { is_block(&node.borrow().node_name) };

    let is_void_fn = |node: &Rc<RefCell<Node>>| -> bool { is_void(&node.borrow().node_name) };

    collapse_whitespace(CollapseWhitespaceOptions {
        element: Rc::clone(&root),
        is_block: is_block_fn,
        is_void: is_void_fn,
        is_pre: is_pre_fn,
    });

    root
}

/// Input types for root_node function
pub enum RootNodeInput {
    Html(String),
    #[allow(dead_code)]
    Node(Rc<RefCell<Node>>),
}

/// Convert scraper ElementRef to our Node structure
fn convert_element_to_node(element: scraper::ElementRef) -> Rc<RefCell<Node>> {
    let tag_name = element.value().name().to_uppercase();

    // Collect attributes
    let mut attributes = HashMap::new();
    for (name, value) in element.value().attrs() {
        attributes.insert(name.to_string(), value.to_string());
    }

    let node = Node::with_attributes(NodeType::Element, tag_name, None, attributes);

    // Process children
    let mut children = Vec::new();
    for child in element.children() {
        match child.value() {
            scraper::node::Node::Element(_elem) => {
                let child_element_ref = scraper::ElementRef::wrap(child).expect("valid element");
                let child_node = convert_element_to_node(child_element_ref);
                child_node.borrow().parent.replace(Rc::downgrade(&node));
                children.push(child_node);
            }
            scraper::node::Node::Text(text) => {
                let text_content = text.text.to_string();
                if !text_content.is_empty() {
                    let text_node =
                        Node::new(NodeType::Text, "#text".to_string(), Some(text_content));
                    text_node.borrow().parent.replace(Rc::downgrade(&node));
                    children.push(text_node);
                }
            }
            _ => {} // Skip comments, processing instructions, etc.
        }
    }

    // Set up sibling relationships
    for (i, child) in children.iter().enumerate() {
        if i > 0 {
            child
                .borrow()
                .previous_sibling
                .replace(Some(Rc::clone(&children[i - 1])));
        }
        if i < children.len() - 1 {
            child
                .borrow()
                .next_sibling
                .replace(Some(Rc::clone(&children[i + 1])));
        }
    }

    // Set first child if we have children
    if let Some(first) = children.first() {
        node.borrow().first_child.replace(Some(Rc::clone(first)));
    }

    // Store all children
    *node.borrow().children.borrow_mut() = children;

    node
}

/// Clone a node recursively
fn clone_node(original: &Rc<RefCell<Node>>) -> Rc<RefCell<Node>> {
    let original_borrow = original.borrow();

    let cloned = Node::with_attributes(
        original_borrow.node_type.clone(),
        original_borrow.node_name.clone(),
        original_borrow.data.borrow().clone(),
        original_borrow.attributes.borrow().clone(),
    );

    // Clone children recursively
    let mut cloned_children = Vec::new();
    for child in original_borrow.children.borrow().iter() {
        let cloned_child = clone_node(child);
        cloned_child.borrow().parent.replace(Rc::downgrade(&cloned));
        cloned_children.push(cloned_child);
    }

    // Set up sibling relationships
    for (i, child) in cloned_children.iter().enumerate() {
        if i > 0 {
            child
                .borrow()
                .previous_sibling
                .replace(Some(Rc::clone(&cloned_children[i - 1])));
        }
        if i < cloned_children.len() - 1 {
            child
                .borrow()
                .next_sibling
                .replace(Some(Rc::clone(&cloned_children[i + 1])));
        }
    }

    // Set first child if we have children
    if let Some(first) = cloned_children.first() {
        cloned.borrow().first_child.replace(Some(Rc::clone(first)));
    }

    // Store all children
    *cloned.borrow().children.borrow_mut() = cloned_children;

    cloned
}

/// Check if node is PRE or CODE element
fn is_pre_or_code(node: &Rc<RefCell<Node>>) -> bool {
    let node_name = &node.borrow().node_name;
    node_name == "PRE" || node_name == "CODE"
}
