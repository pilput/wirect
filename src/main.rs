use http_body_util::Full;
use hyper::{
    Method, Request, Response, StatusCode,
    body::{Bytes, Incoming},
    header::LOCATION,
    server::conn::http1,
    service::service_fn,
};
use hyper_util::rt::TokioIo;
use std::{convert::Infallible, sync::Arc};
use tokio::net::TcpListener;

fn redirect_location(base: &str, uri: &hyper::Uri) -> String {
    let path = uri.path();

    match uri.query() {
        Some(query) if !query.is_empty() => format!("{base}{path}?{query}"),
        _ => format!("{base}{path}"),
    }
}

fn respond(status: StatusCode, body: &'static str) -> Response<Full<Bytes>> {
    let mut response = Response::new(Full::new(Bytes::from_static(body.as_bytes())));
    *response.status_mut() = status;
    response
}

async fn handle(
    base_url: Arc<str>,
    req: Request<Incoming>,
) -> Result<Response<Full<Bytes>>, Infallible> {
    if req.uri().path() == "/health" && matches!(*req.method(), Method::GET | Method::HEAD) {
        return Ok(respond(StatusCode::OK, "OK"));
    }

    let location = redirect_location(&base_url, req.uri());
    let mut response = respond(StatusCode::PERMANENT_REDIRECT, "");

    match location.parse() {
        Ok(value) => {
            response.headers_mut().insert(LOCATION, value);
            Ok(response)
        }
        Err(_) => Ok(respond(StatusCode::BAD_REQUEST, "invalid request URI")),
    }
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    let new_domain = std::env::var("NEW_DOMAIN")
        .expect("NEW_DOMAIN must be set in .env or environment");
    let base_url: Arc<str> = Arc::from(new_domain.trim_end_matches('/'));

    let host = std::env::var("HOST").unwrap_or_else(|_| "0.0.0.0".into());
    let port = std::env::var("PORT").unwrap_or_else(|_| "8080".into());
    let bind_addr = format!("{host}:{port}");

    println!("Starting redirect server on http://{bind_addr}");
    println!("Redirecting all incoming requests to {base_url}");

    let listener = TcpListener::bind(&bind_addr)
        .await
        .expect("failed to bind TCP listener");

    loop {
        let stream = match listener.accept().await {
            Ok((stream, _)) => stream,
            Err(err) => {
                eprintln!("failed to accept connection: {err}");
                continue;
            }
        };

        let base_url = Arc::clone(&base_url);
        tokio::spawn(async move {
            let service = service_fn(move |req| handle(Arc::clone(&base_url), req));
            if let Err(err) = http1::Builder::new()
                .serve_connection(TokioIo::new(stream), service)
                .await
            {
                eprintln!("connection error: {err}");
            }
        });
    }
}
