use crate::ast::ElementClosingTag;
use crate::ast::NodeData;
use crate::ast::ScriptOrStyleLang;
use crate::entity::decode::decode_entities;
use crate::parse::bang::parse_bang;
use crate::parse::brace_directive_len;
use crate::parse::comment::parse_comment;
use crate::parse::content::ContentType::*;
use crate::parse::doctype::parse_doctype;
use crate::parse::element::is_esi_tag;
use crate::parse::element::parse_element;
use crate::parse::element::parse_tag;
use crate::parse::element::peek_tag_name;
use crate::parse::element::script_lang;
use crate::parse::instruction::parse_instruction;
use crate::parse::Code;
use aho_corasick::AhoCorasick;
use aho_corasick::AhoCorasickBuilder;
use aho_corasick::MatchKind;
use minify_html_common::gen::codepoints::TAG_NAME_CHAR;
use minify_html_common::spec::tag::ns::Namespace;
use minify_html_common::spec::tag::omission::can_omit_as_before;
use minify_html_common::spec::tag::omission::can_omit_as_last_node;
use minify_html_common::spec::tag::void::VOID_TAGS;
use minify_html_common::spec::tag::whitespace::get_whitespace_minification_for_tag;
use minify_html_common::whitespace::collapse_whitespace;
use minify_html_common::whitespace::is_all_whitespace;
use minify_html_common::whitespace::left_trim;
use minify_html_common::whitespace::right_trim;
use once_cell::sync::Lazy;

#[derive(Copy, Clone, Eq, PartialEq)]
enum ContentType {
  Bang,
  ClosingTag,
  Comment,
  Doctype,
  IgnoredTag,
  Instruction,
  MalformedLeftChevronSlash,
  OmittedClosingTag,
  OpeningTag,
  Text,
  // Pebble, Mustache, Django, Go, Jinja, Twix, Nunjucks, Handlebars, Liquid.
  OpaqueBraceBrace,
  OpaqueBraceHash,
  OpaqueBracePercent,
  // Sailfish, JSP, EJS, ERB.
  OpaqueChevronPercent,
}

fn maybe_ignore_html_head_body(
  code: &mut Code,
  typ: ContentType,
  parent: &[u8],
  name: &[u8],
) -> ContentType {
  match (typ, name, parent) {
    (OpeningTag, b"html", _) => {
      if code.seen_html_open {
        IgnoredTag
      } else {
        code.seen_html_open = true;
        typ
      }
    }
    (OpeningTag, b"head", _) => {
      if code.seen_head_open {
        IgnoredTag
      } else {
        code.seen_head_open = true;
        typ
      }
    }
    (ClosingTag, b"head", _) => {
      if code.seen_head_close {
        IgnoredTag
      } else {
        code.seen_head_close = true;
        typ
      }
    }
    (OmittedClosingTag, _, b"head") => {
      code.seen_head_close = true;
      typ
    }
    (OpeningTag, b"body", _) => {
      if code.seen_body_open {
        IgnoredTag
      } else {
        code.seen_body_open = true;
        typ
      }
    }
    _ => typ,
  }
}

fn build_content_type_matcher(
  with_opaque_brace: bool,
  with_opaque_chevron_percent: bool,
) -> (AhoCorasick, Vec<ContentType>) {
  let mut patterns = Vec::<Vec<u8>>::new();
  let mut types = Vec::<ContentType>::new();

  // Only when the character after a `<` is TAG_NAME_CHAR is the `<` is an opening tag.
  // Otherwise, the `<` is interpreted literally as part of text.
  for c in 0u8..128u8 {
    if TAG_NAME_CHAR[c] {
      patterns.push(vec![b'<', c]);
      types.push(ContentType::OpeningTag);
    };
  }

  patterns.push(b"</".to_vec());
  types.push(ContentType::ClosingTag);

  patterns.push(b"<?".to_vec());
  types.push(ContentType::Instruction);

  patterns.push(b"<!doctype".to_vec());
  types.push(ContentType::Doctype);

  patterns.push(b"<!".to_vec());
  types.push(ContentType::Bang);

  patterns.push(b"<!--".to_vec());
  types.push(ContentType::Comment);

  if with_opaque_brace {
    patterns.push(b"{{".to_vec());
    types.push(ContentType::OpaqueBraceBrace);

    patterns.push(b"{#".to_vec());
    types.push(ContentType::OpaqueBraceHash);

    patterns.push(b"{%".to_vec());
    types.push(ContentType::OpaqueBracePercent);
  };

  if with_opaque_chevron_percent {
    patterns.push(b"<%".to_vec());
    types.push(ContentType::OpaqueChevronPercent);
  };

  (
    AhoCorasickBuilder::new()
      .ascii_case_insensitive(true)
      .match_kind(MatchKind::LeftmostLongest)
      // Keep in sync with order of CONTENT_TYPE_FROM_PATTERN.
      .build(patterns)
      .unwrap(),
    types,
  )
}

static CONTENT_TYPE_MATCHER: Lazy<(AhoCorasick, Vec<ContentType>)> =
  Lazy::new(|| build_content_type_matcher(false, false));
static CONTENT_TYPE_MATCHER_OPAQUE_BRACE: Lazy<(AhoCorasick, Vec<ContentType>)> =
  Lazy::new(|| build_content_type_matcher(true, false));
static CONTENT_TYPE_MATCHER_OPAQUE_CP: Lazy<(AhoCorasick, Vec<ContentType>)> =
  Lazy::new(|| build_content_type_matcher(false, true));
static CONTENT_TYPE_MATCHER_OPAQUE_BRACE_CP: Lazy<(AhoCorasick, Vec<ContentType>)> =
  Lazy::new(|| build_content_type_matcher(true, true));

fn content_type_matcher(code: &Code) -> &'static (AhoCorasick, Vec<ContentType>) {
  match (
    code.opts.treat_brace_as_opaque,
    code.opts.treat_chevron_percent_as_opaque,
  ) {
    (false, false) => &CONTENT_TYPE_MATCHER,
    (true, false) => &CONTENT_TYPE_MATCHER_OPAQUE_BRACE,
    (false, true) => &CONTENT_TYPE_MATCHER_OPAQUE_CP,
    (true, true) => &CONTENT_TYPE_MATCHER_OPAQUE_BRACE_CP,
  }
}

pub struct ParsedContent {
  pub children: Vec<NodeData>,
  pub closing_tag_omitted: bool,
}

fn literal_tag_name(source: &[u8]) -> Option<&[u8]> {
  let source = source.strip_prefix(b"<")?;
  let source = source.strip_prefix(b"/").unwrap_or(source);
  let len = source.iter().take_while(|&&c| TAG_NAME_CHAR[c]).count();
  (len != 0).then_some(&source[..len])
}

// Elements that are block-level or hidden by default, so whitespace between two of them never
// renders. This is narrower than the layout whitespace rule for element content, which also covers
// inline elements like `select` and `picture`.
fn is_block_level(name: &[u8]) -> bool {
  matches!(
    name.to_ascii_lowercase().as_slice(),
    b"address"
      | b"article"
      | b"aside"
      | b"base"
      | b"blockquote"
      | b"body"
      | b"caption"
      | b"col"
      | b"colgroup"
      | b"datalist"
      | b"dd"
      | b"details"
      | b"dialog"
      | b"div"
      | b"dl"
      | b"dt"
      | b"fieldset"
      | b"figcaption"
      | b"figure"
      | b"footer"
      | b"form"
      | b"h1"
      | b"h2"
      | b"h3"
      | b"h4"
      | b"h5"
      | b"h6"
      | b"head"
      | b"header"
      | b"hgroup"
      | b"hr"
      | b"html"
      | b"li"
      | b"link"
      | b"main"
      | b"menu"
      | b"meta"
      | b"nav"
      | b"ol"
      | b"optgroup"
      | b"option"
      | b"p"
      | b"script"
      | b"section"
      | b"style"
      | b"summary"
      | b"table"
      | b"tbody"
      | b"td"
      | b"template"
      | b"tfoot"
      | b"th"
      | b"thead"
      | b"title"
      | b"tr"
      | b"ul"
  )
}

// Literal markup inside a raw block is output as is, so it can open an element whose content
// must not be compacted, or leave a start tag unclosed.
fn opens_any_tag(source: &[u8], names: &[&[u8]]) -> bool {
  source.split(|&c| c == b'<').skip(1).any(|rest| {
    let len = rest.iter().take_while(|&&c| TAG_NAME_CHAR[c]).count();
    names
      .iter()
      .any(|name| rest[..len].eq_ignore_ascii_case(name))
  })
}

fn leaves_tag_open(source: &[u8]) -> bool {
  memchr::memrchr(b'<', source).is_some_and(|i| memchr::memchr(b'>', &source[i..]).is_none())
}

const WHITESPACE_SENSITIVE_TAGS: &[&[u8]] = &[
  b"code",
  b"iframe",
  b"listing",
  b"noembed",
  b"noframes",
  b"noscript",
  b"plaintext",
  b"pre",
  b"script",
  b"style",
  b"textarea",
  b"title",
  b"xmp",
];

fn compact_template_text(value: &mut Vec<u8>, previous: Option<&NodeData>, next: &[u8]) {
  let next_name = literal_tag_name(next);
  let previous_name = match previous {
    Some(NodeData::Element { name, .. }) => Some(name.as_slice()),
    Some(NodeData::Opaque { raw_source }) if raw_source.starts_with(b"</") => {
      literal_tag_name(raw_source)
    }
    _ => None,
  };
  // Apply container trimming only to an entirely literal, directly bounded
  // text body. Never infer parentage across directives or sibling elements.
  if let Some(NodeData::Element {
    name,
    closing_tag: ElementClosingTag::Omitted,
    ..
  }) = previous
  {
    if next.starts_with(b"</") && next_name.is_some_and(|next| next.eq_ignore_ascii_case(name)) {
      let rule = get_whitespace_minification_for_tag(Namespace::Html, name, false);
      if rule.trim {
        left_trim(value);
        right_trim(value);
      }
    }
  }
  // A separator between inline or unknown/custom elements can be visible.
  // Remove whitespace-only runs only between two known layout boundaries.
  if is_all_whitespace(value)
    && previous_name.is_some_and(is_block_level)
    && next_name.is_some_and(is_block_level)
  {
    value.clear();
  } else {
    collapse_whitespace(value);
  }
}

// A source template is not an HTML tree: mutually exclusive branches can open
// and close different elements. Keep token order, and compact literal text only
// in normal HTML contexts without inferring a tree across template boundaries.
pub fn parse_template_content(code: &mut Code) -> ParsedContent {
  let mut nodes = Vec::new();
  let matcher = content_type_matcher(code);
  let mut foreign_depth = 0usize;
  let mut uncertain_foreign = false;
  let mut pre_depth = 0usize;
  let mut code_depth = 0usize;
  let mut uncertain_whitespace_sensitive = false;
  while !code.at_end() {
    let (text_len, typ) = match matcher.0.find(code.as_slice()) {
      Some(m) => (m.start(), matcher.1[m.pattern()]),
      None => (code.rem(), Text),
    };
    if text_len != 0 {
      let mut raw_source = code.copy_and_shift(text_len);
      if foreign_depth == 0
        && !uncertain_foreign
        && pre_depth == 0
        && code_depth == 0
        && !uncertain_whitespace_sensitive
      {
        // Entities remain source bytes; only HTML whitespace is compacted.
        compact_template_text(&mut raw_source, nodes.last(), code.as_slice());
      }
      // `<{{ tag }}` is a start tag whose name the tokenizer can't see. Its quoted values follow
      // as text, and it can open a `<pre>` or `<script>`, so stop compacting from here on.
      if raw_source.ends_with(b"<")
        && matches!(
          typ,
          OpaqueBraceBrace | OpaqueBraceHash | OpaqueBracePercent | OpaqueChevronPercent
        )
      {
        uncertain_whitespace_sensitive = true;
      }
      if !raw_source.is_empty() {
        nodes.push(NodeData::Opaque { raw_source });
      }
    }
    match typ {
      Text => break,
      OpeningTag => {
        let start = code.take_checkpoint();
        let mut tag = parse_tag(code);
        let foreign_root = matches!(tag.name.as_slice(), b"svg" | b"math");
        let foreign = foreign_depth != 0 || uncertain_foreign || foreign_root;
        if foreign
          && code
            .opts
            .contains_branching_directive(code.slice_since(start))
        {
          // A directive in foreign markup can change the namespace in later
          // branches. Retain subsequent headers instead of guessing a context.
          uncertain_foreign = true;
        }
        if foreign
          || (matches!(tag.name.as_slice(), b"html" | b"head") && tag.raw_opening_tag.is_none())
        {
          tag.raw_opening_tag = Some(code.slice_since(start).to_vec());
        }
        if foreign_root && !tag.self_closing {
          foreign_depth += 1;
        }
        let special_content = matches!(
          tag.name.as_slice(),
          b"title"
            | b"textarea"
            | b"script"
            | b"style"
            | b"xmp"
            | b"iframe"
            | b"noembed"
            | b"noframes"
            | b"noscript"
        );
        let plaintext = tag.name == b"plaintext";
        let closing_tag = if tag.self_closing
          && (foreign || (code.opts.treat_esi_tags_as_self_closable && is_esi_tag(&tag.name)))
        {
          ElementClosingTag::SelfClosing
        } else if VOID_TAGS.contains(tag.name.as_slice()) {
          ElementClosingTag::Void
        } else {
          ElementClosingTag::Omitted
        };
        if closing_tag == ElementClosingTag::Omitted {
          match tag.name.as_slice() {
            b"pre" => pre_depth += 1,
            b"code" => code_depth += 1,
            _ => {}
          }
        }
        let body = if special_content && closing_tag == ElementClosingTag::Omitted {
          Some(code.slice_and_shift_special_content(&tag.name).to_vec())
        } else if plaintext {
          Some(code.copy_and_shift(code.rem()))
        } else {
          None
        };
        // A directive can select a different literal rawtext/RCDATA closing
        // tag. Later source may still be raw content on another branch, so it
        // must not be interpreted as normal HTML tokens.
        let uncertain_special_content = special_content
          && body
            .as_ref()
            .is_some_and(|body| code.opts.contains_branching_directive(body));
        // A literal script or style body under a literal start tag is minified as usual. A
        // templated start tag can change the script type, so its body stays as is.
        let script_or_style_lang = match tag.name.as_slice() {
          _ if tag.raw_opening_tag.is_some() => None,
          _ if body
            .as_ref()
            .is_some_and(|body| code.opts.contains_template_syntax(body)) =>
          {
            None
          }
          b"script" => Some(script_lang(&tag.attributes)),
          b"style" => Some(ScriptOrStyleLang::CSS),
          _ => None,
        };
        nodes.push(NodeData::Element {
          attributes: tag.attributes,
          raw_opening_tag: tag.raw_opening_tag,
          children: Vec::new(),
          closing_tag,
          name: tag.name,
          namespace: Namespace::Html,
          next_sibling_element_name: Vec::new(),
        });
        match (body, script_or_style_lang) {
          (Some(code), Some(lang)) => nodes.push(NodeData::ScriptOrStyleContent { code, lang }),
          (Some(raw_source), None) => nodes.push(NodeData::Opaque { raw_source }),
          (None, _) => {}
        }
        if uncertain_special_content {
          nodes.push(NodeData::Opaque {
            raw_source: code.copy_and_shift(code.rem()),
          });
        }
      }
      ClosingTag => {
        let start = code.take_checkpoint();
        let tag = parse_tag(code);
        if matches!(tag.name.as_slice(), b"svg" | b"math") {
          foreign_depth = foreign_depth.saturating_sub(1);
        }
        match tag.name.as_slice() {
          b"pre" => pre_depth = pre_depth.saturating_sub(1),
          b"code" => code_depth = code_depth.saturating_sub(1),
          _ => {}
        }
        nodes.push(NodeData::Opaque {
          raw_source: code.slice_since(start).to_vec(),
        });
      }
      kind @ (Instruction | Bang | Doctype) => {
        let start = code.take_checkpoint();
        let node = match kind {
          Instruction => parse_instruction(code),
          Bang => parse_bang(code),
          Doctype => parse_doctype(code),
          _ => unreachable!(),
        };
        if code.opts.contains_template_syntax(code.slice_since(start)) {
          nodes.push(NodeData::Opaque {
            raw_source: code.slice_since(start).to_vec(),
          });
        } else {
          nodes.push(node);
        }
      }
      Comment => {
        // Removing a comment between raw text tokens can create a new entity
        // or HTML tag (e.g. `&am<!-- -->p;`). Only remove at a safe boundary.
        let safe_before = match nodes.last() {
          None | Some(NodeData::Element { .. }) => true,
          Some(NodeData::Opaque { raw_source }) => raw_source.ends_with(b">"),
          _ => false,
        };
        let start = code.take_checkpoint();
        let comment = parse_comment(code);
        if safe_before {
          nodes.push(comment);
        } else {
          nodes.push(NodeData::Opaque {
            raw_source: code.slice_since(start).to_vec(),
          });
        }
      }
      OpaqueBraceBrace | OpaqueBraceHash | OpaqueBracePercent | OpaqueChevronPercent => {
        let start = code.take_checkpoint();
        let raw_block =
          code.opts.treat_brace_as_opaque && brace_directive_len(code.as_slice(), b"raw").is_some();
        code.shift_template();
        let token = code.slice_since(start);
        if raw_block {
          // A raw block's literal output can open preformatted, rawtext or foreign markup, or
          // leave a start tag open. Retain what follows in that case, without interpreting or
          // rewriting the raw body itself.
          uncertain_whitespace_sensitive |=
            opens_any_tag(token, WHITESPACE_SENSITIVE_TAGS) || leaves_tag_open(token);
          uncertain_foreign |= opens_any_tag(token, &[b"svg", b"math"]);
        } else if matches!(typ, OpaqueBracePercent | OpaqueChevronPercent)
          && code.opts.contains_branching_directive(token)
        {
          // A branch may close a sensitive or foreign element on only one path. Preserve
          // subsequent text rather than guessing which path owns each token.
          uncertain_foreign |= foreign_depth != 0;
          uncertain_whitespace_sensitive |= pre_depth != 0 || code_depth != 0;
        }
        nodes.push(NodeData::Opaque {
          raw_source: token.to_vec(),
        });
      }
      IgnoredTag | MalformedLeftChevronSlash | OmittedClosingTag => unreachable!(),
    }
  }
  ParsedContent {
    children: nodes,
    closing_tag_omitted: true,
  }
}

// Use empty slice for `grandparent` or `parent` if none.
pub fn parse_content(
  code: &mut Code,
  ns: Namespace,
  grandparent: &[u8],
  parent: &[u8],
) -> ParsedContent {
  // We assume the closing tag has been omitted until we see one explicitly before EOF (or it has been omitted as per the spec).
  let mut closing_tag_omitted = true;
  let mut nodes = Vec::<NodeData>::new();
  let matcher = content_type_matcher(code);
  loop {
    let (text_len, mut typ) = match matcher.0.find(code.as_slice()) {
      Some(m) => (m.start(), matcher.1[m.pattern()]),
      None => (code.rem(), Text),
    };
    // Due to dropped malformed code, it's possible for two or more text nodes to be contiguous. Ensure they always get merged into one.
    // NOTE: Even though bangs/comments/etc. have no effect on layout, they still split text (e.g. `&am<!-- -->p`).
    if text_len > 0 {
      let text = decode_entities(code.slice_and_shift(text_len), false);
      match nodes.last_mut() {
        Some(NodeData::Text { value }) => value.extend_from_slice(&text),
        _ => nodes.push(NodeData::Text { value: text }),
      };
    };
    // Check using Parsing.md tag rules.
    #[allow(clippy::if_same_then_else)] // For readability.
    if typ == OpeningTag || typ == ClosingTag {
      let name = peek_tag_name(code);
      if typ == OpeningTag {
        debug_assert!(!name.is_empty());
        if can_omit_as_before(parent, &name) {
          // The upcoming opening tag implicitly closes the current element e.g. `<tr><td>(current position)<td>`.
          typ = OmittedClosingTag;
        };
      } else if name.is_empty() {
        // Malformed code, drop until and including next `>`.
        typ = MalformedLeftChevronSlash;
      } else if grandparent == name.as_slice() && can_omit_as_last_node(grandparent, parent) {
        // The upcoming closing tag implicitly closes the current element e.g. `<tr><td>(current position)</tr>`.
        // This DOESN'T handle when grandparent doesn't exist (represented by an empty slice). However, in that case it's irrelevant, as it would mean we would be at EOF, and our parser simply auto-closes everything anyway. (Normally we'd have to determine if `<p>Hello` is an error or allowed.)
        typ = OmittedClosingTag;
      } else if VOID_TAGS.contains(name.as_slice()) {
        // Closing tag for void element, drop.
        typ = IgnoredTag;
      } else if parent.is_empty() || parent != name.as_slice() {
        // Closing tag mismatch, drop.
        typ = IgnoredTag;
      };
      typ = maybe_ignore_html_head_body(code, typ, parent, &name);
    };
    match typ {
      Text => break,
      OpeningTag => nodes.push(parse_element(code, ns, parent)),
      ClosingTag => {
        closing_tag_omitted = false;
        break;
      }
      Instruction => nodes.push(parse_instruction(code)),
      Bang => nodes.push(parse_bang(code)),
      Comment => nodes.push(parse_comment(code)),
      Doctype => nodes.push(parse_doctype(code)),
      MalformedLeftChevronSlash => code.shift(match memchr::memchr(b'>', code.as_slice()) {
        Some(m) => m + 1,
        None => code.rem(),
      }),
      OmittedClosingTag => {
        closing_tag_omitted = true;
        break;
      }
      IgnoredTag => drop(parse_tag(code)),
      OpaqueBraceBrace | OpaqueBraceHash | OpaqueBracePercent | OpaqueChevronPercent => {
        let start = code.take_checkpoint();
        code.shift_template();
        nodes.push(NodeData::Opaque {
          raw_source: code.slice_since(start).to_vec(),
        });
      }
    };
  }
  ParsedContent {
    children: nodes,
    closing_tag_omitted,
  }
}
