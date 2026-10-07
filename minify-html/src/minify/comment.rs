use crate::cfg::Cfg;

pub fn minify_comment(cfg: &Cfg, out: &mut Vec<u8>, code: &[u8], ended: bool) {
  let is_ssi = code.starts_with(b"#");
  let is_esi = cfg.preserve_esi_tags && code.starts_with(b"esi");
  let has_template = code.windows(2).any(|seq| {
    (cfg.preserve_brace_template_syntax && matches!(seq, b"{{" | b"{%" | b"{#"))
      || (cfg.preserve_chevron_percent_template_syntax && seq == b"<%")
  });
  if cfg.keep_comments || (is_ssi && cfg.keep_ssi_comments) || is_esi || has_template {
    out.extend_from_slice(b"<!--");
    out.extend_from_slice(code);
    if ended {
      out.extend_from_slice(b"-->");
    };
  };
}
