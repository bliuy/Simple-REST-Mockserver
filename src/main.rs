use std::cell::Cell;

use actix_web::{web::Json, App, Either, HttpResponse, HttpServer, Responder};
use anyhow::Result;
use redis::{aio::Connection, AsyncCommands};
use serde::{Deserialize, Serialize};
mod errors;
use crate::registration::MockServerPayload;
mod registration;

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
    request_body: Json<MockServerPayload>,
    app_data: actix_web::web::Data<AppData>,
) -> impl Responder {
    let payload = request_body.into_inner();

    // Returning 400 Bad Request if validation fails
    if let Err(e) = registration::validate_registration_request(&payload) {
        return HttpResponse::BadRequest().body(format!("{}", e));
    }

    // Checking if the unique_key exists within the redis database
    let mut redis_conn = app_data.redis_conn.take();
    let res: Result<bool, redis::RedisError> = redis_conn.exists("test").await;
    // let exists: Result<bool, redis::RedisError> = app_data
    //     .redis_conn
    //     .exists(&payload.http_request.unique_key)
    //     .await;

    dbg!(payload);

    HttpResponse::Ok().finish()
}

struct AppData {
    redis_conn: Cell<Connection>,
}

#[actix_web::main]
async fn main() -> Result<()> {
    // // Creating the async connection instance to the Redis backend
    // let redis_client = redis::Client::open("redis://127.0.0.1/")?;
    // let redis_conn = redis_client.get_async_connection().await?;

    // let app_data = actix_web::web::Data::new(AppData {
    //     redis_conn: redis_conn,
    // });

    HttpServer::new(move || {
        App::new()
            .app_data(async {
                let redis_client = redis::Client::open("redis://127.0.0.1/").unwrap();
                let redis_conn = redis_client.get_async_connection().await.unwrap();
            
                let app_data = actix_web::web::Data::new(AppData {
                    redis_conn:Cell::new(redis_conn),
                });
                app_data
            })
            .service(hello)
            .service(echo)
            .service(register)
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await?;

    Ok(())
}
