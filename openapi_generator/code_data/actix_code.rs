use actix_web::server;

fn main() {

    server::new(move || {
        App::with_state(AppState { db: addr.clone() })
            .middleware(middleware::Logger::default())
            .resource("/user", |r| {
                r.method(http::Method::POST).with_async(create_user)
            })
            .resource("/token", |r| {
                r.method(http::Method::POST).with_async(get_token)
            })
    })
    .bind("127.0.0.1:8080")
    .unwrap()
    .start();

}
