use std::cell::Cell;

use actix_web::{
    web::{self, Json},
    App, Either, HttpRequest, HttpResponse, HttpServer, Responder,
};
use anyhow::Result;
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
) -> impl Responder {
    // General configuration
    let unique_key = unique_key_path.into_inner();
    let redis_conn = &mut app_data.redis_conn.clone();

    // Getting the corresponding data tied to the unique key within the Redis environment
    let request_method: Option<String> =
        match redis_conn.hget(&unique_key, "request_method").await.ok() {
            Some(redis_response) if redis_response != redis::Value::Nil => {
                let redis_response: redis::Value = redis_response;
                redis::FromRedisValue::from_redis_value(&redis_response).ok()
            }
            _ => None,
        };

    // Validating that the method call used aligns with what was specified during the registration process.
    match request_method {
        Some(i) => {
            if request.method().as_str() != i {
                return HttpResponse::MethodNotAllowed().finish();
            }
        }
        None => match redis_conn.exists(&unique_key).await {
            Ok(i) if i == 1 => {
                let _: i8 = i;
                return HttpResponse::NotFound().finish();
            }
            _ => return HttpResponse::InternalServerError().finish(),
        },
    }

    // Getting + deserializing the "request_config" field.
    let request_config: Option<serde_json::Value> =
        match redis_conn.hget(&unique_key, "request_config").await.ok() {
            Some(redis_response) if redis_response != redis::Value::Nil => {
                if let Some(i) = redis::FromRedisValue::from_redis_value(&redis_response).ok() {
                    let _: String = i;
                    serde_json::from_str::<'_, serde_json::Value>(&i).ok()
                } else {
                    None
                }
            }
            _ => None,
        };

    // Error handling
    let request_config = match request_config {
        Some(i) => i,
        None => return HttpResponse::InternalServerError().finish(),
    };

    // Validating headers - Checking if the current request headers has a corresponding match in the registered header
    let registered_headers = request_config.get("headers").unwrap();
    for (header_name, header_value) in request.headers().iter() {
        match registered_headers.get(header_name.as_str()) {
            Some(registered_header_value) => {
                if registered_header_value.is_string() {
                    match registered_header_value.as_str().unwrap()
                        == &String::from_utf8(header_value.as_bytes().to_vec()).unwrap()
                    {
                        true => {}
                        false => return HttpResponse::BadRequest().finish(),
                    }
                }
            }
            None => return HttpResponse::NotFound().finish(),
        };
    }

    // Creating the payload to be send as a response
    let response_config: Option<serde_json::Value> =
        match redis_conn.hget(&unique_key, "response_config").await.ok() {
            Some(redis_response) if redis_response != redis::Value::Nil => {
                let redis_response: redis::Value = redis_response;
                if let Some(i) = redis::FromRedisValue::from_redis_value(&redis_response).ok() {
                    let _: String = i;
                    serde_json::from_str::<'_, serde_json::Value>(&i).ok()
                } else {
                    None
                }
            }
            _ => None,
        };

    // Error handling
    let response_config = match response_config {
        Some(i) => i,
        None => return HttpResponse::InternalServerError().finish(),
    };

    // Constructing the HTTP response
    let response_body = response_config
        .get("body")
        .unwrap()
        .as_str()
        .unwrap()
        .to_string();
    let response_headers = serde_json::from_str::<serde_json::Value>(
        response_config.get("body").unwrap().as_str().unwrap(),
    )
    .unwrap();

    let mut response = HttpResponse::Ok();
    for (k, v) in response_headers.as_object().unwrap().iter() {
        let key = k.clone();
        let val = v.as_str().unwrap().to_string();
        response.append_header((key, val));
    }

    response.body(response_body)

}

#[actix_web::main]
async fn main() -> Result<()> {
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
                        .guard(actix_web::guard::Put())
                        .guard(actix_web::guard::Post())
                        .guard(actix_web::guard::Patch())
                        .to(poll),
                ),
            )
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await?;

    Ok(())
}
