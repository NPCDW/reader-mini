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

pub async fn get_bookshelf(client: &reqwest::Client, base: &str) -> anyhow::Result<Vec<Book>> {
    let text = client
        .get(join(base, "getBookshelf"))
        .send()
        .await?
        .text()
        .await?;
    Ok(unwrap_resp::<Vec<Book>>(&text)?
        .into_iter()
        .filter(|b| !b.name.is_empty())
        .collect())
}

pub async fn get_chapter_list(
    client: &reqwest::Client,
    base: &str,
    book_url: &str,
) -> anyhow::Result<Vec<Chapter>> {
    let text = client
        .get(join(base, "getChapterList"))
        .query(&[("url", book_url)])
        .send()
        .await?
        .text()
        .await?;
    Ok(unwrap_resp::<Vec<Chapter>>(&text)?
        .into_iter()
        .filter(|c| !c.title.is_empty())
        .collect())
}

pub async fn get_book_content(
    client: &reqwest::Client,
    base: &str,
    book_url: &str,
    index: i64,
) -> anyhow::Result<String> {
    let text = client
        .get(join(base, "getBookContent"))
        .query(&[("url", book_url), ("index", &index.to_string())])
        .send()
        .await?
        .text()
        .await?;
    unwrap_resp::<String>(&text)
}

pub async fn save_book_progress(
    client: &reqwest::Client,
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
    let text = client
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

/// 按"每页字符数"切分正文，并保证翻页时上一页最后一行出现在下一页开头。
///
/// 做法：逐行累加装箱；装不下就翻页，并把"上一页最后一行"当作新页首行。
/// 超长单行会先按宽度硬切成多段再装箱。
pub fn paginate(text: &str, chars_per_page: usize) -> Vec<String> {
    // 先把正文按"物理行"整理成待装箱的片段；超长行硬切成多段，
    // 每段标记 is_continuation，续行不参与"翻页携带末行"。
    struct Seg {
        text: String,
        is_continuation: bool,
    }

    let mut segs: Vec<Seg> = Vec::new();
    for line in text.split('\n').map(|l| l.trim_end()) {
        if line.trim().is_empty() {
            continue;
        }
        if line.chars().count() <= chars_per_page {
            segs.push(Seg {
                text: line.to_string(),
                is_continuation: false,
            });
        } else {
            let chars: Vec<char> = line.chars().collect();
            for (i, chunk) in chars.chunks(chars_per_page).enumerate() {
                segs.push(Seg {
                    text: chunk.iter().collect(),
                    is_continuation: i > 0,
                });
            }
        }
    }
    if segs.is_empty() {
        return vec![String::new()];
    }

    let mut pages: Vec<Vec<String>> = Vec::new();
    let mut cur: Vec<String> = Vec::new();
    let mut cur_len = 0usize;

    for seg in segs {
        let l = seg.text.chars().count();
        if cur_len + l > chars_per_page && !cur.is_empty() {
            // 先结算当前页，再把它最后一行带到新页开头
            let carry = if seg.is_continuation {
                None
            } else {
                cur.last().cloned()
            };
            pages.push(std::mem::take(&mut cur));
            cur_len = 0;
            if let Some(last) = carry {
                cur_len += last.chars().count();
                cur.push(last);
            }
        }
        cur_len += l;
        cur.push(seg.text);
    }
    if !cur.is_empty() {
        pages.push(cur);
    }

    pages.into_iter().map(|p| p.join("\n")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paginate_keeps_last_line_as_next_first() {
        let text = "第一行内容\n第二行内容\n第三行内容\n第四行内容";
        let pages = paginate(text, 12);
        assert!(pages.len() >= 2, "应当被切成多页, got {pages:?}");
        // 第 2 页首行 == 第 1 页末行
        let p1_last = pages[0].lines().last().unwrap().to_string();
        let p2_first = pages[1].lines().next().unwrap().to_string();
        assert_eq!(p1_last, p2_first, "翻页需保留上一页最后一行");
    }

    #[test]
    fn paginate_single_page() {
        let pages = paginate("短文本", 100);
        assert_eq!(pages.len(), 1);
    }

    #[test]
    fn paginate_handles_long_line() {
        let line = "字".repeat(50);
        let pages = paginate(&line, 20);
        assert!(pages.len() >= 3);
        // 每页最多 = chars_per_page + 携带的上一页末行
        for p in &pages {
            assert!(
                p.chars().count() <= 40,
                "页宽超出预期: {}",
                p.chars().count()
            );
        }
    }
}
