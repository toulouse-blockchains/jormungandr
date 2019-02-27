use actix_web::middleware::cors::Cors;
use actix_web::server::{HttpHandler, HttpHandlerTask};
use actix_web::{http::{self, header}, pred, App, HttpResponse};
use rest::server_service::{ServerResult, ServerService};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub struct ServerServiceBuilder {
    pkcs12: PathBuf,
    address: SocketAddr,
    prefix: Arc<String>,
    handlers: Vec<Box<Fn() -> Box<HttpHandler<Task = Box<HttpHandlerTask>>> + Send + Sync>>,
}

impl ServerServiceBuilder {
    pub fn new(pkcs12: impl AsRef<Path>, address: SocketAddr, prefix: impl Into<String>) -> Self {
        Self {
            pkcs12: pkcs12.as_ref().to_path_buf(),
            address,
            prefix: Arc::new(prefix.into()),
            handlers: vec![],
        }
        .add_handler(|| {
            Cors::for_app(App::new().filter(pred::Options()))
                .send_wildcard()
                .allowed_header(header::CONTENT_TYPE)
                // catch all resourse in needed for the cors application
                // that doesn't work with zero resources.
                .resource("{path:.*}", |_| HttpResponse::Ok())
                .register()
        })
    }

    pub fn add_handler<F, S: 'static>(mut self, handler: F) -> Self
    where
        F: Fn() -> App<S> + Send + Sync + Clone + 'static,
    {
        let prefix = self.prefix.clone();
        let prefixed_handler = move ||
            handler().middleware(Cors::default()).prefix(&**prefix).boxed();
        self.handlers.push(Box::new(prefixed_handler));
        self
    }

    pub fn build(self) -> ServerResult<ServerService> {
        let handlers = Arc::new(self.handlers);
        let multi_handler = move || handlers.iter().map(|handler| handler()).collect::<Vec<_>>();
        ServerService::start(self.pkcs12, self.address, multi_handler)
    }
}
