use std::error::Error;
use std::fmt;

/// An error whose message is already user-facing and should be shown verbatim
/// (without the generic "unexpected error" wrapper).
#[derive(Debug)]
pub struct UserError(pub String);

impl fmt::Display for UserError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Error for UserError {}
