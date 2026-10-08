<h1>
minify-html
<img width="24" src="https://wilsonl.in/minify-html/icon/rust.png">
</h1>

A Rust HTML minifier meticulously optimised for speed and effectiveness.

- Advanced minification strategy beats other minifiers in effectiveness while still being much faster.
- Handles [invalid HTML](./notes/Parsing.md) and [templating syntax](#templating-syntax).
- Uses [oxc](https://github.com/oxc-project/oxc) and [lightningcss](https://github.com/parcel-bundler/lightningcss) for super fast JS and CSS minification.

View the [changelog](./CHANGELOG.md) to see the latest updates.

## Performance

Comparison with [html-minifier](https://github.com/kangax/html-minifier) and [minimize](https://github.com/Swaagie/minimize), run on the top web pages. [See the breakdown here.](./bench)

<img width="400" alt="Chart showing speed of HTML minifiers" src="./bench/graphs/average-speeds.png"><img width="400" alt="Chart showing compression of HTML minifiers" src="./bench/graphs/average-sizes.png">

The [onepass](https://github.com/wilsonzlin/minify-html/tree/master/minify-html-onepass) variant is even more optimised for speed. See its [README](https://github.com/wilsonzlin/minify-html/tree/master/minify-html-onepass) for more details.

## Compatibility and usage

<details>
<summary><img width="24" src="https://wilsonl.in/minify-html/icon/rust.png"> <strong>Rust</strong></summary>

### Get

```toml
[dependencies]
minify-html = "0.18.1"
```

### Use

Check out the [docs](https://docs.rs/minify-html) for API and usage examples.

</details>

## Templating syntax

minify-html can parse and preserve `{{`/`{%`/`{#` and `<%` syntax in the source code, which allows minification of many HTML templates written for most engines like Pebble, Mustache, Django, Go, Jinja, Twix, Nunjucks, Handlebars, Sailfish, JSP, EJS, and ERB. Look for the `preserve_*_template_syntax` Cfg options.

PHP blocks (`<?php` or `<?=`) also happen to be processing instructions, which are preserved by default.

When an enabled template delimiter occurs, the source is parsed as tokens rather than as a fully rendered HTML tree. Quoted delimiter strings, braced expressions, nested `{# ... #}` comments and `{% raw %}` blocks are preserved. Template-bearing start tags retain attribute order, quotes, quoted value bytes and token internals, but formatting whitespace between attributes is compacted to a separator. Explicit closing tags remain intact. Normal HTML text whitespace is collapsed, and whitespace-only runs between known layout boundaries are removed; inline/custom-element and expression boundaries retain their separators.

Preformatted/raw content, RCDATA/rawtext bodies and foreign text retain their whitespace. Conditional sensitive/foreign boundaries deliberately retain uncertain later whitespace rather than infer a rendered branch tree. Literal HTML opening tags and safe comments/doctypes are still minified, and so are literal `<script>` and `<style>` bodies when `minify_js`/`minify_css` are enabled. A single enabled template delimiter switches the whole document to this mode, which never omits closing tags; inputs without one keep the normal HTML minifier.

Each file is compacted as if it renders in normal flow. If a child `{% block %}`, a macro or an included file is rendered inside a parent's `<pre>`, `<textarea>` or `<code>`, its whitespace will still be collapsed, so keep such content in the same file as the preformatted element or don't minify that template.

For Askama build-time preprocessing with ESI, enable `preserve_brace_template_syntax` and `preserve_esi_tags`, keep closing and HTML/head opening tags, and leave possibly-noncompliant minification disabled.

Rendered output usually has no template syntax left, so it goes through the normal HTML minifier, which drops whitespace-only text inside containers like `div`. That joins `<span>Hello</span> <span>world</span>` into `Helloworld`. To minify rendered HTML with the same whitespace rules as its template, enable `preserve_inline_whitespace`. It uses this mode whether or not template syntax is found.

## Edge Side Includes

[ESI](https://www.w3.org/TR/esi-lang/) markup is made up of XML elements in the `esi:` namespace that an edge proxy (e.g. Varnish, Akamai, Fastly) resolves before the response ever reaches a browser. Because it's XML and not HTML, an empty element is written with self-closing syntax:

```html
<div><esi:include src="/a" /><span>after</span></div>
```

HTML has no self-closing syntax for non-[void](https://html.spec.whatwg.org/multipage/syntax.html#void-elements) elements, so by default the trailing `/` is ignored and `<span>after</span>` is parsed as a *child* of the unclosed `<esi:include>`, mangling the document structure.

Set the `preserve_esi_tags` Cfg option to parse tags in the `esi:` namespace as XML instead. Self-closing tags are then kept as empty elements, and attribute values on ESI tags are kept exactly as written (entities included) and always quoted, so that XML-based ESI processors can still parse them:

```html
<div><esi:include src="/a"/><span>after</span></div>
```

Since XML has no unquoted values, a slash right before the `>` of an ESI tag also self-closes it, so `<esi:include src=/a/>` becomes `<esi:include src="/a"/>`. Note that this only applies to tags that are explicitly self-closed or explicitly closed; `<esi:include src="/a">` with no closing tag is still parsed as an HTML opening tag.

## Minification

### Spec compliance

WHATWG is the current HTML standard and [obsoletes all previous standards](https://www.w3.org/html/). WHATWG lists suggested validators [here](https://whatwg.org/validator/).

To minify even further, it's possible to enable options that may output HTML that doesn't fully pass validation, but is still interpreted and rendered correctly according to the [WHATWG parsing specification](https://html.spec.whatwg.org/multipage/parsing.html), which major browser engines (Firefox, Chrome, Safari) implement. Refer to these options:

- `allow_noncompliant_unquoted_attribute_values`
- `allow_optimal_entities`
- `allow_removing_spaces_between_attributes`
- `minify_doctype`

`Cfg::enable_possibly_noncompliant` enables all of these at once.

### Whitespace

minify-html has advanced context-aware whitespace minification that does things such as:

- Leave whitespace untouched in `pre` and `code`, which are whitespace sensitive.
- Trim and collapse whitespace in content tags, as whitespace is collapsed anyway when rendered.
- Remove whitespace in layout tags, which allows the use of inline layouts while keeping formatted code.

#### Methods

There are three whitespace minification methods. When processing text content, minify-html chooses which ones to use depending on the containing element.

<details>
<summary><strong>Collapse whitespace</strong></summary>

> **Applies to:** any element except [whitespace sensitive](./minify-html-common/src/spec/tag/whitespace.rs) elements.

Reduce a sequence of whitespace characters in text nodes to a single space (U+0020).

<table><thead><tr><th>Before<th>After<tbody><tr><td>

```html
<p>↵
··The·quick·brown·fox↵
··jumps·over·the·lazy↵
··dog.↵
</p>
```

<td>

```html
<p>·The·quick·brown·fox·jumps·over·the·lazy·dog.·</p>
```

</table>
</details>

<details>
<summary><strong>Destroy whole whitespace</strong></summary>

> **Applies to:** any element except [whitespace sensitive](./minify-html-common/src/spec/tag/whitespace.rs), [content](src/spec/tag/whitespace.rs), [content-first](./minify-html-common/src/spec/tag/whitespace.rs), and [formatting](./minify-html-common/src/spec/tag/whitespace.rs) elements.

Remove any text nodes between tags that only consist of whitespace characters.

<table><thead><tr><th>Before<th>After<tbody><tr><td>

```html
<ul>↵
··<li>A</li>↵
··<li>B</li>↵
··<li>C</li>↵
</ul>
```

<td>

```html
<ul>↵
··<li>A</li><li>B</li><li>C</li>↵
</ul>
```

</table>
</details>

<details>
<summary><strong>Trim whitespace</strong></summary>

> **Applies to:** any element except [whitespace sensitive](./minify-html-common/src/spec/tag/whitespace.rs) and [formatting](./minify-html-common/src/spec/tag/whitespace.rs) elements.

Remove any leading/trailing whitespace from any leading/trailing text nodes of a tag.

<table><thead><tr><th>Before<th>After<tbody><tr><td>

```html
<p>↵
··Hey,·I·<em>just</em>·found↵
··out·about·this·<strong>cool</strong>·website!↵
··<sup>[1]</sup>↵
</p>
```

<td>

```html
<p>Hey,·I·<em>just</em>·found↵
··out·about·this·<strong>cool</strong>·website!↵
··<sup>[1]</sup></p>
```

</table>
</details>

#### Element types

minify-html assumes HTML and SVG elements are used in specific ways, based on standards and best practices. By making these assumptions, it can apply optimal whitespace minification strategies. If these assumptions do not hold, consider adjusting the HTML source or turning off whitespace minification.

|Group|Elements|Expected children|
|---|---|---|
|Formatting|`a`, `strong`, [and others](./minify-html-common/src/spec/tag/whitespace.rs)|Formatting elements, text.|
|Content|`h1`, `p`, [and others](./minify-html-common/src/spec/tag/whitespace.rs)|Formatting elements, text.|
|Layout|`div`, `ul`, [and others](./minify-html-common/src/spec/tag/whitespace.rs)|Layout elements, content elements.|
|Content-first|`label`, `li`, [and others](./minify-html-common/src/spec/tag/whitespace.rs)|Like content but could be layout with only one child.|

<details>
<summary><strong>Formatting elements</strong></summary>

> Whitespace is collapsed.

Formatting elements are usually inline elements that wrap around part of some text in a content element, so its whitespace isn't trimmed as they're probably part of the content.

</details>

<details>
<summary><strong>Content elements</strong></summary>

> Whitespace is trimmed and collapsed.

Content elements usually represent a contiguous and complete unit of content such as a paragraph. As such, whitespace is significant but sequences of them are most likely due to formatting.

###### Before

```html
<p>↵
··Hey,·I·<em>just</em>·found↵
··out·about·this·<strong>cool</strong>·website!↵
··<sup>[1]</sup>↵
</p>
```

###### After

```html
<p>Hey,·I·<em>just</em>·found·out·about·this·<strong>cool</strong>·website!·<sup>[1]</sup></p>
```

</details>

<details>
<summary><strong>Layout elements</strong></summary>

> Whitespace is trimmed and collapsed. Whole whitespace is removed.

These elements should only contain other elements and no text. This makes it possible to remove whole whitespace, which is useful when using `display: inline-block` so that whitespace between elements (e.g. indentation) does not alter layout and styling.

###### Before

```html
<ul>↵
··<li>A</li>↵
··<li>B</li>↵
··<li>C</li>↵
</ul>
```

###### After

```html
<ul><li>A</li><li>B</li><li>C</li></ul>
```

</details>

<details>
<summary><strong>Content-first elements</strong></summary>

> Whitespace is trimmed and collapsed.

These elements are usually like content elements but are occasionally used like a layout element with one child. Whole whitespace is not removed as it might contain content, but this is OK for using as layout as there is only one child and whitespace is trimmed.

###### Before

```html
<li>↵
··<article>↵
····<section></section>↵
····<section></section>↵
··</article>↵
</li>
```

###### After

```html
<li><article><section></section><section></section></article></li>
```

</details>

### Tags

[Optional opening and closing tags](https://html.spec.whatwg.org/multipage/syntax.html#syntax-tag-omission) are removed.

### Attributes

Any entities in attribute values are decoded, and then the shortest representation of the value is calculated and used:

- Double quoted, with any `"` encoded.
- Single quoted, with any `'` encoded.
- Unquoted, with `"`/`'` first character (if applicable), any `>`, and any whitespace encoded.

Attributes have their whitespace (after any decoding) trimmed and collapsed when possible.

[Boolean attribute](https://github.com/wilsonzlin/html-data) values are removed.
[Some other attributes](https://github.com/wilsonzlin/html-data) are completely removed if their value is empty or the default value after any processing.

`type` attributes on `script` tags with a value equaling a [JavaScript MIME type](https://mimesniff.spec.whatwg.org/#javascript-mime-type) are removed.

If an attribute value is empty after any processing, everything but the name is completely removed (i.e. no `=`), as an empty attribute is implicitly [the same](https://html.spec.whatwg.org/multipage/syntax.html#attributes-2) as an attribute with an empty string value.

Spaces are removed between attributes when possible.

### Entities

Entities are decoded if they're valid and shorter or equal in length when decoded. UTF-8 sequences that have a shorter entity representation are encoded.

Numeric entities that do not refer to a valid [Unicode Scalar Value](https://www.unicode.org/glossary/#unicode_scalar_value) are replaced with the [replacement character](https://en.wikipedia.org/wiki/Specials_(Unicode_block)#Replacement_character).

Encoding is avoided when possible; for example, `<` are only encoded in content if they are followed by a valid tag name character.
If necessary, the shortest entity representation is chosen.

### Comments

Comments are removed.

### Ignored

Bangs, [processing instructions](https://en.wikipedia.org/wiki/Processing_Instruction), and empty elements are not removed as it is assumed there is a special reason for their declaration.

## Parsing

minify-html can process any HTML, handling all possible syntax (including invalid ones) gracefully like browsers. See [Parsing.md](./notes/Parsing.md) for more details.

## Issues and contributions

Pull requests and any contributions welcome!

If minify-html did something unexpected, misunderstood some syntax, or incorrectly kept/removed some code, [raise an issue](https://github.com/wilsonzlin/minify-html/issues) with some relevant code that can be used to reproduce and investigate the issue.
