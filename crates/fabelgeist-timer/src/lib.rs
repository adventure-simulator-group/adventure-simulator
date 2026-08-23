use std::future::Future;
use std::time::Duration;

pub fn sleep(duration: Duration) -> impl Future<Output = ()> {
    #[cfg(target_arch = "wasm32")]
    return gloo_timers::future::sleep(duration);
    #[cfg(not(target_arch = "wasm32"))]
    return tokio::time::sleep(duration);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[cfg(not(target_arch = "wasm32"))]
    async fn test_sleep() {
        let now = std::time::Instant::now();
        sleep(Duration::from_millis(10)).await;
        assert!(now.elapsed() >= Duration::from_millis(10));
    }
}
