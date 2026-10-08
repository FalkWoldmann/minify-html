use aho_corasick::AhoCorasick;
use aho_corasick::AhoCorasickBuilder;
use std::sync::LazyLock;

pub static TAG_TEXTAREA_END: LazyLock<AhoCorasick> = LazyLock::new(|| {
    AhoCorasickBuilder::new()
        .ascii_case_insensitive(true)
        .build(["</textarea"])
        .unwrap()
});

pub static TAG_TITLE_END: LazyLock<AhoCorasick> = LazyLock::new(|| {
    AhoCorasickBuilder::new()
        .ascii_case_insensitive(true)
        .build(["</title"])
        .unwrap()
});
