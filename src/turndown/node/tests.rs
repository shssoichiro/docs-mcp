use super::*;

#[test]
fn node_creation() {
    let node = Node::new(NodeType::Element, "DIV".to_string(), None);
    assert_eq!(node.borrow().node_name, "DIV");
    assert!(matches!(node.borrow().node_type, NodeType::Element));
}

#[test]
fn is_block() {
    let div_node = Node::new(NodeType::Element, "DIV".to_string(), None);
    assert!(div_node.borrow().is_block());

    let span_node = Node::new(NodeType::Element, "SPAN".to_string(), None);
    assert!(!span_node.borrow().is_block());
}

#[test]
fn is_code() {
    let code_node = Node::new(NodeType::Element, "CODE".to_string(), None);
    assert!(Node::is_code(&code_node));

    let div_node = Node::new(NodeType::Element, "DIV".to_string(), None);
    assert!(!Node::is_code(&div_node));

    // Test parent code inheritance
    let child_node = Node::new(NodeType::Element, "SPAN".to_string(), None);
    child_node
        .borrow()
        .parent
        .replace(Rc::downgrade(&code_node));
    assert!(Node::is_code(&child_node));
}

#[test]
fn edge_whitespace() {
    // Helper function to create expected EdgeWhitespace like JS test
    fn ews(
        leading_ascii: &str,
        leading_non_ascii: &str,
        trailing_non_ascii: &str,
        trailing_ascii: &str,
    ) -> super::EdgeWhitespace {
        super::EdgeWhitespace {
            leading: format!("{}{}", leading_ascii, leading_non_ascii),
            leading_ascii: leading_ascii.to_string(),
            leading_non_ascii: leading_non_ascii.to_string(),
            trailing: format!("{}{}", trailing_non_ascii, trailing_ascii),
            trailing_non_ascii: trailing_non_ascii.to_string(),
            trailing_ascii: trailing_ascii.to_string(),
        }
    }

    let ws = "\r\n \t";
    let test_cases = [
        (format!("{}HELLO WORLD{}", ws, ws), ews(ws, "", "", ws)),
        (format!("{}H{}", ws, ws), ews(ws, "", "", ws)),
        (
            format!("{}\u{a0}{}HELLO{}WORLD{}\u{a0}{}", ws, ws, ws, ws, ws),
            ews(ws, &format!("\u{a0}{}", ws), &format!("{}\u{a0}", ws), ws),
        ),
        (
            format!("\u{a0}{}HELLO{}WORLD{}\u{a0}", ws, ws, ws),
            ews("", &format!("\u{a0}{}", ws), &format!("{}\u{a0}", ws), ""),
        ),
        (
            format!("\u{a0}{}\u{a0}", ws),
            ews("", &format!("\u{a0}{}\u{a0}", ws), "", ""),
        ),
        (
            format!("{}\u{a0}{}", ws, ws),
            ews(ws, &format!("\u{a0}{}", ws), "", ""),
        ),
        (format!("{}\u{a0}", ws), ews(ws, "\u{a0}", "", "")),
        ("HELLO WORLD".to_string(), ews("", "", "", "")),
        (String::new(), ews("", "", "", "")),
        (format!("TEST{}END", " ".repeat(32767)), ews("", "", "", "")), // performance check
    ];

    for (input, expected) in &test_cases {
        let result = super::edge_whitespace(input);
        assert_eq!(
            result.leading, expected.leading,
            "Failed for input: {:?}",
            input
        );
        assert_eq!(
            result.leading_ascii, expected.leading_ascii,
            "Failed leading_ascii for input: {:?}",
            input
        );
        assert_eq!(
            result.leading_non_ascii, expected.leading_non_ascii,
            "Failed leading_non_ascii for input: {:?}",
            input
        );
        assert_eq!(
            result.trailing, expected.trailing,
            "Failed trailing for input: {:?}",
            input
        );
        assert_eq!(
            result.trailing_non_ascii, expected.trailing_non_ascii,
            "Failed trailing_non_ascii for input: {:?}",
            input
        );
        assert_eq!(
            result.trailing_ascii, expected.trailing_ascii,
            "Failed trailing_ascii for input: {:?}",
            input
        );
    }
}

#[test]
fn flanking_whitespace() {
    let options = TurndownOptions::default();

    // Block element should have no flanking whitespace
    let div_node = Node::new(NodeType::Element, "DIV".to_string(), None);
    let flanking = Node::flanking_whitespace(&div_node, &options);
    assert_eq!(flanking.leading, "");
    assert_eq!(flanking.trailing, "");

    // Code element with preformatted_code option
    let code_node = Node::new(NodeType::Element, "CODE".to_string(), None);
    let preformatted_options = TurndownOptions {
        preformatted_code: true,
        ..TurndownOptions::default()
    };
    let flanking_code = Node::flanking_whitespace(&code_node, &preformatted_options);
    assert_eq!(flanking_code.leading, "");
    assert_eq!(flanking_code.trailing, "");
}

#[test]
fn text_content() {
    let text_node = Node::new(
        NodeType::Text,
        "#text".to_string(),
        Some("Hello".to_string()),
    );
    assert_eq!(text_node.borrow().text_content(), "Hello");

    let div_node = Node::new(NodeType::Element, "DIV".to_string(), None);
    let child_text = Node::new(
        NodeType::Text,
        "#text".to_string(),
        Some("World".to_string()),
    );
    div_node.borrow().children.borrow_mut().push(child_text);
    assert_eq!(div_node.borrow().text_content(), "World");
}
