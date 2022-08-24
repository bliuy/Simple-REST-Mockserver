use thiserror::Error;

#[derive(Debug, Error)]
pub enum RegistrationError {
    #[error("Expected > 4 characters for the unique key, got {actual_length}.")]
    UniqueKeyTooShort { actual_length: usize },
    #[error("Non ASCII characters detected within the input unique key: {0}")]
    UniqueKeyNonAscii(String),
    #[error("Invalid HTTP status code used: {0}")]
    InvalidHttpStatusCode(u16),
}
