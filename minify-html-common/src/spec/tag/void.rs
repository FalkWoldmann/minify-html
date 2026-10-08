use ahash::AHashSet;
use std::sync::LazyLock;

pub static VOID_TAGS: LazyLock<AHashSet<&'static [u8]>> = LazyLock::new(|| {
    let mut s = AHashSet::<&'static [u8]>::default();
    s.insert(b"area");
    s.insert(b"base");
    s.insert(b"br");
    s.insert(b"col");
    s.insert(b"embed");
    s.insert(b"hr");
    s.insert(b"img");
    s.insert(b"input");
    s.insert(b"keygen");
    s.insert(b"link");
    s.insert(b"meta");
    s.insert(b"param");
    s.insert(b"source");
    s.insert(b"track");
    s.insert(b"wbr");
    s
});
