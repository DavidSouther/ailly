//! Core library for Ailly.

/// A greeting message produced by Ailly.
pub struct Greeting {
    /// The text content of the greeting.
    text: String,
}

impl Greeting {
    /// Creates a new [`Greeting`] with the given text.
    pub fn new<T>(text: T) -> Self
    where
        T: Into<String>,
    {
        Self { text: text.into() }
    }

    /// Returns the greeting text.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }
}

#[cfg(test)]
mod tests {
    use super::Greeting;

    #[test]
    fn new_stores_text() {
        let greeting = Greeting::new("hello");
        assert_eq!(greeting.text(), "hello");
    }

    #[test]
    fn text_round_trips_through_string() {
        let greeting = Greeting::new(String::from("owned"));
        assert_eq!(greeting.text(), "owned");
    }
}
