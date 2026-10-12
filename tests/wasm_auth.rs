use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

/// Тесты в `wasm-pack test` работают на собственном HTTP-сервере
/// на случайном порту, а не через Vite. Поэтому относительные URL
/// (`/api/auth/login`) резолвятся против тестового origin и не доходят
/// до auth-server. Переключаем клиент на абсолютный адрес.
fn use_direct_server() {
    hello_wasm02::auth::set_api_base("http://127.0.0.1:3000");
}

#[wasm_bindgen_test]
async fn test_make_request_with_bearer() {
    let resp = hello_wasm02::fetch::make_authenticated_request(
        "http://127.0.0.1:3000/api/me",
        "test-token",
    )
    .await
    .unwrap();

    // Без валидного токена сервер вернёт 401 — но заголовок был отправлен.
    assert_eq!(resp.status(), 401);
}

#[wasm_bindgen_test]
async fn test_authorization_header_set() {
    // Косвенная проверка: сервер отвечает 401, а не 400 — значит, заголовок дошёл.
    let resp = hello_wasm02::fetch::make_authenticated_request(
        "http://127.0.0.1:3000/api/me",
        "invalid",
    )
    .await
    .unwrap();
    assert_eq!(resp.status(), 401);
}

#[wasm_bindgen_test]
async fn test_401_handling() {
    let resp = hello_wasm02::fetch::make_authenticated_request(
        "http://127.0.0.1:3000/api/me",
        "",
    )
    .await
    .unwrap();
    assert_eq!(resp.status(), 401);
}

#[wasm_bindgen_test]
async fn test_403_handling() {
    use_direct_server();
    let store = hello_wasm02::auth::TokenStore::new();
    hello_wasm02::auth::login(&store, "user", "secret").await.unwrap();

    let token = store.borrow().access_token.clone().unwrap();
    let resp = hello_wasm02::fetch::make_authenticated_request(
        "http://127.0.0.1:3000/api/admin",
        &token,
    )
    .await
    .unwrap();

    assert_eq!(resp.status(), 403);
}

#[wasm_bindgen_test]
async fn test_token_refresh_flow() {
    use_direct_server();
    let store = hello_wasm02::auth::TokenStore::new();
    hello_wasm02::auth::login(&store, "user", "secret").await.unwrap();

    // Первый запрос — успешен.
    let resp = hello_wasm02::fetch::send_with_auth_retry(
        &store,
        "http://127.0.0.1:3000/api/me",
    )
    .await
    .unwrap();
    assert_eq!(resp.status(), 200);

    // Искусственно портим access-токен, оставляя refresh валидным.
    store.borrow_mut().access_token = Some("broken".into());

    let resp2 = hello_wasm02::fetch::send_with_auth_retry(
        &store,
        "http://127.0.0.1:3000/api/me",
    )
    .await
    .unwrap();
    assert_eq!(resp2.status(), 200, "refresh должен был восстановить доступ");
}

#[wasm_bindgen_test]
async fn test_auth_header_not_duplicated() {
    use_direct_server();
    let store = hello_wasm02::auth::TokenStore::new();
    hello_wasm02::auth::login(&store, "user", "secret").await.unwrap();

    store.borrow_mut().access_token = Some("broken".into());

    // send_with_auth_retry создаёт новый Request — дублирования быть не должно.
    let resp = hello_wasm02::fetch::send_with_auth_retry(
        &store,
        "http://127.0.0.1:3000/api/me",
    )
    .await
    .unwrap();
    assert_eq!(resp.status(), 200);
}