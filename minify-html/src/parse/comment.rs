use crate::ast::NodeData;
use crate::parse::Code;
use aho_corasick::AhoCorasick;
use aho_corasick::AhoCorasickBuilder;
use once_cell::sync::Lazy;

static COMMENT_END: Lazy<AhoCorasick> =
  Lazy::new(|| AhoCorasickBuilder::new().build(["-->"]).unwrap());

pub fn parse_comment(code: &mut Code) -> NodeData {
  debug_assert!(code.as_slice().starts_with(b"<!--"));
  code.shift(4);
  let (data, matched) =
    if code.opts.treat_brace_as_opaque || code.opts.treat_chevron_percent_as_opaque {
      let data = code.slice_and_shift_template_aware_until(b"-->").to_vec();
      (data, if code.at_end() { 0 } else { 3 })
    } else {
      let (len, matched) = match COMMENT_END.find(code.as_slice()) {
        Some(m) => (m.start(), m.end() - m.start()),
        None => (code.rem(), 0),
      };
      (code.copy_and_shift(len), matched)
    };
  // It might be EOF.
  code.shift(matched);
  NodeData::Comment {
    code: data,
    ended: matched > 0,
  }
}
