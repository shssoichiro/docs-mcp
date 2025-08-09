use super::*;

#[test]
fn root_node_from_html() {
    let html = "<p>Hello <strong>world</strong></p>";
    let options = RootNodeOptions::default();
    let root = root_node(RootNodeInput::Html(html.to_string()), &options);

    assert_eq!(root.borrow().node_name, "X-TURNDOWN");
    assert_eq!(
        root.borrow().get_attribute("id"),
        Some("turndown-root".to_string())
    );

    // Check that we have children
    let binding = root.borrow();
    let children = binding.children.borrow();
    assert!(!children.is_empty());
}

#[test]
fn root_node_from_node() {
    let original = Node::new(NodeType::Element, "DIV".to_string(), None);
    let child = Node::new(
        NodeType::Text,
        "#text".to_string(),
        Some("Hello".to_string()),
    );

    child.borrow().parent.replace(Rc::downgrade(&original));
    original
        .borrow()
        .children
        .borrow_mut()
        .push(Rc::clone(&child));
    original
        .borrow()
        .first_child
        .replace(Some(Rc::clone(&child)));

    let options = RootNodeOptions::default();
    let root = root_node(RootNodeInput::Node(original), &options);

    assert_eq!(root.borrow().node_name, "DIV");
    assert_eq!(root.borrow().children.borrow().len(), 1);
    assert_eq!(
        root.borrow().children.borrow()[0].borrow().text_content(),
        "Hello"
    );
}

#[test]
fn is_pre_or_code() {
    let pre_node = Node::new(NodeType::Element, "PRE".to_string(), None);
    assert!(super::is_pre_or_code(&pre_node));

    let code_node = Node::new(NodeType::Element, "CODE".to_string(), None);
    assert!(super::is_pre_or_code(&code_node));

    let div_node = Node::new(NodeType::Element, "DIV".to_string(), None);
    assert!(!super::is_pre_or_code(&div_node));
}

#[test]
fn clone_node() {
    let original = Node::new(NodeType::Element, "DIV".to_string(), None);
    let child = Node::new(
        NodeType::Text,
        "#text".to_string(),
        Some("Hello".to_string()),
    );

    child.borrow().parent.replace(Rc::downgrade(&original));
    original
        .borrow()
        .children
        .borrow_mut()
        .push(Rc::clone(&child));
    original
        .borrow()
        .first_child
        .replace(Some(Rc::clone(&child)));

    let cloned = super::clone_node(&original);

    assert_eq!(cloned.borrow().node_name, "DIV");
    assert_eq!(cloned.borrow().children.borrow().len(), 1);
    assert_eq!(
        cloned.borrow().children.borrow()[0].borrow().text_content(),
        "Hello"
    );

    // Ensure it's a different instance
    assert!(!Rc::ptr_eq(&original, &cloned));
}

#[test]
fn preformatted_code_option() {
    let html = "<pre><code>var x = 1;</code></pre>";
    let options = RootNodeOptions {
        preformatted_code: true,
    };
    let root = root_node(RootNodeInput::Html(html.to_string()), &options);

    // Should successfully parse without panicking
    assert_eq!(root.borrow().node_name, "X-TURNDOWN");
}
