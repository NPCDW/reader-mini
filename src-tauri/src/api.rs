//! 服务端接口：书架 / 目录 / 正文 / 进度。
//!
//! 四个接口都返回 `{data, errorMsg, isSuccess}` 的信封，统一在 `unwrap_resp` 里拆。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Book {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub intro: String,
    #[serde(default)]
    pub book_url: String,
    #[serde(default)]
    pub latest_chapter_title: String,
    #[serde(default)]
    pub dur_chapter_title: String,
    #[serde(default)]
    pub dur_chapter_index: i64,
    #[serde(default)]
    pub dur_chapter_pos: i64,
    #[serde(default)]
    pub total_chapter_num: i64,
    #[serde(default)]
    pub word_count: String,
    #[serde(default)]
    pub kind: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Chapter {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub book_url: String,
    #[serde(default)]
    pub base_url: String,
}

/// 交给阅读窗口的一份内容
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadPayload {
    pub book_index: usize,
    pub chapter_index: i64,
    pub chapter_title: String,
    /// 起始行；`usize::MAX` 表示「从末尾读起」（往回翻章时用）
    pub start_line: usize,
    /// 上次读到的正文位置（`book.durChapterPos`），以字数计：
    /// 本章第一屏是 0，第一屏 20 个字则第二屏是 20。换章时为 0（新章从头读起）
    #[serde(default)]
    pub dur_chapter_pos: i64,
    /// 样式版本。改了背景 / 字色 / 字号 / 行高之后，就算正文没变，
    /// 也要让已经开着的阅读窗口重新读一次配置
    #[serde(default)]
    pub style_tick: u64,
    /// 这次是不是「打开 / 换章」：是的话阅读窗口要按 `dur_chapter_pos` 重新定位
    /// 到第几页 —— 哪怕手上这份正文跟刚才那份一模一样（同一本同一章再点一次「阅读」，
    /// 服务端记的位置可能已经被别的客户端改过了）。
    /// 只重刷样式时是 `false`：那一次要把读者留在他正在看的那一页
    #[serde(default)]
    pub reposition: bool,
    pub text: String,
}

#[derive(Deserialize)]
struct Resp<T> {
    data: Option<T>,
    #[serde(default, rename = "errorMsg")]
    error_msg: String,
    #[serde(default, rename = "isSuccess")]
    is_success: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProgressBody<'a> {
    name: &'a str,
    author: &'a str,
    dur_chapter_index: i64,
    dur_chapter_pos: i64,
    dur_chapter_time: i64,
    dur_chapter_title: &'a str,
}

/// 请求头里报的门面：`reader-mini/<版本>`，和 `Cargo.toml` / `tauri.conf.json` 里那个版本同一个数
pub const USER_AGENT: &str = concat!("reader-mini/", env!("CARGO_PKG_VERSION"));

/// 全项目唯一的 HTTP 客户端。
///
/// 接口一共四个，但客户端只建这一个：连接池复用是一回事，更要紧的是
/// `User-Agent` 这种「每个请求都得带上」的东西只在这里设一次，
/// 以后谁新加接口都不会漏。
pub fn client() -> reqwest::Client {
    use std::sync::OnceLock;
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(15))
                .user_agent(USER_AGENT)
                .build()
                .unwrap_or_default()
        })
        .clone()
}

/// 手写百分号编码：只为两个查询串，不值得为它引 reqwest 的 `query` feature
pub fn encode_query(params: &[(&str, &str)]) -> String {
    params
        .iter()
        .map(|(k, v)| format!("{}={}", percent_encode(k), percent_encode(v)))
        .collect::<Vec<_>>()
        .join("&")
}

fn percent_encode(s: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(s.len());
    for b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(*b as char)
            }
            _ => {
                out.push('%');
                out.push(HEX[(b >> 4) as usize] as char);
                out.push(HEX[(b & 0x0f) as usize] as char);
            }
        }
    }
    out
}

fn join(base: &str, path: &str) -> String {
    format!(
        "{}/{}",
        base.trim_end_matches('/'),
        path.trim_start_matches('/')
    )
}

fn unwrap_resp<T>(text: &str) -> anyhow::Result<T>
where
    T: serde::de::DeserializeOwned + Default,
{
    let resp: Resp<T> = serde_json::from_str(text).map_err(|e| {
        anyhow::anyhow!("解析响应失败: {e}; 原文: {}", &text[..text.len().min(200)])
    })?;
    if !resp.is_success {
        anyhow::bail!("接口返回失败: {}", resp.error_msg);
    }
    Ok(resp.data.unwrap_or_default())
}

async fn get_text(url: String) -> anyhow::Result<String> {
    Ok(client().get(url).send().await?.text().await?)
}

pub async fn get_bookshelf(base: &str) -> anyhow::Result<Vec<Book>> {
    let text = get_text(join(base, "getBookshelf")).await?;
    Ok(unwrap_resp::<Vec<Book>>(&text)?
        .into_iter()
        .filter(|b| !b.name.is_empty())
        .collect())
}

pub async fn get_chapter_list(base: &str, book_url: &str) -> anyhow::Result<Vec<Chapter>> {
    let url = join(
        base,
        &format!("getChapterList?{}", encode_query(&[("url", book_url)])),
    );
    let text = get_text(url).await?;
    // 不能过滤空标题：返回的下标就是 `getBookContent` 的 index，缺一个整章错位
    unwrap_resp::<Vec<Chapter>>(&text)
}

pub async fn get_book_content(base: &str, book_url: &str, index: i64) -> anyhow::Result<String> {
    let url = join(
        base,
        &format!(
            "getBookContent?{}",
            encode_query(&[("url", book_url), ("index", &index.to_string())])
        ),
    );
    let text = get_text(url).await?;
    unwrap_resp::<String>(&text)
}

pub async fn save_book_progress(
    base: &str,
    book: &Book,
    index: i64,
    pos: i64,
    title: &str,
) -> anyhow::Result<()> {
    let body = ProgressBody {
        name: &book.name,
        author: &book.author,
        dur_chapter_index: index,
        dur_chapter_pos: pos,
        dur_chapter_time: now_ms(),
        dur_chapter_title: title,
    };
    let text = client()
        .post(join(base, "saveBookProgress"))
        .json(&body)
        .send()
        .await?
        .text()
        .await?;
    let _ = unwrap_resp::<serde_json::Value>(&text)?;
    Ok(())
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_is_percent_encoded() {
        let q = encode_query(&[("url", "https://example.org/book/1.htm"), ("index", "7")]);
        assert_eq!(q, "url=https%3A%2F%2Fexample.org%2Fbook%2F1.htm&index=7");
    }

    #[test]
    fn envelope_is_unwrapped() {
        let r = unwrap_resp::<Vec<Book>>(
            r#"{"data":[{"name":"三体","bookUrl":"x","durChapterIndex":3}],"errorMsg":"","isSuccess":true}"#,
        )
        .unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].name, "三体");
        assert_eq!(r[0].dur_chapter_index, 3);
        assert!(
            unwrap_resp::<Vec<Book>>(r#"{"data":[],"errorMsg":"boom","isSuccess":false}"#).is_err()
        );
    }

    #[test]
    fn base_url_trailing_slash_is_tolerated() {
        assert_eq!(join("http://a/", "/b"), "http://a/b");
    }

    /// 请求上带的 `User-Agent` 是「发出去」那一刻由客户端补的，
    /// 从 `RequestBuilder` 上看不到，所以只能真发一次、看落到线上的那句。
    #[test]
    fn every_request_reports_itself_as_reader_mini() {
        use std::io::{Read, Write};
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut sock, _) = listener.accept().unwrap();
            let mut buf = [0u8; 4096];
            let n = sock.read(&mut buf).unwrap();
            let _ = sock
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}");
            String::from_utf8_lossy(&buf[..n]).to_string()
        });

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let _ = client()
                .get(format!("http://{addr}/getBookshelf"))
                .send()
                .await
                .unwrap();
        });
        let raw = server.join().unwrap();
        let ua = raw
            .lines()
            .find(|l| l.to_ascii_lowercase().starts_with("user-agent:"))
            .unwrap_or_else(|| panic!("请求没带 User-Agent: {raw}"));
        let ua = ua.trim().split_once(":").unwrap().1.trim();
        assert_eq!(ua, USER_AGENT);
        assert_eq!(
            USER_AGENT,
            format!("reader-mini/{}", env!("CARGO_PKG_VERSION"))
        );
    }
}
