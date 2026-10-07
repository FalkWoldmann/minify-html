use crate::ast::ElementClosingTag;
use crate::ast::NodeData;
use crate::entity::decode::decode_entities;
use crate::parse::bang::parse_bang;
use crate::parse::comment::parse_comment;
use crate::parse::content::ContentType::*;
use crate::parse::doctype::parse_doctype;
use crate::parse::element::is_esi_tag;
use crate::parse::element::parse_element;
use crate::parse::element::parse_tag;
use crate::parse::element::peek_tag_name;
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

// A source template is not an HTML tree: mutually exclusive branches can open
// and close different elements. Keep its token order and all text boundaries,
// while still applying the ordinary minifier to literal HTML opening tags.
pub fn parse_template_content(code: &mut Code) -> ParsedContent {
  let mut nodes = Vec::new();
  let matcher = content_type_matcher(code);
  let mut foreign_depth = 0usize;
  let mut uncertain_foreign = false;
  while !code.at_end() {
    let (text_len, typ) = match matcher.0.find(code.as_slice()) {
      Some(m) => (m.start(), matcher.1[m.pattern()]),
      None => (code.rem(), Text),
    };
    if text_len != 0 {
      nodes.push(NodeData::Opaque {
        raw_source: code.copy_and_shift(text_len),
      });
    }
    match typ {
      Text => break,
      OpeningTag => {
        let start = code.take_checkpoint();
        let mut tag = parse_tag(code);
        let foreign_root = matches!(tag.name.as_slice(), b"svg" | b"math");
        let foreign = foreign_depth != 0 || uncertain_foreign || foreign_root;
        if foreign && code.opts.contains_template_syntax(code.slice_since(start)) {
          // A directive in foreign markup can change the namespace in later
          // branches. Retain subsequent headers instead of guessing a context.
          uncertain_foreign = true;
        }
        if (foreign || matches!(tag.name.as_slice(), b"html" | b"head"))
          && tag.raw_opening_tag.is_none()
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
        let body = if special_content && closing_tag == ElementClosingTag::Omitted {
          Some(code.slice_and_shift_special_content(&tag.name).to_vec())
        } else if plaintext {
          Some(code.copy_and_shift(code.rem()))
        } else {
          None
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
        if let Some(raw_source) = body {
          nodes.push(NodeData::Opaque { raw_source });
        }
      }
      ClosingTag => {
        let start = code.take_checkpoint();
        let tag = parse_tag(code);
        if matches!(tag.name.as_slice(), b"svg" | b"math") {
          foreign_depth = foreign_depth.saturating_sub(1);
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
        if foreign_depth != 0 {
          uncertain_foreign = true;
        }
        let start = code.take_checkpoint();
        code.shift_template();
        nodes.push(NodeData::Opaque {
          raw_source: code.slice_since(start).to_vec(),
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
