#[cfg(not(target_arch = "wasm32"))]
fn main() {
    rhythm::run(rhythm::native::NativePlatform);
}

#[cfg(target_arch = "wasm32")]
fn main() {
    rhythm::web::boot();
}
