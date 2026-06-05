//! Domain types for Ailly's content layer.

/// Macro that derives the standard string-newtype boilerplate: `Clone`,
/// `Debug`, `PartialEq`, `Eq`, `Hash`, `as_str()`, `From<String>`,
/// `From<&str>`, `AsRef<str>`, and `Display`. Pass additional attributes
/// (extra derives, serde annotations, doc comments) via `$(#[$meta])*` before
/// the type name; they are emitted before the macro-generated derive line.
macro_rules! string_newtype {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Debug, PartialEq, Eq, Hash)]
        pub struct $name(String);

        impl $name {
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl From<String> for $name {
            fn from(value: String) -> Self {
                Self(value)
            }
        }

        impl From<&str> for $name {
            fn from(value: &str) -> Self {
                Self(value.to_owned())
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

pub mod assembly;
pub mod conversation;
pub mod evaluation;
pub mod project;
pub mod repository;

pub use project::PathError;
pub use project::Project;
pub use project::ProjectError;
pub use project::ProjectPath;
pub use project::RunTx;
