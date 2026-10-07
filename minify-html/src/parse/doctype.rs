use crate::ast::NodeData;
use crate::parse::Code;
use memchr::memchr;
use minify_html_common::gen::codepoints::WHITESPACE;

pub fn parse_doctype(code: &mut Code) -> NodeData {
  debug_assert!(code.as_slice()[..9].eq_ignore_ascii_case(b"<!doctype"));
  code.shift(9);
  code.shift_while_in_lookup(WHITESPACE);
  code.shift_if_next_seq_case_insensitive(b"html");
  code.shift_while_in_lookup(WHITESPACE);
  let data = if code.opts.treat_brace_as_opaque || code.opts.treat_chevron_percent_as_opaque {
    code.slice_and_shift_template_aware_until(b">").to_vec()
  } else {
    let len = memchr(b'>', code.as_slice()).unwrap_or(code.rem());
    code.copy_and_shift(len)
  };
  let ended = code.shift_if_next(b'>');
  NodeData::Doctype {
    legacy: data,
    ended,
  }
}
