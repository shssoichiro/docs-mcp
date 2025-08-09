use crate::turndown::{LinkReferenceStyle, LinkStyle, node::NodeType};

use super::*;

#[test]
fn reference_link_functionality() {
    let mut rules = Rules::default();
    let options = TurndownOptions {
        link_style: LinkStyle::Referenced,
        link_reference_style: LinkReferenceStyle::Full,
        ..TurndownOptions::default()
    };

    // Create a mock link node with href and title
    let mut attributes = HashMap::new();
    attributes.insert("href".to_string(), "https://example.com".to_string());
    attributes.insert("title".to_string(), "Example Site".to_string());

    let link_node = Node::with_attributes(NodeType::Element, "A".to_string(), None, attributes);

    // Apply the reference link rule
    let result = rules.apply_rule("referenceLink", "Example Link", &link_node, &options);

    // Check that we get a reference link format
    assert_eq!(result, "[Example Link][1]");

    // Check that references were stored
    let references = rules.get_references("referenceLink", false);
    assert_eq!(
        references,
        "\n\n[1]: https://example.com \"Example Site\"\n\n"
    );

    // Test clearing references
    let references_cleared = rules.get_references("referenceLink", true);
    assert_eq!(
        references_cleared,
        "\n\n[1]: https://example.com \"Example Site\"\n\n"
    );

    // After clearing, should be empty
    let empty_refs = rules.get_references("referenceLink", false);
    assert_eq!(empty_refs, "");
}

#[test]
fn reference_link_collapsed_style() {
    let mut rules = Rules::default();
    let options = TurndownOptions {
        link_style: LinkStyle::Referenced,
        link_reference_style: LinkReferenceStyle::Collapsed,
        ..TurndownOptions::default()
    };

    let mut attributes = HashMap::new();
    attributes.insert("href".to_string(), "https://example.com".to_string());

    let link_node = Node::with_attributes(NodeType::Element, "A".to_string(), None, attributes);

    let result = rules.apply_rule("referenceLink", "Example Link", &link_node, &options);
    assert_eq!(result, "[Example Link][]");

    let references = rules.get_references("referenceLink", true);
    assert_eq!(references, "\n\n[Example Link]: https://example.com\n\n");
}

#[test]
fn reference_link_shortcut_style() {
    let mut rules = Rules::default();
    let options = TurndownOptions {
        link_style: LinkStyle::Referenced,
        link_reference_style: LinkReferenceStyle::Shortcut,
        ..TurndownOptions::default()
    };

    let mut attributes = HashMap::new();
    attributes.insert("href".to_string(), "https://example.com".to_string());

    let link_node = Node::with_attributes(NodeType::Element, "A".to_string(), None, attributes);

    let result = rules.apply_rule("referenceLink", "Example Link", &link_node, &options);
    assert_eq!(result, "[Example Link]");

    let references = rules.get_references("referenceLink", true);
    assert_eq!(references, "\n\n[Example Link]: https://example.com\n\n");
}

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
