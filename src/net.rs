//! HTTP 客户端装配。
//!
//! reqwest 0.13 默认走 rustls，而 `rustls-no-provider` 要求进程里先装好
//! 一个 CryptoProvider。开发镜像里有 openssl 还能编 C，但更稳的做法是
//! 直接用纯 Rust 的 ring，避免额外 C 依赖。

use std::sync::Once;
use std::time::Duration;

static PROVIDER: Once = Once::new();

/// 确保 rustls 的加密实现已就绪（幂等，可重复调用）
pub fn ensure_crypto_provider() {
    PROVIDER.call_once(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

/// 构建应用统一的 HTTP 客户端
pub fn client() -> anyhow::Result<reqwest::Client> {
    ensure_crypto_provider();
    Ok(reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()?)
}
