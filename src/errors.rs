use actix_web::ResponseError;
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

#[derive(Debug, Error)]
pub enum ResponseErrors {
    #[error("Error occured in the Redis Backend service. See error response: {0}")]
    RedisError(String),
    #[error("The Redis server returned a Nil response.")]
    RedisNilValue,
    #[error(
        "An error was encountered when attempting to convert a Redis value. See error raised: {0}."
    )]
    RedisConversionError(String),
    #[error("Error raised when converting Redis Value to json. See error: {0}")]
    SerdeJsonConversionError(String),
    #[error("Matching failed. {0}")]
    IncorrectDetails(String),
    #[error("The following error is raised when attempting to retrieve data from Redis: {0}")]
    MissingInformation(String),
    #[error(
        "Expected the following HTTP method: {0}. Got the following HTTP method instead: {1}."
    )]
    IncorrectHttpMethod(String, String),
    #[error("Placeholder Error. Should not be present in the final production code.")]
    PlaceholderError,
}

impl ResponseError for ResponseErrors {}
