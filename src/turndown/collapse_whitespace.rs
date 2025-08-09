//! The collapseWhitespace function is adapted from collapse-whitespace
//! by Luc Thevenard.
//!
//! The MIT License (MIT)
//!
//! Copyright (c) 2014 Luc Thevenard <lucthevenard@gmail.com>
//!
//! Permission is hereby granted, free of charge, to any person obtaining a copy
//! of this software and associated documentation files (the "Software"), to deal
//! in the Software without restriction, including without limitation the rights
//! to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
//! copies of the Software, and to permit persons to whom the Software is
//! furnished to do so, subject to the following conditions:
//!
//! The above copyright notice and this permission notice shall be included in
//! all copies or substantial portions of the Software.
//!
//! THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
//! IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
//! FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
//! AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
//! LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
//! OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
//! THE SOFTWARE.

use super::node::{Node, NodeType};
use fancy_regex::Regex;
use std::cell::RefCell;
use std::rc::Rc;

type IsPreFn = Box<dyn Fn(&Rc<RefCell<Node>>) -> bool>;

pub struct CollapseWhitespaceOptions<F1, F2>
where
    F1: Fn(&Rc<RefCell<Node>>) -> bool,
    F2: Fn(&Rc<RefCell<Node>>) -> bool,
{
    pub element: Rc<RefCell<Node>>,
    pub is_block: F1,
    pub is_void: F2,
    pub is_pre: Option<IsPreFn>,
}

pub fn collapse_whitespace<F1, F2>(options: CollapseWhitespaceOptions<F1, F2>)
where
    F1: Fn(&Rc<RefCell<Node>>) -> bool,
    F2: Fn(&Rc<RefCell<Node>>) -> bool,
{
    let element = options.element;
    let is_block = options.is_block;
    let is_void = options.is_void;
    let default_is_pre: IsPreFn =
        Box::new(|node: &Rc<RefCell<Node>>| node.borrow().node_name == "PRE");
    let is_pre = options.is_pre.as_ref().unwrap_or(&default_is_pre);

    // Check if element has first child or is PRE
    if element.borrow().first_child.borrow().is_none() || is_pre(&element) {
        return;
    }

    let whitespace_regex = Regex::new(r"[ \r\n\t]+").expect("regex is valid");
    let mut prev_text: Option<Rc<RefCell<Node>>> = None;
    let mut keep_leading_ws = false;

    let mut prev: Option<Rc<RefCell<Node>>> = None;
    let mut node = next(prev.as_ref(), &element, is_pre);

    while let Some(current_node) = node {
        if Rc::ptr_eq(&current_node, &element) {
            break;
        }

        let node_type = current_node.borrow().node_type.clone();

        match node_type {
            NodeType::Text => {
                let mut text = current_node
                    .borrow()
                    .data
                    .borrow()
                    .as_ref()
                    .unwrap_or(&String::new())
                    .clone();
                text = whitespace_regex.replace_all(&text, " ").to_string();

                // Check if we should remove leading whitespace
                if (prev_text.is_none()
                    || prev_text
                        .as_ref()
                        .expect("prev_text is set")
                        .borrow()
                        .data
                        .borrow()
                        .as_ref()
                        .is_some_and(|data| data.ends_with(' ')))
                    && !keep_leading_ws
                    && text.starts_with(' ')
                {
                    text = text.chars().skip(1).collect();
                }

                // If text is empty, remove the node
                if text.is_empty() {
                    node = remove(&current_node);
                    continue;
                }

                *current_node.borrow().data.borrow_mut() = Some(text);
                prev_text = Some(Rc::clone(&current_node));
            }
            NodeType::Element => {
                if is_block(&current_node) || current_node.borrow().node_name == "BR" {
                    if let Some(ref prev_text_node) = prev_text {
                        let prev_borrow = prev_text_node.borrow();
                        let mut data = prev_borrow.data.borrow_mut();
                        if let Some(ref mut text) = *data
                            && text.ends_with(' ')
                        {
                            text.pop();
                        }
                    }
                    prev_text = None;
                    keep_leading_ws = false;
                } else if is_void(&current_node) || is_pre(&current_node) {
                    prev_text = None;
                    keep_leading_ws = true;
                } else if prev_text.is_some() {
                    keep_leading_ws = false;
                }
            }
        }

        let next_node = next(prev.as_ref(), &current_node, is_pre);
        prev = Some(current_node);
        node = next_node;
    }

    // Clean up trailing whitespace on the last text node
    if let Some(ref prev_text_node) = prev_text {
        let should_remove = {
            let prev_borrow = prev_text_node.borrow();
            let mut data = prev_borrow.data.borrow_mut();
            data.as_mut().is_some_and(|text| {
                if text.ends_with(' ') {
                    text.pop();
                }
                text.is_empty()
            })
        };

        if should_remove {
            remove(prev_text_node);
        }
    }
}

fn remove(node: &Rc<RefCell<Node>>) -> Option<Rc<RefCell<Node>>> {
    let next_node = {
        let node_borrow = node.borrow();
        node_borrow
            .next_sibling
            .borrow()
            .clone()
            .or_else(|| node_borrow.parent.borrow().upgrade())
    };

    // Remove node from parent's children
    if let Some(parent) = node.borrow().parent.borrow().upgrade() {
        {
            let parent_borrow = parent.borrow();
            let mut parent_children = parent_borrow.children.borrow_mut();
            parent_children.retain(|child| !Rc::ptr_eq(child, node));
        }

        // Update first_child if necessary
        if parent
            .borrow()
            .first_child
            .borrow()
            .as_ref()
            .is_some_and(|fc| Rc::ptr_eq(fc, node))
        {
            *parent.borrow().first_child.borrow_mut() = node.borrow().next_sibling.borrow().clone();
        }
    }

    next_node
}

fn next<F>(
    prev: Option<&Rc<RefCell<Node>>>,
    current: &Rc<RefCell<Node>>,
    is_pre: &F,
) -> Option<Rc<RefCell<Node>>>
where
    F: Fn(&Rc<RefCell<Node>>) -> bool,
{
    let should_skip_children = prev.is_some_and(|prev_node| {
        Rc::ptr_eq(
            &prev_node
                .borrow()
                .parent
                .borrow()
                .upgrade()
                .unwrap_or_else(|| Rc::clone(current)),
            current,
        )
    }) || is_pre(current);

    if should_skip_children {
        current
            .borrow()
            .next_sibling
            .borrow()
            .clone()
            .or_else(|| current.borrow().parent.borrow().upgrade())
    } else {
        current
            .borrow()
            .first_child
            .borrow()
            .clone()
            .or_else(|| current.borrow().next_sibling.borrow().clone())
            .or_else(|| current.borrow().parent.borrow().upgrade())
    }
}
