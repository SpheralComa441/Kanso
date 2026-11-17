// The sites whose public feeds Kanso can read. Part of `news.rs` (included there).
//
// Provisional list: replaced by the feeds verified to answer.

/// Every source Kanso knows, grouped by language.
pub const NEWS_SOURCES: &[NewsSource] = &[
    NewsSource {
        id: "pcgamer",
        name: "PC Gamer",
        lang: Lang::En,
        topic: NewsTopic::Gaming,
        feed_url: "https://www.pcgamer.com/rss/",
        site_url: "https://www.pcgamer.com/",
        hosts: &["pcgamer.com"],
    },
];
