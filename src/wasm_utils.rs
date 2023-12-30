#[cfg(target_arch = "wasm32")]
// Yield to Javascript
pub async fn yield_() {
    sleep_ms(0).await;
}

#[cfg(target_arch = "wasm32")]
// Hack to get async sleep on wasm
pub async fn sleep_ms(millis: i32) {
    let mut cb = |resolve: js_sys::Function, _reject: js_sys::Function| {
        web_sys::window()
            .unwrap()
            .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, millis)
            .expect("Failed to call set_timeout");
    };
    let p = js_sys::Promise::new(&mut cb);
    wasm_bindgen_futures::JsFuture::from(p).await.unwrap();
}