use crate::cfg::Cfg;
use minify_html_common::gen::codepoints::Lookup;
use minify_html_common::gen::codepoints::WHITESPACE;

pub mod bang;
pub mod comment;
pub mod content;
pub mod doctype;
pub mod element;
pub mod instruction;
pub mod script;
pub mod style;
#[cfg(test)]
mod tests;
pub mod textarea;
pub mod title;

#[derive(Default, Clone, Debug)]
pub struct ParseOpts {
  pub treat_brace_as_opaque: bool,
  pub treat_chevron_percent_as_opaque: bool,
  pub treat_esi_tags_as_self_closable: bool,
}

impl From<&Cfg> for ParseOpts {
  fn from(cfg: &Cfg) -> Self {
    ParseOpts {
      treat_brace_as_opaque: cfg.preserve_brace_template_syntax,
      treat_chevron_percent_as_opaque: cfg.preserve_chevron_percent_template_syntax,
      treat_esi_tags_as_self_closable: cfg.preserve_esi_tags,
    }
  }
}

impl ParseOpts {
  pub fn contains_template_syntax(&self, source: &[u8]) -> bool {
    source.windows(2).any(|seq| {
      (self.treat_brace_as_opaque && matches!(seq, b"{{" | b"{%" | b"{#"))
        || (self.treat_chevron_percent_as_opaque && seq == b"<%")
    })
  }

  // True if `source` has a directive that can render different literal markup on different
  // paths, e.g. close a `<pre>` in only one branch. Expressions, comments and directives that
  // render their body exactly once can't.
  pub fn contains_branching_directive(&self, source: &[u8]) -> bool {
    (0..source.len()).any(|i| {
      let rest = &source[i..];
      (self.treat_brace_as_opaque && rest.starts_with(b"{%") && !is_linear_brace_directive(rest))
        || (self.treat_chevron_percent_as_opaque
          && rest.starts_with(b"<%")
          && !matches!(rest.get(2), Some(b'=' | b'#' | b'@'))
          && !rest.starts_with(b"<%--"))
    })
  }
}

// Directives that define or render their body exactly once, so markup around them can't differ
// between render paths.
fn is_linear_brace_directive(source: &[u8]) -> bool {
  let body = source[2..].trim_ascii_start();
  let body = strip_whitespace_control(body).trim_ascii_start();
  let len = body
    .iter()
    .take_while(|c| c.is_ascii_alphanumeric() || **c == b'_')
    .count();
  matches!(
    &body[..len],
    b"block"
      | b"endblock"
      | b"call"
      | b"include"
      | b"import"
      | b"extends"
      | b"let"
      | b"set"
      | b"filter"
      | b"endfilter"
  )
}

fn brace_directive_len(source: &[u8], name: &[u8]) -> Option<usize> {
  let body = source.strip_prefix(b"{%")?.trim_ascii_start();
  let body = strip_whitespace_control(body).trim_ascii_start();
  let body = body.strip_prefix(name)?.trim_ascii_start();
  let body = strip_whitespace_control(body).trim_ascii_start();
  let rest = body.strip_prefix(b"%}")?;
  Some(source.len() - rest.len())
}

fn strip_whitespace_control(source: &[u8]) -> &[u8] {
  if matches!(source.first(), Some(b'-' | b'+' | b'~')) {
    &source[1..]
  } else {
    source
  }
}

pub struct Code<'c> {
  code: &'c [u8],
  next: usize,
  pub(crate) opts: ParseOpts,

  pub seen_html_open: bool,
  pub seen_head_open: bool,
  pub seen_head_close: bool,
  pub seen_body_open: bool,
}

#[derive(Copy, Clone)]
pub struct Checkpoint(usize);

impl<'c> Code<'c> {
  pub fn new_with_opts(code: &[u8], opts: ParseOpts) -> Code<'_> {
    Code {
      code,
      next: 0,
      opts,
      seen_html_open: false,
      seen_head_open: false,
      seen_head_close: false,
      seen_body_open: false,
    }
  }

  pub fn new(code: &[u8]) -> Code<'_> {
    Code::new_with_opts(code, ParseOpts::default())
  }

  pub fn as_slice(&self) -> &[u8] {
    &self.code[self.next..]
  }

  pub fn take_checkpoint(&self) -> Checkpoint {
    Checkpoint(self.next)
  }

  pub fn restore_checkpoint(&mut self, cp: Checkpoint) {
    self.next = cp.0;
  }

  pub fn slice_since(&self, cp: Checkpoint) -> &[u8] {
    &self.code[cp.0..self.next]
  }

  pub fn at_end(&self) -> bool {
    debug_assert!(self.next <= self.code.len());
    self.next == self.code.len()
  }

  pub fn shift_if_next(&mut self, c: u8) -> bool {
    if self.code.get(self.next).filter(|&&n| n == c).is_some() {
      self.next += 1;
      true
    } else {
      false
    }
  }

  pub fn shift_if_next_seq_case_insensitive(&mut self, seq: &[u8]) -> bool {
    if self
      .code
      .get(self.next..self.next + seq.len())
      .filter(|n| n.eq_ignore_ascii_case(seq))
      .is_some()
    {
      self.next += seq.len();
      true
    } else {
      false
    }
  }

  pub fn shift_if_next_in_lookup(&mut self, lookup: &'static Lookup) -> Option<u8> {
    let c = self.code.get(self.next).filter(|&&n| lookup[n]).copied();
    if c.is_some() {
      self.next += 1;
    };
    c
  }

  pub fn shift_if_next_not_in_lookup(&mut self, lookup: &'static Lookup) -> Option<u8> {
    let c = self.code.get(self.next).filter(|&&n| !lookup[n]).copied();
    if c.is_some() {
      self.next += 1;
    };
    c
  }

  pub fn shift(&mut self, n: usize) {
    self.next += n;
  }

  pub fn slice_and_shift(&mut self, n: usize) -> &[u8] {
    let str = &self.code[self.next..self.next + n];
    self.next += n;
    str
  }

  pub fn copy_and_shift(&mut self, n: usize) -> Vec<u8> {
    self.slice_and_shift(n).to_vec()
  }

  pub fn copy_and_shift_while_in_lookup(&mut self, lookup: &'static Lookup) -> Vec<u8> {
    let mut len = 0;
    loop {
      match self.code.get(self.next + len) {
        Some(&c) if lookup[c] => len += 1,
        _ => break,
      };
    }
    self.copy_and_shift(len)
  }

  pub fn slice_and_shift_while_not_in_lookup(&mut self, lookup: &'static Lookup) -> &[u8] {
    let mut len = 0;
    loop {
      match self.code.get(self.next + len) {
        Some(&c) if !lookup[c] => len += 1,
        _ => break,
      };
    }
    self.slice_and_shift(len)
  }

  // Returns the last character matched.
  pub fn shift_while_in_lookup(&mut self, lookup: &'static Lookup) -> Option<u8> {
    let mut last: Option<u8> = None;
    loop {
      match self.code.get(self.next) {
        Some(&c) if lookup[c] => {
          self.next += 1;
          last = Some(c);
        }
        _ => break,
      };
    }
    last
  }

  pub fn slice_and_shift_while_not_seq_case_insensitive(&mut self, seq: &[u8]) -> &[u8] {
    let mut len = 0;
    let mut quote = None;
    let mut braces = 0usize;
    let mut comment_depth = 1usize;
    let mut found = false;
    while len < self.rem() {
      let remaining = &self.as_slice()[len..];
      if seq == b"#}" && remaining.starts_with(b"{#") {
        comment_depth += 1;
        len += 2;
        continue;
      }
      if quote.is_none()
        && braces == 0
        && remaining
          .get(..seq.len())
          .is_some_and(|candidate| candidate.eq_ignore_ascii_case(seq))
      {
        len += seq.len();
        if seq == b"#}" {
          comment_depth -= 1;
          if comment_depth != 0 {
            continue;
          }
        }
        found = true;
        break;
      }
      let c = remaining[0];
      if let Some(delim) = quote {
        if c == b'\\' {
          len += remaining.len().min(2);
          continue;
        }
        if c == delim {
          quote = None;
        }
      } else if seq != b"#}" {
        match c {
          b'\'' | b'"' => quote = Some(c),
          b'{' if seq == b"}}" => braces += 1,
          b'}' if seq == b"}}" && braces != 0 => braces -= 1,
          _ => {}
        }
      }
      len += 1;
    }
    if !found && seq != b"#}" {
      // An apostrophe in a comment or a Rust lifetime has no closing quote. Rather than letting
      // the token run to EOF, fall back to the first closing delimiter.
      if let Some(end) = memchr::memmem::find(self.as_slice(), seq) {
        len = end + seq.len();
      }
    }
    self.slice_and_shift(len)
  }

  pub fn shift_template(&mut self) -> bool {
    let closing = match self.as_slice() {
      [b'{', b'{', ..] if self.opts.treat_brace_as_opaque => b"}}",
      [b'{', b'%', ..] if self.opts.treat_brace_as_opaque => b"%}",
      [b'{', b'#', ..] if self.opts.treat_brace_as_opaque => b"#}",
      [b'<', b'%', ..] if self.opts.treat_chevron_percent_as_opaque => b"%>",
      _ => return false,
    };
    let start = self.take_checkpoint();
    self.shift(2);
    self.slice_and_shift_while_not_seq_case_insensitive(closing);
    if closing == b"%}"
      && brace_directive_len(self.slice_since(start), b"raw") == Some(self.slice_since(start).len())
    {
      // The raw body is literal template source: only its end marker has syntax.
      while let Some(offset) = memchr::memmem::find(self.as_slice(), b"{%") {
        self.shift(offset);
        if let Some(len) = brace_directive_len(self.as_slice(), b"endraw") {
          self.shift(len);
          return true;
        }
        // Do not scan quotes or directives inside the literal raw body.
        self.shift(2);
      }
      self.shift(self.rem());
    }
    true
  }

  pub fn slice_and_shift_attribute_value(&mut self, lookup: &'static Lookup) -> &[u8] {
    if !self.opts.treat_brace_as_opaque && !self.opts.treat_chevron_percent_as_opaque {
      return self.slice_and_shift_while_not_in_lookup(lookup);
    }
    let start = self.take_checkpoint();
    while !self.at_end() {
      if self.shift_template() {
        continue;
      }
      if lookup[self.as_slice()[0]] {
        break;
      }
      self.shift(1);
    }
    self.slice_since(start)
  }

  pub fn slice_and_shift_template_aware_until(&mut self, seq: &[u8]) -> &[u8] {
    let start = self.take_checkpoint();
    while !self.at_end() && !self.as_slice().starts_with(seq) {
      if !self.shift_template() {
        self.shift(1);
      }
    }
    self.slice_since(start)
  }

  // HTML rawtext/RCDATA ends at its real closing tag, not at an end-tag string
  // inside a template expression. The body remains byte-for-byte opaque.
  pub fn slice_and_shift_special_content(&mut self, tag_name: &[u8]) -> &[u8] {
    let start = self.take_checkpoint();
    while !self.at_end() {
      if self.shift_template() {
        continue;
      }
      let remaining = self.as_slice();
      if remaining.starts_with(b"</")
        && remaining
          .get(2..2 + tag_name.len())
          .is_some_and(|name| name.eq_ignore_ascii_case(tag_name))
        && remaining
          .get(2 + tag_name.len())
          .is_none_or(|c| WHITESPACE[*c] || matches!(c, b'>' | b'/'))
      {
        break;
      }
      self.shift(1);
    }
    self.slice_since(start)
  }

  pub fn rem(&self) -> usize {
    self.code.len() - self.next
  }
}
