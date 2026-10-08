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

pub fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .unwrap_or_default()
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
}
