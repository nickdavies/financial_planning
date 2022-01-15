use std::path::{Path, PathBuf};

use actix_multipart::Multipart;
use actix_web::{get, post, App, HttpResponse, HttpServer, Responder};
use actix_web_httpauth::middleware::HttpAuthentication;
use anyhow::{Context, Result};

use financial_planning_lib::input::{read_configs};

use crate::auth::AuthProvider;

mod auth;
mod run_model;

#[get("/")]
async fn home() -> impl Responder {
    HttpResponse::Found().header("Location", "/static/model.html").finish()
}

#[post("/run_model")]
async fn run(payload: Multipart) -> Result<HttpResponse, actix_web::Error> {
    let files = match run_model::extract_files(payload).await {
        Ok(files) => files,
        Err(e) => {
            return Ok(HttpResponse::BadRequest().body(format!("Failed to read provided files: {:#?}", e)));
        }
    };

    let config = match read_configs(Path::new("./plan.toml"), run_model::MapFileLoader::new(files)) {
        Ok(config) => config,
        Err(e) => {
            return Ok(HttpResponse::BadRequest().body(format!("Failed to build model config: {:#?}", e)));
        }
    };
    let output = match run_model::run(config) {
        Ok(output) => output,
        Err(e) => {
            return Ok(HttpResponse::InternalServerError().body(format!("Failed to execute model: {:#?}", e)));
        }
    };
    let req_body = format!("Model output:\n{:#?}", output);
    println!("{}", req_body);
    Ok(HttpResponse::Ok().body(req_body).into())
}

#[actix_web::main]
pub async fn run_server(port: u16, auth_file: PathBuf, static_dir: PathBuf) -> Result<()> {
    let auth_provider =
        AuthProvider::new_from_file(&auth_file).context("failed to build auth provider")?;
    HttpServer::new(move|| {
        let auth_provider = auth_provider.clone();
        let auth = HttpAuthentication::basic(move |r, c| {
            let auth_provider = auth_provider.clone();
            async move {auth_provider.validate_request(r, c)}
        });
        App::new()
            .service(actix_files::Files::new("/static", static_dir.clone()))
            .wrap(auth)
            .service(home)
            .service(run)
    })
    .bind(format!("0.0.0.0:{}", port))?
    .run()
    .await?;

    Ok(())
}
