use std::collections::HashMap;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::errors::RegistrationError;

#[derive(Debug, Deserialize, Serialize)]
pub struct MockServerPayloadHttpRequestRequestConfig {
    pub(crate) headers: Option<HashMap<String, String>>,
    pub(crate) body: Option<String>,
}
#[derive(Debug, Deserialize, Serialize)]
pub struct MockServerPayloadHttpRequestResponseConfig {
    pub(crate) headers: Option<HashMap<String, String>>,
    pub(crate) body: Option<String>,
}
#[derive(Debug, Deserialize, Serialize)]
pub struct MockServerPayloadHttpRequest {
    pub(crate) method: String,
    pub(crate) unique_key: String,
    pub(crate) request_config: MockServerPayloadHttpRequestRequestConfig,
}
#[derive(Debug, Deserialize, Serialize)]
pub struct MockServerPayloadHttpResponse {
    pub(crate) status_code: u16,
    pub(crate) response_config: MockServerPayloadHttpRequestResponseConfig,
}
#[derive(Debug, Deserialize, Serialize)]
pub struct MockServerPayload {
    pub(crate) http_request: MockServerPayloadHttpRequest,
    pub(crate) http_response: MockServerPayloadHttpResponse,
}

/// Function consumes the MockServerPayload to create a RegistrationPayload instance.
pub(crate) fn validate_registration_request(request: &MockServerPayload) -> Result<()> {
    // Validating the HTTP method field
    match request.http_request.method.as_str() {
        "GET" | "POST" | "PUT" | "PATCH" => {}
        _ => {}
    }

    // Validating the the unique_id meets the requirements - > 4 characters long, all ASCII
    // Checking if the key exists already in the DB comes later.

    if request.http_request.unique_key.len() <= 4 {
        return Err(RegistrationError::UniqueKeyTooShort {
            actual_length: request.http_request.unique_key.len(),
        }
        .into());
    }

    if request.http_request.unique_key.is_ascii() == false {
        return Err(
            RegistrationError::UniqueKeyNonAscii(request.http_request.unique_key.clone()).into(),
        );
    }

    // Checking the validity of the response status code
    let provided_status_code = request.http_response.status_code;
    if let Err(_) = actix_web::http::StatusCode::from_u16(provided_status_code) {
        return Err(RegistrationError::InvalidHttpStatusCode(provided_status_code).into());
    }

    Ok(())
}
