use std::cell::Cell;

use actix_web::{
    web::{self, Json},
    App, Either, HttpRequest, HttpResponse, HttpServer, Responder, ResponseError,
};
use errors::ResponseErrors;
use redis::{
    aio::{Connection, ConnectionManager},
    AsyncCommands,
};
use serde::{de::IntoDeserializer, Deserialize, Serialize};
mod errors;
// mod redishandler;
mod registration;
use crate::registration::MockServerPayload;

pub(crate) struct AppData {
    redis_conn: ConnectionManager,
}

#[actix_web::get("/hello_world")]
async fn hello() -> impl Responder {
    HttpResponse::Ok().body("Hello world!")
}

#[actix_web::post("/echo")]
async fn echo(req_body: String) -> impl Responder {
    HttpResponse::Ok().body(req_body)
}

#[actix_web::post("/register")]
async fn register(
    app_data: actix_web::web::Data<AppData>,
    request_body: Json<MockServerPayload>,
) -> impl Responder {
    println!("Register request recevied.");
    let payload = request_body.into_inner();

    // Returning 400 Bad Request if validation fails
    if let Err(e) = registration::validate_registration_request(&payload) {
        return HttpResponse::BadRequest().body(format!("{}", e));
    }

    // Getting a connection handle to the redis backend
    let redis_conn = &mut app_data.redis_conn.clone();

    // Checking if the request unique key record already exists
    let exists: Result<bool, redis::RedisError> =
        redis_conn.exists(&payload.http_request.unique_key).await;
    match exists {
        Ok(i) => match i {
            false => {}
            true => {
                return HttpResponse::MethodNotAllowed()
                    .body("Unique key already exists within the database!")
            }
        },
        Err(e) => {
            return HttpResponse::BadRequest()
                .body("Error when checking the uniqueness of the request key.")
        }
    }

    // Writing the record into the redis backend
    let unique_key: &str = &payload.http_request.unique_key;
    let request_config_serialized =
        match serde_json::to_string(&payload.http_request.request_config) {
            Ok(i) => i,
            Err(err) => {
                return HttpResponse::InternalServerError().body(format!(
                    "Error when attempting to save the request headers: {:#?}",
                    err
                ))
            }
        };
    let request_method = payload.http_request.method.clone();
    let response_config_serialized =
        match serde_json::to_string(&payload.http_response.response_config) {
            Ok(i) => i,
            Err(err) => {
                return HttpResponse::InternalServerError().body(format!(
                    "Error when attempting to save the request headers: {:#?}",
                    err
                ))
            }
        };

    // Constructing the required pipeline
    let result: Result<i8, redis::RedisError> = redis::Pipeline::new()
        .atomic()
        .hset_nx(unique_key, "request_config", request_config_serialized)
        .hset_nx(unique_key, "request_method", request_method)
        .hset_nx(unique_key, "response_config", response_config_serialized)
        .query_async(redis_conn)
        .await;

    HttpResponse::Ok().body(format!("{}", result.unwrap()))
}

// #[actix_web::get("/poll/{unique_key}")]
async fn poll(
    app_data: actix_web::web::Data<AppData>,
    unique_key_path: web::Path<String>,
    request: HttpRequest,
) -> Result<HttpResponse, ResponseErrors> {
    // Setup
    let unique_key = unique_key_path.into_inner();
    let redis_conn = &mut app_data.redis_conn.clone();

    // Validating the request method
    redis_conn
        .hget(&unique_key, "request_method")
        .await
        .map_err(|e| ResponseErrors::RedisError(format!("{}", e)))
        .map(|i| -> Result<String, ResponseErrors> {
            let _: redis::Value = i;
            redis::FromRedisValue::from_redis_value(&i)
                .map_err(|e| ResponseErrors::RedisError(format!("{}", e)))
        })?
        .map(|i| -> Result<String, ResponseErrors> {
            if i != request.method().as_str() {
                return Err(ResponseErrors::IncorrectHttpMethod(
                    i,
                    request.method().as_str().to_owned(),
                ));
            }
            Ok(i)
        })??;

    // Getting the registered request configuration
    // The configuration indicates how the incoming request should be structured.
    // If the incoming request matches the registered configuration, the corresponding registered response will be returned.
    let registered_request_configuration = redis_conn
        .hget(&unique_key, "request_config")
        .await
        .map_err(|e| ResponseErrors::RedisError(format!("{}", e)))
        .map(|i| -> Result<redis::Value, _> {
            if i == redis::Value::Nil {
                return Err(ResponseErrors::RedisNilValue);
            }
            Ok(i)
        })?
        .map(|i| -> Result<String, _> {
            redis::FromRedisValue::from_redis_value(&i)
                .map_err(|e| ResponseErrors::RedisConversionError(e.to_string()))
        })?
        .map(|i| -> Result<serde_json::Value, _> {
            serde_json::from_str(&i)
                .map_err(|e| ResponseErrors::SerdeJsonConversionError(e.to_string()))
        })??;

    // Validating headers - Checking if all the header fields in the registration config is found in the current request
    let registered_headers = registered_request_configuration
        .get("headers")
        .ok_or(ResponseErrors::MissingInformation(
            "'headers' field is missing from the Redis dataset.".to_owned(),
        ))
        .map(|i| {
            i.as_object().ok_or(ResponseErrors::RedisConversionError(
                "Cannot convert registered headers to Map object".to_owned(),
            ))
        })??;

    for (header_name, header_val) in registered_headers.iter() {
        request
            .headers()
            .get(header_name)
            .ok_or(ResponseErrors::IncorrectDetails(format!(
                "Missing the following header: {}",
                header_name
            )))
            .map(|i| -> Result<(), ResponseErrors> {
                let registered_value =
                    header_val
                        .as_str()
                        .ok_or(ResponseErrors::RedisConversionError(
                            "Cannot convert header to String object".to_owned(),
                        ))?;
                let current_value = i.to_str().map_err(|_| {
                    ResponseErrors::RedisConversionError(
                        "Unable to convert header value to String".to_owned(),
                    )
                })?;
                if registered_value != current_value {
                    return Err(ResponseErrors::PlaceholderError);
                }
                Ok(())
            })??;
    }

    // Constructing the response
    // Validation has been completed in the prior steps

    let registered_response = redis_conn
        .hget(&unique_key, "response_config")
        .await
        .map_err(|e| ResponseErrors::RedisError(format!("{}", e)))
        .map(|i| -> Result<redis::Value, _> {
            if i == redis::Value::Nil {
                return Err(ResponseErrors::RedisNilValue);
            }
            Ok(i)
        })?
        .map(|i| -> Result<String, _> {
            redis::FromRedisValue::from_redis_value(&i)
                .map_err(|e| ResponseErrors::RedisConversionError(e.to_string()))
        })?
        .map(|i| -> Result<serde_json::Value, _> {
            serde_json::from_str(&i)
                .map_err(|e| ResponseErrors::SerdeJsonConversionError(e.to_string()))
        })??;

    let registered_response_headers = registered_response
        .get("headers")
        .ok_or(ResponseErrors::RedisError(
            "'headers' field is missing from the 'response_config' section.".to_owned(),
        ))
        .map(|i| {
            i.as_object().ok_or(ResponseErrors::RedisConversionError(
                "Cannot convert registered headers to Map object".to_owned(),
            ))
        })??;

    let registered_response_body = registered_response
        .get("body")
        .ok_or(ResponseErrors::RedisError(
            "'body' field is missing from the 'response_config' section.".to_owned(),
        ))
        .map(|i| {
            i.as_str().ok_or(ResponseErrors::RedisConversionError(
                "Cannot convert registered body to String object".to_owned(),
            ))
        })??;

    let mut response = HttpResponse::Ok();
    for (k, v) in registered_response_headers.iter() {
        let header_pair = (
            k.clone(),
            v.as_str()
                .ok_or(ResponseErrors::RedisConversionError(
                    "Cannot convert registered header to String object".to_owned(),
                ))?
                .to_owned(),
        );
        response.append_header(header_pair);
    }

    let final_response = response.body(registered_response_body.to_owned());

    Ok(final_response)
}

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    // Creating the async connection instance to the Redis backend
    let redis_client = redis::Client::open("redis://127.0.0.1:6379")?;
    // let redis_client = redis::Client::open("localhost:6379")?;
    let redis_conn = ConnectionManager::new(redis_client).await?;

    // Creating the redis handler function

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(AppData {
                redis_conn: redis_conn.clone(),
            }))
            .service(hello)
            .service(echo)
            .service(register)
            .service(
                web::resource("/poll/{unique_key}").route(
                    web::route()
                        .guard(actix_web::guard::Get())
                        // .guard(actix_web::guard::Put())
                        // .guard(actix_web::guard::Post())
                        // .guard(actix_web::guard::Patch())
                        .to(poll),
                ),
            )
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await?;

    Ok(())
}
