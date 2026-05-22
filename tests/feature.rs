use ailly_two::Greeting;

#[test]
fn greeting_preserves_text() {
    let greeting = Greeting::new("Hello from a feature test");
    assert_eq!(greeting.text(), "Hello from a feature test");
}

#[test]
fn greeting_accepts_owned_string() {
    let greeting = Greeting::new(String::from("owned input"));
    assert_eq!(greeting.text(), "owned input");
}
