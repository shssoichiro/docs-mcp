use crate::turndown::node::NodeType;

use super::*;

#[test]
fn list_last_element_child() {
    let rules = Rules::default();
    let options = TurndownOptions::default();

    // Create a list item (LI) with a nested list as the last child
    let li_node = Node::new(NodeType::Element, "LI".to_string(), None);

    let ul_node = Node::new(NodeType::Element, "UL".to_string(), None);

    // Set up parent-child relationship
    ul_node.borrow().parent.replace(Rc::downgrade(&li_node));
    li_node
        .borrow()
        .children
        .borrow_mut()
        .push(Rc::clone(&ul_node));

    // Test when UL is the last (and only) element child of LI
    let result =
        (rules.get("list").expect("has list rule").replacement)("test content", &ul_node, &options);
    assert_eq!(result, "\ntest content");

    // Create another element child to make UL not the last
    let p_node = Node::new(NodeType::Element, "P".to_string(), None);
    p_node.borrow().parent.replace(Rc::downgrade(&li_node));
    li_node.borrow().children.borrow_mut().push(p_node);

    // Now UL should not be the last element child
    let result2 =
        (rules.get("list").expect("has list rule").replacement)("test content", &ul_node, &options);
    assert_eq!(result2, "\n\ntest content\n\n");
}

#[test]
fn list_without_li_parent() {
    let rules = Rules::default();
    let options = TurndownOptions::default();

    // Create a list without LI parent
    let ul_node = Node::new(NodeType::Element, "UL".to_string(), None);

    // Test when UL has no LI parent
    let result =
        (rules.get("list").expect("has list rule").replacement)("test content", &ul_node, &options);
    assert_eq!(result, "\n\ntest content\n\n");
}
