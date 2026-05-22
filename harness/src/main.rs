use ailly_two::Greeting;

#[expect(clippy::print_stdout, reason = "binary entry point")]
fn main() {
    let greeting = Greeting::new("Hello, Ailly!");
    println!("{}", greeting.text());
}
