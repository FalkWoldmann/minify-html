use crate::cfg::Cfg;
use crate::minify;
use minify_html_common::tests::create_common_css_test_data;
use minify_html_common::tests::create_common_js_test_data;
use minify_html_common::tests::create_common_noncompliant_test_data;
use minify_html_common::tests::create_common_test_data;
use std::str::from_utf8;

pub fn eval_with_cfg(src: &'static [u8], expected: &'static [u8], cfg: &Cfg) {
  let min = minify(&src, cfg);
  assert_eq!(from_utf8(&min).unwrap(), from_utf8(expected).unwrap(),);
}

pub fn eval_with_noncompliant(src: &'static [u8], expected: &'static [u8]) {
  let mut cfg = Cfg::new();
  cfg.enable_possibly_noncompliant();
  eval_with_cfg(src, expected, &cfg)
}

pub fn eval_with_js_min(src: &'static [u8], expected: &'static [u8]) -> () {
  let mut cfg = Cfg::new();
  cfg.minify_js = true;
  eval_with_cfg(src, expected, &cfg);
}

pub fn eval_with_css_min(src: &'static [u8], expected: &'static [u8]) -> () {
  let mut cfg = Cfg::new();
  cfg.minify_css = true;
  eval_with_cfg(src, expected, &cfg);
}

pub fn eval(src: &'static [u8], expected: &'static [u8]) {
  let mut cfg = Cfg::new();
  // Most common tests assume the following minifications aren't done.
  cfg.keep_html_and_head_opening_tags = true;
  cfg.allow_optimal_entities = true;
  eval_with_cfg(src, expected, &cfg);
}

// NOTE: This is different to `eval` as that enables `keep_html_and_head_opening_tags`.
fn eval_without_keep_html_head(src: &'static [u8], expected: &'static [u8]) -> () {
  eval_with_cfg(src, expected, &Cfg::new());
}

#[test]
fn test_common() {
  for (a, b) in create_common_test_data() {
    eval(a, b);
  }
  for (a, b) in create_common_noncompliant_test_data() {
    eval_with_noncompliant(a, b);
  }
  for (a, b) in create_common_css_test_data() {
    eval_with_css_min(a, b);
  }
  for (a, b) in create_common_js_test_data() {
    eval_with_js_min(a, b);
  }
}

#[test]
fn test_keep_ssi_comments() {
  eval(b"<!--#include >", b"");
  let mut cfg = Cfg::default();
  cfg.keep_ssi_comments = true;
  eval_with_cfg(b"<!--#include >", b"<!--#include >", &cfg);
}

#[test]
fn test_keep_input_type_text_attr() {
  eval(b"<input type=\"text\">", b"<input>");
  let mut cfg = Cfg::default();
  cfg.keep_input_type_text_attr = true;
  eval_with_cfg(b"<input type=\"TExt\">", b"<input type=text>", &cfg);
}

#[test]
fn test_preserve_template_brace_syntax() {
  eval_with_js_min(
    b"<p> {{   hello    world! %}  {%}{#} echo '  </p><P><script>  let x = 1; //'  }} </p>",
    b"<p>{{ hello world! %} {%}{#} echo '<p><script>let x=1;",
  );
  let mut cfg = Cfg::default();
  cfg.preserve_brace_template_syntax = true;
  eval_with_cfg(
    b"<p> {{   hello    world! %}  {%}{#} echo '  </p><P><script>  let x = 1; //'  }} </p>",
    b"<p> {{   hello    world! %}  {%}{#} echo '  </p><P><script>  let x = 1; //'  }} </p>",
    &cfg,
  );
  eval_with_cfg(
    b"<p> {%   hello    world! %}  {%}{#} echo '  </p><P><script>  let x = 1; //'  %} </p>",
    b"<p> {%   hello    world! %}  {%}{#} echo '  </p><P><script>  let x = 1; //'  %} </p>",
    &cfg,
  );
  eval_with_cfg(
    b"<p> {#   hello    world! #}  {#}{# echo '  </p><P><script>  let x = 1; //'  #} </p>",
    b"<p> {#   hello    world! #}  {#}{# echo '  </p><P><script>  let x = 1; //'  #} </p>",
    &cfg,
  );
}

#[test]
fn should_preserve_askama_control_flow_inside_start_tags() {
  let cfg = Cfg {
    preserve_brace_template_syntax: true,
    ..Cfg::default()
  };
  let source = br#"<button
{% match legacy_sheet_url %}
{% when Some with (sheet_url) %}
    data-url='{{ sheet_url | base64_encode }}'
{% when None %}
    data-benefit-id="{{ benefit_id }}"
{% endmatch %}
    aria-label="Details zu {{ campaign_title }}"
>{{ text }}</button>"#;
  eval_with_cfg(source, source, &cfg);
}

#[test]
fn should_keep_dynamic_attribute_values_quoted() {
  let cfg = Cfg {
    preserve_brace_template_syntax: true,
    ..Cfg::default()
  };
  eval_with_cfg(
    br#"<span title="{{title}}">{{ text }}</span>"#,
    br#"<span title="{{title}}">{{ text }}</span>"#,
    &cfg,
  );
}

#[test]
fn should_preserve_quotes_and_chevrons_inside_template_expressions() {
  let cfg = Cfg {
    preserve_brace_template_syntax: true,
    ..Cfg::default()
  };
  let source = br#"<a title="{{ choose("a>b", "it's quoted") }}" data-x="last">{{ text }}</a>"#;
  eval_with_cfg(source, source, &cfg);
}

#[test]
fn should_preserve_askama_whitespace_controls_and_repeated_blocks() {
  let cfg = Cfg {
    preserve_brace_template_syntax: true,
    ..Cfg::default()
  };
  let source = br#"<div {%~ if first ~%}data-first="yes"{%~ endif ~%} {%~ if second ~%}data-second="yes"{%~ endif ~%}>{{ text }}</div>"#;
  eval_with_cfg(source, source, &cfg);
}

#[test]
fn should_preserve_esi_template_attributes_without_swallowing_siblings() {
  let cfg = Cfg {
    preserve_brace_template_syntax: true,
    preserve_esi_tags: true,
    ..Cfg::default()
  };
  let source = br#"<esi:include src="{{url}}" /><span>after</span>"#;
  eval_with_cfg(source, source, &cfg);
}

#[test]
fn should_still_minify_static_tags_with_template_preservation_enabled() {
  let cfg = Cfg {
    preserve_brace_template_syntax: true,
    ..Cfg::default()
  };
  eval_with_cfg(
    br#"<button title="someValue" data-id="42">  text  </button>"#,
    br#"<button data-id=42 title=someValue>text</button>"#,
    &cfg,
  );
  eval_with_cfg(
    br#"<p>{{name}}</p><button title="someValue" data-id="42">  text  </button><!-- remove -->"#,
    br#"<p>{{name}}</p><button data-id=42 title=someValue>  text  </button>"#,
    &cfg,
  );
}

#[test]
fn should_preserve_an_unterminated_attribute_template_without_panicking() {
  let cfg = Cfg {
    preserve_brace_template_syntax: true,
    ..Cfg::default()
  };
  eval_with_cfg(b"<div {%", b"<div {%", &cfg);
}

#[test]
fn should_preserve_template_strings_containing_closing_delimiters() {
  let cfg = Cfg {
    preserve_brace_template_syntax: true,
    ..Cfg::default()
  };
  let source = br###"<a title="{{ choose("}}", "\"quoted\"", "a>b") }}">text</a>"###;
  eval_with_cfg(source, source, &cfg);
}

#[test]
fn should_preserve_nested_comments_inside_attributes() {
  let cfg = Cfg {
    preserve_brace_template_syntax: true,
    ..Cfg::default()
  };
  let source =
    br#"<button {# outer {# nested #} "quoted" > comment #} data-value="{{value}}">text</button>"#;
  eval_with_cfg(source, source, &cfg);
}

#[test]
fn should_preserve_braced_expressions_inside_attributes() {
  let cfg = Cfg {
    preserve_brace_template_syntax: true,
    ..Cfg::default()
  };
  let source = br#"<div data-value="{{ {"key": "value"} }}">text</div>"#;
  eval_with_cfg(source, source, &cfg);
}

#[test]
fn should_preserve_chevron_templates_in_unquoted_attribute_values() {
  let cfg = Cfg {
    preserve_chevron_percent_template_syntax: true,
    ..Cfg::default()
  };
  let source = br#"<div title=<%= choose("a>b", "text") %> data-other="value">text</div>"#;
  eval_with_cfg(source, source, &cfg);
}

#[test]
fn should_keep_svg_start_and_end_tag_case_consistent() {
  let cfg = Cfg {
    preserve_brace_template_syntax: true,
    ..Cfg::default()
  };
  let source = br#"<svg><linearGradient id="{{id}}"></linearGradient></svg>"#;
  eval_with_cfg(source, source, &cfg);
}

#[test]
fn should_not_guess_a_script_type_selected_by_a_template() {
  let cfg = Cfg {
    preserve_brace_template_syntax: true,
    minify_js: true,
    ..Cfg::default()
  };
  let source = br#"<script {% if json %}type="application/json"{% else %}type="module"{% endif %}>[1, 2]</script>"#;
  eval_with_cfg(source, source, &cfg);
}

#[test]
fn should_preserve_quoted_delimiters_and_braced_expressions_in_content() {
  let cfg = Cfg {
    preserve_brace_template_syntax: true,
    ..Cfg::default()
  };
  eval_with_cfg(
    br###"<div>{{ choose("}}", "\"<B title='x'>  kept  </B>\"", {"key": "value"}) }}{% if choose("%}", "<B title='y'>") %}yes{% endif %}</div><a title="value">next</a>"###,
    br###"<div>{{ choose("}}", "\"<B title='x'>  kept  </B>\"", {"key": "value"}) }}{% if choose("%}", "<B title='y'>") %}yes{% endif %}</div><a title=value>next</a>"###,
    &cfg,
  );
}

#[test]
fn should_preserve_nested_template_comments_in_content() {
  let cfg = Cfg {
    preserve_brace_template_syntax: true,
    ..Cfg::default()
  };
  eval_with_cfg(
    br#"<div>{# outer {# inner #} <B title="literal">  text  </B> #}{{value}}</div><span title="after">after</span>"#,
    br#"<div>{# outer {# inner #} <B title="literal">  text  </B> #}{{value}}</div><span title=after>after</span>"#,
    &cfg,
  );
}

#[test]
fn should_keep_split_branch_opening_and_closing_tags_in_order() {
  let cfg = Cfg {
    preserve_brace_template_syntax: true,
    ..Cfg::default()
  };
  eval_with_cfg(
    br#"{% if first %}<section class="first">{% else %}<aside class="other">{% endif %}body{% if first %}</section>{% else %}</aside>{% endif %}<a title="after">after</a>"#,
    br#"{% if first %}<section class=first>{% else %}<aside class=other>{% endif %}body{% if first %}</section>{% else %}</aside>{% endif %}<a title=after>after</a>"#,
    &cfg,
  );
  // Optional closing tags cannot be omitted based on a combined branch tree.
  let optional_tags = b"<ul>{% if first %}<li>{% else %}<li>{% endif %}one{% if first %}</li>{% else %}</li>{% endif %}<li>two</li></ul>";
  eval_with_cfg(optional_tags, optional_tags, &cfg);
  let document_branches = b"{% if first %}<html><head>{% else %}<html><head>{% endif %}<title>{{title}}</title></head><body>body</body></html>";
  eval_with_cfg(document_branches, document_branches, &cfg);
}

#[test]
fn should_keep_inline_text_spacing_next_to_dynamic_expressions() {
  let cfg = Cfg {
    preserve_brace_template_syntax: true,
    ..Cfg::default()
  };
  let source = b"<div><span>Hello</span> {{ name }}  {{ more }}</div><p>  {{- controlled -}}  {%~ if visible ~%} visible {%~ endif ~%}</p>";
  eval_with_cfg(source, source, &cfg);
}

#[test]
fn should_preserve_raw_block_literal_bytes_and_resume_html_minification() {
  let cfg = Cfg {
    preserve_brace_template_syntax: true,
    ..Cfg::default()
  };
  eval_with_cfg(
    br###"<div title="literal">{%~ raw ~%}<B title="x &amp;">  {{ "}}" }} {% invalid " {% endrawx %}  </B>{%~ endraw ~%}</div><img src="/after">"###,
    br###"<div title=literal>{%~ raw ~%}<B title="x &amp;">  {{ "}}" }} {% invalid " {% endrawx %}  </B>{%~ endraw ~%}</div><img src=/after>"###,
    &cfg,
  );
}

#[test]
fn should_preserve_template_strings_in_rcdata_and_rawtext_bodies() {
  let cfg = Cfg {
    preserve_brace_template_syntax: true,
    minify_js: true,
    minify_css: true,
    ..Cfg::default()
  };
  eval_with_cfg(
    br###"<title>{{ choose("</title>", "&amp; <b>x</b>") }}</title><textarea>{{ choose("</textarea>", "&amp; <b>x</b>") }}</textarea><script>{{ choose("</script>", "&amp; <b>x</b>") }}</script><style>{{ choose("</style>", "&amp; <b>x</b>") }}</style><span title="after">after</span>"###,
    br###"<title>{{ choose("</title>", "&amp; <b>x</b>") }}</title><textarea>{{ choose("</textarea>", "&amp; <b>x</b>") }}</textarea><script>{{ choose("</script>", "&amp; <b>x</b>") }}</script><style>{{ choose("</style>", "&amp; <b>x</b>") }}</style><span title=after>after</span>"###,
    &cfg,
  );
}

#[test]
fn should_preserve_template_expressions_inside_html_comments() {
  let cfg = Cfg {
    preserve_brace_template_syntax: true,
    ..Cfg::default()
  };
  eval_with_cfg(
    br###"<!-- {{ choose("-->", "<B title='literal'>") }} --><span title="after">after</span>"###,
    br###"<!-- {{ choose("-->", "<B title='literal'>") }} --><span title=after>after</span>"###,
    &cfg,
  );
  let split_entity = b"<p>{{name}}&am<!-- boundary -->p;</p>";
  eval_with_cfg(split_entity, split_entity, &cfg);
  let whitespace_control = b"<p>&am <!-- boundary -->{{- suffix}}</p>";
  eval_with_cfg(whitespace_control, whitespace_control, &cfg);
}

#[test]
fn should_preserve_empty_esi_values_comments_and_following_siblings() {
  let mut cfg = Cfg {
    preserve_esi_tags: true,
    ..Cfg::default()
  };
  eval_with_cfg(
    br#"<!--esi <esi:include src=""/> --><esi:include src="" alt="" /><span>after</span>"#,
    br#"<!--esi <esi:include src=""/> --><esi:include alt="" src=""/><span>after</span>"#,
    &cfg,
  );
  cfg.preserve_brace_template_syntax = true;
  eval_with_cfg(
    br#"<esi:include src="" /><span>{{after}}</span>"#,
    br#"<esi:include src=""/><span>{{after}}</span>"#,
    &cfg,
  );
}

#[test]
fn test_preserve_template_chevron_percent_syntax() {
  let mut cfg = Cfg::default();
  cfg.preserve_chevron_percent_template_syntax = true;
  eval_with_cfg(
    b"<p> <%   hello    world! #}  {#}{# echo '  </p><P><script>  let x = 1; //'  %> </p>",
    b"<p> <%   hello    world! #}  {#}{# echo '  </p><P><script>  let x = 1; //'  %> </p>",
    &cfg,
  );
}

#[test]
fn test_preserve_esi_tags() {
  let mut cfg = Cfg::default();
  cfg.preserve_esi_tags = true;

  // Without the flag, the trailing `/` is not a self-closing indicator, so following siblings are
  // incorrectly parsed as children of the ESI element.
  eval_with_cfg(
    br#"<div><esi:include src="/a" /><span>after</span></div>"#,
    br#"<div><esi:include src=/a><span>after</span>"#,
    &Cfg::default(),
  );
  eval_with_cfg(
    br#"<div><esi:include src="/a" /><span>after</span></div>"#,
    br#"<div><esi:include src="/a"/><span>after</span></div>"#,
    &cfg,
  );

  // Applies to every tag in the `esi:` namespace, including nested control flow.
  eval_with_cfg(
    br#"<esi:comment text="hello" /><p>p</p>"#,
    br#"<esi:comment text="hello"/><p>p"#,
    &cfg,
  );
  eval_with_cfg(
    br#"<esi:choose><esi:when test="a"><esi:include src="/a" /></esi:when><esi:otherwise><esi:include src="/b" /></esi:otherwise></esi:choose>"#,
    br#"<esi:choose><esi:when test="a"><esi:include src="/a"/></esi:when><esi:otherwise><esi:include src="/b"/></esi:otherwise></esi:choose>"#,
    &cfg,
  );

  // Attribute values stay quoted so that XML-based ESI processors can still parse them.
  eval_with_cfg(
    br#"<esi:include src="/a" alt="/b" />"#,
    br#"<esi:include alt="/b" src="/a"/>"#,
    &cfg,
  );

  // Elements with an explicit closing tag keep their children, as before. Quoting is only forced on
  // the ESI element's own attributes; nested HTML is minified as usual.
  eval_with_cfg(
    br#"<esi:remove><a href="/a">a</a></esi:remove>"#,
    br#"<esi:remove><a href=/a>a</a></esi:remove>"#,
    &cfg,
  );

  // Non-ESI tags are unaffected: a trailing `/` on an HTML element is still not self-closing.
  eval_with_cfg(
    br#"<div><img src="/a" /><span>after</span></div>"#,
    br#"<div><img src=/a><span>after</span></div>"#,
    &cfg,
  );
  eval_with_cfg(
    br#"<div><p /><span>after</span></div>"#,
    br#"<div><p><span>after</span></div>"#,
    &cfg,
  );
}

#[test]
fn test_minification_of_doctype() {
  let mut cfg = Cfg::new();
  cfg.minify_doctype = true;
  eval_with_cfg(b"<!DOCTYPE html><div>", b"<!doctypehtml><div>", &cfg);
  eval_with_cfg(
    b"<!DOCTYPE html SYSTEM 'about:legacy-compat'><div>",
    b"<!doctypehtml SYSTEM 'about:legacy-compat'><div>",
    &cfg,
  );
}

#[test]
fn test_removal_of_empty_closing_tag() {
  eval(b"<body><p>1</><p>2</body>", b"<body><p>1<p>2");
}

#[test]
fn test_parsing_extra_head_tag() {
  // Extra `<head>` in `<label>` should be dropped, so whitespace around `<head>` should be joined and therefore trimmed due to `<label>` whitespace rules.
  eval(
    b"<html><head><meta><head><link><head><body><label>  <pre> </pre> <head>  </label>",
    b"<html><head><meta><link><body><label><pre> </pre></label>",
  );
  // Same as above except it's a `</head>`, which should get reinterpreted as a `<head>`.
  eval(
    b"<html><head><meta><head><link><head><body><label>  <pre> </pre> </head>  </label>",
    b"<html><head><meta><link><body><label><pre> </pre></label>",
  );
  // `<head>` gets implicitly closed by `<body>`, so any following `</head>` should be ignored. (They should be anyway, since `</head>` would not be a valid closing tag.)
  eval(
    b"<html><head><body><label> </head> </label>",
    b"<html><head><body><label></label>",
  );
}

#[test]
fn test_removal_of_html_and_head_opening_tags() {
  // Even though `<head>` is dropped, it's still parsed, so its content is still subject to `<head>` whitespace minification rules.
  eval_without_keep_html_head(
    b"<!DOCTYPE html><html><head>  <meta> <body>",
    b"<!doctype html><meta><body>",
  );
  // The tag should not be dropped if it has attributes.
  eval_without_keep_html_head(
    b"<!DOCTYPE html><html lang=en><head>  <meta> <body>",
    b"<!doctype html><html lang=en><meta><body>",
  );
  // The tag should be dropped if it has no attributes after minification.
  eval_without_keep_html_head(
    b"<!DOCTYPE html><html style='  '><head>  <meta> <body>",
    b"<!doctype html><meta><body>",
  );
}

#[test]
fn test_unmatched_closing_tag() {
  eval(b"Hello</p>Goodbye", b"HelloGoodbye");
  eval(b"Hello<br></br>Goodbye", b"Hello<br>Goodbye");
  eval(b"<div>Hello</p>Goodbye", b"<div>HelloGoodbye");
  eval(b"<ul><li>a</p>", b"<ul><li>a");
  eval(b"<ul><li><rt>a</p>", b"<ul><li><rt>a");
  eval(
    b"<html><head><body><ul><li><rt>a</p>",
    b"<html><head><body><ul><li><rt>a",
  );
}

#[test]
// NOTE: Keep inputs in sync with onepass variant. Outputs are different as main variant reorders attributes.
fn test_space_between_attrs_minification() {
  eval_with_noncompliant(
    b"<div a=\" \" b=\" \"></div>",
    b"<div a=\" \"b=\" \"></div>",
  );
  eval_with_noncompliant(b"<div a=' ' b=\" \"></div>", b"<div a=\" \"b=\" \"></div>");
  eval_with_noncompliant(
    b"<div a=&#x20 b=\" \"></div>",
    b"<div a=\" \"b=\" \"></div>",
  );
  eval_with_noncompliant(b"<div a=\"1\" b=\" \"></div>", b"<div b=\" \"a=1></div>");
  eval_with_noncompliant(b"<div a='1' b=\" \"></div>", b"<div b=\" \"a=1></div>");
  eval_with_noncompliant(b"<div a=\"a\"b=\"b\"></div>", b"<div a=a b=b></div>");
}

#[test]
fn test_attr_whatwg_unquoted_value_minification() {
  eval(b"<a b==></a>", br#"<a b="="></a>"#);
  eval(br#"<a b=`'"<<==/`/></a>"#, br#"<a b="`'&#34<<==/`/"></a>"#);
}

#[test]
fn test_alt_attr_minification() {
  eval(br#"<img alt="  ">"#, br#"<img alt="  ">"#);
  eval(br#"<img alt=" ">"#, br#"<img alt=" ">"#);
  eval(br#"<img alt="">"#, br#"<img alt>"#);
  eval(br#"<img alt=''>"#, br#"<img alt>"#);
  eval(br#"<img alt>"#, br#"<img alt>"#);
  eval(br#"<x-any-tag alt>"#, br#"<x-any-tag alt>"#);
}

#[test]
fn test_viewport_attr_minification() {
  eval_with_noncompliant(
    b"<meta name=viewport content='width=device-width, initial-scale=1'>",
    b"<meta content=width=device-width,initial-scale=1 name=viewport>",
  );
  eval(
    b"<meta name=viewport content='width=device-width, initial-scale=1'>",
    br#"<meta content="width=device-width,initial-scale=1" name=viewport>"#,
  );
}

#[test]
fn test_style_attr_minification() {
  eval_with_css_min(
    br#"<div style="color: yellow;"></div>"#,
    br#"<div style=color:#ff0></div>"#,
  );
  // `style` attributes are removed if fully minified away.
  eval_with_css_min(br#"<div style="  /*  */   "></div>"#, br#"<div></div>"#);
}
